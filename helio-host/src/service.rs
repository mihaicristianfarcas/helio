//! HelioHost: the Windows service that keeps a host running, the way
//! SunshineService keeps sunshine.exe running (`tools/sunshinesvc.cpp`).
//!
//! The service runs as LocalSystem in session 0, which has no desktop. It
//! launches `helio-host host` into the active console session with a copy of its own
//! SYSTEM token. That combination is the point:
//! - SYSTEM can open the secure desktop, so the host captures UAC prompts, the
//!   lock screen and the sign-in screen, and injects input into them;
//! - the console session is where the displays are.
//!
//! It relaunches the host when the console session changes (sign-out, fast user
//! switching, a reboot to the sign-in screen) or if the host exits: at once
//! when the host exited to start again (`restart`), else after 3 s.
//!
//! It runs, and launches, only the engine's copy in `C:\Program Files\Helio Host`
//! that `helio-host enable` puts there (`hosting`): never a `helio-host.exe` in a folder
//! a user can write to.

use std::ffi::OsString;
use std::path::Path;
use std::time::Duration;

use windows::core::{w, PCWSTR, PWSTR};
use windows::Win32::Foundation::{
    CloseHandle, GetLastError, LocalFree, SetLastError, ERROR_ALREADY_EXISTS,
    ERROR_SERVICE_DOES_NOT_EXIST, HANDLE, HLOCAL, WAIT_OBJECT_0, WIN32_ERROR,
};
use windows::Win32::Security::Authorization::{
    ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
};
use windows::Win32::Security::{
    DuplicateTokenEx, SecurityImpersonation, SetTokenInformation, TokenPrimary, TokenSessionId,
    PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES, TOKEN_ACCESS_MASK, TOKEN_ALL_ACCESS,
    TOKEN_DUPLICATE, TOKEN_QUERY,
};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};
use windows::Win32::System::RemoteDesktop::WTSGetActiveConsoleSessionId;
use windows::Win32::System::Threading::{
    CreateEventW, CreateProcessAsUserW, GetCurrentProcess, GetExitCodeProcess, OpenProcess,
    OpenProcessToken, ResetEvent, SetEvent, TerminateProcess, WaitForMultipleObjects,
    WaitForSingleObject, CREATE_NO_WINDOW, CREATE_UNICODE_ENVIRONMENT, INFINITE,
    PROCESS_INFORMATION, PROCESS_SYNCHRONIZE, STARTUPINFOW,
};
use windows_service::service::{
    ServiceAccess, ServiceControl, ServiceControlAccept, ServiceErrorControl, ServiceExitCode,
    ServiceInfo, ServiceStartType, ServiceState, ServiceStatus, ServiceType,
};
use windows_service::service_control_handler::{self, ServiceControlHandlerResult};
use windows_service::service_manager::{ServiceManager, ServiceManagerAccess};
use windows_service::{define_windows_service, service_dispatcher};

use crate::hosting::State;

pub const SERVICE_NAME: &str = "HelioHost";
const DISPLAY_NAME: &str = "Helio Host";
/// The event the service sets to ask the host to shut down cleanly
/// (restore the displays, tell the client) is named afresh by each run of
/// the service, this and 32 random hex digits, and handed to the host as
/// `host --stop-event NAME`. Not one fixed name: any user may create an
/// event in `Global\`, and one made first under a known name would be the
/// one the service and the host then share, its maker able to set it and
/// stop the host at will. A random name cannot be made in advance; the
/// event only SYSTEM may open, and a name that exists already is refused.
const STOP_EVENT_PREFIX: &str = "Global\\HelioHostStop-";
const STOP_EVENT_ARG: &str = "--stop-event";
/// Only SYSTEM (the service and the host it launches) may open it.
const STOP_EVENT_SDDL: PCWSTR = w!("D:P(A;;GA;;;SY)");

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

define_windows_service!(ffi_service_main, service_main);

/// Entry point when the SCM starts us (`helio-host service`).
pub fn run() -> Result<(), String> {
    service_dispatcher::start(SERVICE_NAME, ffi_service_main)
        .map_err(|e| format!("not started by the SCM: {e}"))
}

struct Events {
    stop: HANDLE,
    session: HANDLE,
}
unsafe impl Send for Events {}
unsafe impl Sync for Events {}

fn service_main(_args: Vec<OsString>) {
    if let Err(e) = service_body() {
        tracing::error!("service failed: {e}");
    }
}

fn service_body() -> Result<(), String> {
    let events = std::sync::Arc::new(unsafe {
        Events {
            stop: CreateEventW(None, true, false, None).map_err(|e| e.to_string())?,
            session: CreateEventW(None, false, false, None).map_err(|e| e.to_string())?,
        }
    });
    let ev = events.clone();
    let status = service_control_handler::register(SERVICE_NAME, move |control| match control {
        ServiceControl::Stop | ServiceControl::Preshutdown | ServiceControl::Shutdown => {
            unsafe { SetEvent(ev.stop).ok() };
            ServiceControlHandlerResult::NoError
        }
        ServiceControl::SessionChange(_) => {
            unsafe { SetEvent(ev.session).ok() };
            ServiceControlHandlerResult::NoError
        }
        ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
        _ => ServiceControlHandlerResult::NotImplemented,
    })
    .map_err(|e| e.to_string())?;

    let set = |state: ServiceState, accept: ServiceControlAccept| {
        let _ = status.set_service_status(ServiceStatus {
            service_type: ServiceType::OWN_PROCESS,
            current_state: state,
            controls_accepted: accept,
            exit_code: ServiceExitCode::Win32(0),
            checkpoint: 0,
            wait_hint: Duration::from_secs(20),
            process_id: None,
        });
    };
    set(
        ServiceState::Running,
        ServiceControlAccept::STOP
            | ServiceControlAccept::PRESHUTDOWN
            | ServiceControlAccept::SESSION_CHANGE,
    );
    tracing::info!("HelioHost running");

    let (host_stop, stop_name) = create_stop_event()?;
    let exe = crate::hosting::trusted_engine()?;
    let exe_w = wide(&exe.to_string_lossy());

    // Until stopped: make sure a host runs in the console session. At once
    // the first time, so that the host runs as soon as the service does
    // (`helio-host status` reports a service without its host as failed); then 3 s
    // after the host exits (at once when it exited to start again), or while
    // there is no console session.
    let mut wait = 0;
    while unsafe { WaitForSingleObject(events.stop, wait) } != WAIT_OBJECT_0 {
        wait = 3000;
        let session = unsafe { WTSGetActiveConsoleSessionId() };
        if session == 0xFFFF_FFFF {
            continue;
        }
        unsafe { ResetEvent(host_stop).ok() };
        let child = match launch(&exe_w, session, &stop_name) {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!(session, "could not launch the host: {e}");
                continue;
            }
        };
        tracing::info!(session, pid = child.dwProcessId, "host launched");
        loop {
            let handles = [events.stop, child.hProcess, events.session];
            let which = unsafe { WaitForMultipleObjects(&handles, false, INFINITE) };
            match which.0 - WAIT_OBJECT_0.0 {
                2 if unsafe { WTSGetActiveConsoleSessionId() } == session => continue,
                0 | 2 => {
                    if which == WAIT_OBJECT_0 {
                        // Stopping, said at once as SunshineService says it
                        // (`tools/sunshinesvc.cpp`): the host takes 4-5 s
                        // while it removes a virtual display a session left,
                        // and `Stop-Service` gives up on a service that is
                        // neither stopped nor stopping after 2 s.
                        set(ServiceState::StopPending, ServiceControlAccept::empty());
                    }
                    // Stopping, or the console moved to another session: ask
                    // the host to wind down, then insist.
                    unsafe { SetEvent(host_stop).ok() };
                    if unsafe { WaitForSingleObject(child.hProcess, 15_000) } != WAIT_OBJECT_0 {
                        tracing::warn!("host did not stop in time; terminating it");
                        unsafe { TerminateProcess(child.hProcess, 1).ok() };
                    }
                    break;
                }
                _ => {
                    let mut code = 0u32;
                    // SAFETY: the child's process handle, open until the
                    // CloseHandle below; `code` is a live local.
                    let _ = unsafe { GetExitCodeProcess(child.hProcess, &mut code) };
                    if code == u32::from(crate::restart::EXIT_CODE) {
                        tracing::info!("the host stopped to start again; relaunching");
                        wait = 0;
                    } else {
                        tracing::warn!(code, "host exited; relaunching");
                    }
                    break;
                }
            }
        }
        unsafe {
            let _ = CloseHandle(child.hThread);
            let _ = CloseHandle(child.hProcess);
        }
    }

    set(ServiceState::Stopped, ServiceControlAccept::empty());
    Ok(())
}

/// The host's stop event, made new: its handle and its name (see
/// [`STOP_EVENT_PREFIX`]).
fn create_stop_event() -> Result<(HANDLE, String), String> {
    let mut rng = rand_core::OsRng;
    let name = format!(
        "{STOP_EVENT_PREFIX}{:016x}{:016x}",
        rand_core::RngCore::next_u64(&mut rng),
        rand_core::RngCore::next_u64(&mut rng)
    );
    let name_w = wide(&name);
    let mut sd = PSECURITY_DESCRIPTOR::default();
    // SAFETY: `STOP_EVENT_SDDL` is a NUL-terminated constant; `sd` is the
    // descriptor the system allocates for it, read by CreateEventW through
    // `attributes` and freed once after it; `name_w` is NUL-terminated and
    // outlives the call. The last error is cleared before CreateEventW and
    // read straight after it, which is how it tells an event that existed.
    unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            STOP_EVENT_SDDL,
            SDDL_REVISION_1,
            &mut sd,
            None,
        )
        .map_err(|e| format!("the stop event's permissions: {}", e.message()))?;
        let attributes = SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: sd.0,
            bInheritHandle: false.into(),
        };
        SetLastError(WIN32_ERROR(0));
        let event = CreateEventW(Some(&attributes), true, false, PCWSTR(name_w.as_ptr()));
        let existed = GetLastError() == ERROR_ALREADY_EXISTS;
        let _ = LocalFree(Some(HLOCAL(sd.0)));
        let event = event.map_err(|e| format!("the stop event: {}", e.message()))?;
        if existed {
            let _ = CloseHandle(event);
            return Err(format!(
                "the stop event {name} exists already, made by another"
            ));
        }
        Ok((event, name))
    }
}

/// In the host: the stop event's name the service gave it, if it did.
fn stop_event_name(args: &[String]) -> Option<&str> {
    args.windows(2)
        .find(|pair| pair[0] == STOP_EVENT_ARG)
        .map(|pair| pair[1].as_str())
}

/// Start `helio-host host` in `session` with a copy of this process's SYSTEM
/// token, told the name of its stop event.
fn launch(exe: &[u16], session: u32, stop_event: &str) -> Result<PROCESS_INFORMATION, String> {
    unsafe {
        let mut own = HANDLE::default();
        OpenProcessToken(
            GetCurrentProcess(),
            TOKEN_ACCESS_MASK(TOKEN_DUPLICATE.0 | TOKEN_QUERY.0),
            &mut own,
        )
        .map_err(|e| format!("OpenProcessToken: {e}"))?;
        let mut token = HANDLE::default();
        let dup = DuplicateTokenEx(
            own,
            TOKEN_ALL_ACCESS,
            None,
            SecurityImpersonation,
            TokenPrimary,
            &mut token,
        );
        let _ = CloseHandle(own);
        dup.map_err(|e| format!("DuplicateTokenEx: {e}"))?;
        SetTokenInformation(token, TokenSessionId, &session as *const u32 as *const _, 4)
            .map_err(|e| format!("SetTokenInformation(TokenSessionId): {e}"))?;

        let mut desktop = wide("winsta0\\default");
        let si = STARTUPINFOW {
            cb: std::mem::size_of::<STARTUPINFOW>() as u32,
            lpDesktop: PWSTR(desktop.as_mut_ptr()),
            ..Default::default()
        };
        let exe_str = String::from_utf16_lossy(&exe[..exe.len() - 1]);
        let mut cmd = wide(&format!("\"{exe_str}\" host {STOP_EVENT_ARG} {stop_event}"));
        let mut pi = PROCESS_INFORMATION::default();
        let result = CreateProcessAsUserW(
            Some(token),
            PCWSTR(exe.as_ptr()),
            Some(PWSTR(cmd.as_mut_ptr())),
            None,
            None,
            false,
            CREATE_UNICODE_ENVIRONMENT | CREATE_NO_WINDOW,
            None,
            None,
            &si,
            &mut pi,
        );
        let _ = CloseHandle(token);
        result.map_err(|e| format!("CreateProcessAsUserW: {e}"))?;
        Ok(pi)
    }
}

/// How long the host has to stop with the service: it is given 15 s to
/// end its session and put the displays back (`service_body`), then ended.
const STOP_WAIT_MS: u32 = 30_000;

fn manager(access: ServiceManagerAccess) -> Result<ServiceManager, String> {
    ServiceManager::local_computer(None::<&str>, access)
        .map_err(|e| format!("cannot open the service manager: {e}"))
}

fn not_installed(e: &windows_service::Error) -> bool {
    matches!(e, windows_service::Error::Winapi(io)
        if io.raw_os_error() == Some(ERROR_SERVICE_DOES_NOT_EXIST.0 as i32))
}

/// Register the service to run `engine` (auto-start, LocalSystem, restart
/// on failure), open the firewall to it, and start it.
pub fn register(engine: &Path) -> Result<(), String> {
    let manager = manager(ServiceManagerAccess::CONNECT | ServiceManagerAccess::CREATE_SERVICE)?;
    let info = ServiceInfo {
        name: OsString::from(SERVICE_NAME),
        display_name: OsString::from(DISPLAY_NAME),
        service_type: ServiceType::OWN_PROCESS,
        start_type: ServiceStartType::AutoStart,
        error_control: ServiceErrorControl::Normal,
        // Quoted by windows-service when it holds a space ("C:\Program
        // Files\..."), as an unquoted path with spaces is a known way in.
        executable_path: engine.to_path_buf(),
        launch_arguments: vec![OsString::from("service")],
        dependencies: vec![],
        account_name: None,
        account_password: None,
    };
    let access = ServiceAccess::CHANGE_CONFIG | ServiceAccess::START | ServiceAccess::QUERY_STATUS;
    let service = match manager.create_service(&info, access) {
        Ok(s) => s,
        Err(_) => {
            let s = manager
                .open_service(SERVICE_NAME, access)
                .map_err(|e| format!("open existing service: {e}"))?;
            s.change_config(&info)
                .map_err(|e| format!("update service: {e}"))?;
            s
        }
    };
    let _ =
        service.set_description("Streams this PC to Helio clients over post-quantum WireGuard.");
    let _ = service.update_failure_actions(windows_service::service::ServiceFailureActions {
        reset_period: windows_service::service::ServiceFailureResetPeriod::After(
            Duration::from_secs(86400),
        ),
        reboot_msg: None,
        command: None,
        actions: Some(vec![
            windows_service::service::ServiceAction {
                action_type: windows_service::service::ServiceActionType::Restart,
                delay: Duration::from_secs(3),
            };
            3
        ]),
    });
    firewall(Some(engine));
    let _ = service.start::<&str>(&[]);
    Ok(())
}

/// Stop the service, delete it, and remove its firewall rules. Nothing to
/// do for a service that is not there.
pub fn unregister() -> Result<(), String> {
    let manager = manager(ServiceManagerAccess::CONNECT)?;
    match manager.open_service(
        SERVICE_NAME,
        ServiceAccess::STOP | ServiceAccess::DELETE | ServiceAccess::QUERY_STATUS,
    ) {
        Ok(service) => {
            stop_and_wait(&service);
            service
                .delete()
                .map_err(|e| format!("delete service: {e}"))?;
        }
        Err(e) if not_installed(&e) => {}
        Err(e) => return Err(format!("open service: {e}")),
    }
    firewall(None);
    Ok(())
}

/// Stop the service if it runs, and wait for it to have stopped; whether
/// it ran.
pub fn stop() -> bool {
    let Ok(manager) = manager(ServiceManagerAccess::CONNECT) else {
        return false;
    };
    manager
        .open_service(
            SERVICE_NAME,
            ServiceAccess::STOP | ServiceAccess::QUERY_STATUS,
        )
        .is_ok_and(|service| stop_and_wait(&service))
}

/// Start the service (after [`stop`]), without waiting for it.
pub fn start() {
    if let Ok(manager) = manager(ServiceManagerAccess::CONNECT) {
        if let Ok(service) = manager.open_service(SERVICE_NAME, ServiceAccess::START) {
            let _ = service.start::<&str>(&[]);
        }
    }
}

/// Stop `service` and wait for its process to end (the host it launched
/// ends first); whether it ran.
fn stop_and_wait(service: &windows_service::service::Service) -> bool {
    let Ok(status) = service.query_status() else {
        return false;
    };
    if status.current_state == ServiceState::Stopped {
        return false;
    }
    // SAFETY: a handle to wait on, closed once below.
    let process = status
        .process_id
        .and_then(|pid| unsafe { OpenProcess(PROCESS_SYNCHRONIZE, false, pid) }.ok());
    let _ = service.stop();
    if let Some(process) = process {
        // SAFETY: the handle is the one just opened.
        unsafe {
            WaitForSingleObject(process, STOP_WAIT_MS);
            let _ = CloseHandle(process);
        }
    }
    true
}

/// What the service manager says of the service, as hosting states go.
pub fn state() -> State {
    let manager = match manager(ServiceManagerAccess::CONNECT) {
        Ok(m) => m,
        Err(e) => return State::Failed(e),
    };
    let service = match manager.open_service(SERVICE_NAME, ServiceAccess::QUERY_STATUS) {
        Ok(s) => s,
        Err(e) if not_installed(&e) => return State::Off,
        Err(e) => return State::Failed(format!("cannot read HelioHost: {e}")),
    };
    let status = match service.query_status() {
        Ok(s) => s,
        Err(e) => return State::Failed(format!("cannot read HelioHost: {e}")),
    };
    match status.current_state {
        ServiceState::Running if status.process_id.is_none_or(host_runs) => State::On,
        ServiceState::Running => State::Failed(
            "the host is not running; HelioHost starts it again in 3 s \
                (its log: C:\\ProgramData\\Helio Host\\logs)"
                .into(),
        ),
        ServiceState::StartPending | ServiceState::ContinuePending => State::Starting,
        ServiceState::StopPending => State::Failed("HelioHost is stopping".into()),
        ServiceState::PausePending | ServiceState::Paused => {
            State::Failed("HelioHost is paused".into())
        }
        ServiceState::Stopped => State::Failed(match status.exit_code {
            ServiceExitCode::Win32(0) => "HelioHost is stopped".into(),
            ServiceExitCode::Win32(code) => format!("HelioHost stopped with error {code}"),
            ServiceExitCode::ServiceSpecific(code) => {
                format!("HelioHost stopped with error {code} of its own")
            }
        }),
    }
}

/// Whether the service (process `service`) has its host running: a
/// process it started. The service itself runs whatever becomes of the
/// host, so its own state cannot say. When it cannot be told, yes.
fn host_runs(service: u32) -> bool {
    // SAFETY: the snapshot is this function's own, closed once; `entry`
    // carries its size, as Process32FirstW and Process32NextW require.
    unsafe {
        let Ok(snapshot) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else {
            return true;
        };
        let mut entry = PROCESSENTRY32W {
            dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };
        let mut found = false;
        let mut more = Process32FirstW(snapshot, &mut entry).is_ok();
        while more && !found {
            found = entry.th32ParentProcessID == service;
            more = Process32NextW(snapshot, &mut entry).is_ok();
        }
        let _ = CloseHandle(snapshot);
        found
    }
}

/// Firewall rules "Helio Host" allowing inbound traffic to `program`, or none.
///
/// `netsh` by its full path, never its bare name: this runs elevated from
/// `helio-host enable` and `disable` in the per-user install folder, which
/// the user (and whatever runs as them) can write, and std's `Command`
/// looks for a bare name beside the running program before System32.
fn firewall(program: Option<&Path>) {
    let netsh = match crate::hosting::system_dir() {
        Ok(dir) => dir.join("netsh.exe"),
        Err(e) => {
            tracing::warn!(error = e, "the firewall rules are left as they are");
            return;
        }
    };
    let _ = std::process::Command::new(&netsh)
        .args([
            "advfirewall",
            "firewall",
            "delete",
            "rule",
            "name=Helio Host",
        ])
        .output();
    let Some(program) = program else {
        return;
    };
    for proto in ["UDP", "TCP"] {
        let _ = std::process::Command::new(&netsh)
            .args([
                "advfirewall",
                "firewall",
                "add",
                "rule",
                "name=Helio Host",
                "dir=in",
                "action=allow",
                &format!("protocol={proto}"),
                &format!("program={}", program.display()),
                "enable=yes",
                "profile=any",
            ])
            .output();
    }
}

/// In the host: return when the service asks us to stop. `args`: the
/// host's command line, which names the stop event when the service
/// launched it.
pub fn wait_for_stop_request(args: &[String]) {
    use windows::Win32::System::Threading::{OpenEventW, SYNCHRONIZATION_SYNCHRONIZE};
    let event = stop_event_name(args).and_then(|name| {
        let wide_name = wide(name);
        // SAFETY: `wide_name` is NUL-terminated and outlives the call.
        let opened = unsafe {
            OpenEventW(
                SYNCHRONIZATION_SYNCHRONIZE,
                false,
                PCWSTR(wide_name.as_ptr()),
            )
        };
        opened
            .inspect_err(|e| {
                // Then the service's stop ends this process the hard way,
                // after its wait, with the displays not put back.
                tracing::warn!(name, error = %e, "the service's stop event cannot be opened");
            })
            .ok()
    });
    let Some(event) = event else {
        // Not launched by the service: nothing to wait for.
        loop {
            std::thread::park();
        }
    };
    // SAFETY: the handle just opened, held for the rest of the process.
    unsafe { WaitForSingleObject(event, INFINITE) };
}
