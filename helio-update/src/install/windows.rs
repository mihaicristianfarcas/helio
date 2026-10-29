//! Installing an update on Windows: the release's installer
//! (`Helio-X.Y.Z-windows-x86_64-setup.exe`, `tools/windows/helio.iss`),
//! the one program that installs Helio, the host's engine with it. An
//! update is that installer run again over Helio, as a person would run a
//! newer one, silently.
//!
//! 1. The installer is downloaded and checked: GitHub's SHA-256, then its
//!    Authenticode signature, by Helio's publisher (`authenticode`).
//! 2. A waiter is started, Windows PowerShell with no window ([`waiter`]):
//!    once the app has quit, it runs the installer for this user, with no
//!    administrator (`/VERYSILENT /SUPPRESSMSGBOXES /NORESTART`), and opens
//!    the app again, the new one or, if the installer failed (it then puts
//!    everything back), the one there was. What went wrong goes in the
//!    update log, which the next start reads.
//! 3. While hosting is on, the service runs the engine's copy in Program
//!    Files, which only `helio-host enable`, as an administrator, replaces
//!    (`helio-host/src/hosting/windows.rs`). So Windows is asked now, while
//!    the app's window is in front ([`update_host`]): asked later, by the
//!    waiter or the installer, which have no window, it would only flash in
//!    the taskbar. Allowed, the engine there waits for the waiter and then
//!    runs the new engine's `enable` (`helio-host update-engine`), which
//!    checks it, puts it in place and restarts the service on it. Declined,
//!    the update stops there: the waiter is ended and nothing has changed.
//!    The installer is told to leave the host to Helio (`/HELIOUPDATE`).
//!    Without hosting on, nothing asks for an administrator.
//!
//! Only a copy the installer put in place updates itself: one unpacked
//! elsewhere would be installed beside, not replaced.

use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use super::{authenticode, ps_quote};

/// How this copy is updated: the installer's, in `dir`.
pub(crate) struct Method {
    dir: PathBuf,
}

/// No console window for a script or a tool.
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Helio's entry in Settings > Apps, under HKEY_CURRENT_USER, as the
/// installer names it: its `AppId`, to which Inno Setup adds `_is1`.
const HELIO_ENTRY: &str =
    r"Software\Microsoft\Windows\CurrentVersion\Uninstall\dev.mihaicristian.helio_is1";

/// HelioHost, the service hosting runs as (`helio-host/src/service.rs`):
/// registered while hosting is on, and only then (`helio-host enable`,
/// `disable`). The installer looks at the same key.
const HOST_SERVICE: &str = r"SYSTEM\CurrentControlSet\Services\HelioHost";

/// What tells the installer that Helio updates the host itself
/// (`tools/windows/helio.iss`).
const HELIO_UPDATE: &str = "/HELIOUPDATE";

/// The file the app writes once it has handed the update over: the waiter
/// runs the installer only when it is there. An app that gave the update
/// up (Windows was not allowed to update the host), or died before handing
/// over, leaves none.
const HANDED_OVER: &str = "handed-over";

/// How long the waiter waits for the app to quit before it runs the
/// installer all the same, which then closes what still runs from Helio's
/// folder (Restart Manager). Quitting takes the app well under a second;
/// a stream ending takes a moment more.
const APP_QUIT_WAIT: Duration = Duration::from_secs(60);

/// Windows' own programs, by full path: never one that happens to be on
/// PATH or in the current folder.
fn system32(program: &str) -> PathBuf {
    let root = std::env::var_os("SystemRoot")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Windows"));
    root.join("System32").join(program)
}

fn powershell() -> PathBuf {
    system32(r"WindowsPowerShell\v1.0\powershell.exe")
}

/// A string value of a registry key, if both are there.
fn registry_string(
    root: windows::Win32::System::Registry::HKEY,
    key: &str,
    value: &str,
) -> Option<String> {
    use windows::core::HSTRING;
    use windows::Win32::System::Registry::{RegGetValueW, RRF_RT_REG_SZ};
    let mut text = [0u16; 2048];
    let mut bytes = std::mem::size_of_val(&text) as u32;
    // SAFETY: `text` is writable for the `bytes` said; RegGetValueW writes at
    // most that, ends a REG_SZ with a NUL, and says how many bytes it wrote.
    let status = unsafe {
        RegGetValueW(
            root,
            &HSTRING::from(key),
            &HSTRING::from(value),
            RRF_RT_REG_SZ,
            None,
            Some(text.as_mut_ptr().cast()),
            Some(&mut bytes),
        )
    };
    if !status.is_ok() {
        return None;
    }
    let len = (bytes as usize / 2).saturating_sub(1).min(text.len());
    Some(String::from_utf16_lossy(&text[..len]))
}

/// Whether two paths name the same folder, as Windows compares them:
/// without regard to case or a trailing separator.
fn same_folder(a: &Path, b: &Path) -> bool {
    let norm = |p: &Path| p.to_string_lossy().trim_end_matches('\\').to_lowercase();
    norm(a) == norm(b)
}

pub(crate) fn method() -> Result<Method, String> {
    use windows::Win32::System::Registry::HKEY_CURRENT_USER;
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let dir = exe
        .parent()
        .ok_or("Helio's folder is unknown")?
        .to_path_buf();
    // Where the installer put Helio, as its entry in Settings > Apps says
    // (per user, as it installs).
    let installed = registry_string(HKEY_CURRENT_USER, HELIO_ENTRY, "InstallLocation");
    if !installed.is_some_and(|f| same_folder(&dir, Path::new(&f))) {
        return Err(format!(
            "This copy of Helio was not installed with its installer (it runs from {}): \
             download the update and install it.",
            dir.display()
        ));
    }
    Ok(Method { dir })
}

pub(crate) fn needs_package(_method: &Method) -> bool {
    true
}

/// The installer, once its signature is known to be Helio's publisher's.
pub(crate) fn open(setup: &Path, _stage: &Path) -> Result<PathBuf, String> {
    authenticode::check(setup, authenticode::PUBLISHER)?;
    Ok(setup.to_path_buf())
}

/// Hand the update over: the waiter started, Windows asked to update the
/// host while hosting is on, and, once all that is settled, the word the
/// waiter waits for. The app then quits.
pub(crate) fn place(method: &Method, setup: &Path, stage: &Path, dir: &Path) -> Result<(), String> {
    let handed_over = stage.join(HANDED_OVER);
    let _ = std::fs::remove_file(&handed_over);
    let log = super::log_path(dir);
    let mut waiter = start_waiter(&Waiter {
        app_pid: std::process::id(),
        handed_over: &handed_over,
        setup,
        setup_log: &setup_log_path(dir),
        app: &method.dir.join("Helio.exe"),
        log: &log,
    })?;
    let handed = (|| {
        if hosting_on() {
            update_host(waiter.id(), &method.dir)?;
        }
        std::fs::write(&handed_over, b"").map_err(|e| format!("{}: {e}", handed_over.display()))
    })();
    if handed.is_err() {
        let _ = waiter.kill();
    }
    handed
}

/// Inno Setup's own log of the update's run, beside the update log.
fn setup_log_path(dir: &Path) -> PathBuf {
    dir.join("update-setup.log")
}

/// Whether hosting is on: HelioHost is registered.
fn hosting_on() -> bool {
    use windows::core::HSTRING;
    use windows::Win32::System::Registry::{RegGetValueW, HKEY_LOCAL_MACHINE, RRF_RT_ANY};
    // SAFETY: asks only whether the value exists: no buffer is passed.
    let status = unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            &HSTRING::from(HOST_SERVICE),
            &HSTRING::from("ImagePath"),
            RRF_RT_ANY,
            None,
            None,
            None,
        )
    };
    status.is_ok()
}

/// What the waiter is told.
struct Waiter<'a> {
    /// The app, which quits first.
    app_pid: u32,
    /// [`HANDED_OVER`].
    handed_over: &'a Path,
    setup: &'a Path,
    /// [`setup_log_path`].
    setup_log: &'a Path,
    /// Helio.exe, opened again after.
    app: &'a Path,
    /// The update log.
    log: &'a Path,
}

/// The waiter's script, for Windows PowerShell 5.1 (every Windows 10 and 11
/// has it). It exits with the installer's exit code (0: installed), which
/// the host's update waits for; 2 when the update was given up. Inno
/// Setup's own codes: 1 it did not start, 2 or 5 cancelled, 3 or 4 a fatal
/// error, 8 it asks for a restart first.
fn waiter(w: &Waiter) -> String {
    // One string, as the installer reads its command line: the log's path
    // may have spaces, and Start-Process passes the string as it is.
    let arguments = format!(
        "/VERYSILENT /SUPPRESSMSGBOXES /NORESTART /NOCANCEL {HELIO_UPDATE} /LOG=\"{}\"",
        w.setup_log.display()
    );
    let folder = w.app.parent().unwrap_or(w.app);
    format!(
        "# Helio's update (helio-update), once Helio has quit: its installer, silently, \
         then Helio again.\n\
         $ErrorActionPreference = 'Stop'\n\
         $code = 1\n\
         try {{\n\
         \x20   Wait-Process -Id {pid} -Timeout {wait} -ErrorAction SilentlyContinue\n\
         \x20   if (-not (Test-Path -LiteralPath {handed_over})) {{ exit 2 }}\n\
         \x20   $setup = Start-Process -FilePath {setup} -ArgumentList {arguments} -PassThru\n\
         \x20   # Held from the start: a process that has exited has no exit code to give\n\
         \x20   # without it.\n\
         \x20   $null = $setup.Handle\n\
         \x20   $setup.WaitForExit()\n\
         \x20   $code = $setup.ExitCode\n\
         \x20   if ($code -ne 0) {{\n\
         \x20       throw ('the installer stopped with exit code ' + $code + ' (its log: ' + {setup_log} + ')')\n\
         \x20   }}\n\
         }} catch {{\n\
         \x20   Add-Content -LiteralPath {log} -Encoding UTF8 -Value $_.Exception.Message\n\
         }}\n\
         Start-Process -FilePath {app} -WorkingDirectory {folder}\n\
         exit $code\n",
        pid = w.app_pid,
        wait = APP_QUIT_WAIT.as_secs(),
        handed_over = ps_quote(&w.handed_over.to_string_lossy()),
        setup = ps_quote(&w.setup.to_string_lossy()),
        arguments = ps_quote(&arguments),
        setup_log = ps_quote(&w.setup_log.to_string_lossy()),
        log = ps_quote(&w.log.to_string_lossy()),
        app = ps_quote(&w.app.to_string_lossy()),
        folder = ps_quote(&folder.to_string_lossy()),
    )
}

/// Started with this app's environment, which the new app is then opened
/// with (a data folder it was pointed at, say), but for a UI demo's steps:
/// those were for this copy.
fn start_waiter(w: &Waiter) -> Result<Child, String> {
    Command::new(powershell())
        .args(["-NoProfile", "-NonInteractive", "-Command", &waiter(w)])
        .env_remove("HELIO_UI_DEMO")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map_err(|e| format!("the update's installer could not be started: {e}"))
}

/// `C:\Program Files\Helio Host\helio-host.exe`, the engine the service
/// runs, found as helio-host finds it: by its known folder, not
/// `%ProgramFiles%`, which whoever runs this could point anywhere.
fn service_engine() -> Result<PathBuf, String> {
    use windows::Win32::System::Com::CoTaskMemFree;
    use windows::Win32::UI::Shell::{FOLDERID_ProgramFiles, SHGetKnownFolderPath, KF_FLAG_DEFAULT};
    // SAFETY: the shell allocates the path; it is copied, then freed once.
    let folder = unsafe {
        let path = SHGetKnownFolderPath(&FOLDERID_ProgramFiles, KF_FLAG_DEFAULT, None)
            .map_err(|e| format!("cannot find Program Files: {}", e.message()))?;
        let text = path.to_string();
        CoTaskMemFree(Some(path.0 as *const _));
        text.map_err(|e| e.to_string())?
    };
    Ok(PathBuf::from(folder).join(r"Helio Host\helio-host.exe"))
}

/// This app's window, if it is the one in front: the window Windows' UAC
/// prompt is asked for, so that it opens at once rather than as a button
/// that flashes in the taskbar (Windows does that for a request from a
/// window, or a program without one, that is not in front).
fn own_window_in_front() -> windows::Win32::Foundation::HWND {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::System::Threading::GetCurrentProcessId;
    use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};
    // SAFETY: both only read which window is in front and whose it is.
    unsafe {
        let window = GetForegroundWindow();
        let mut pid = 0u32;
        GetWindowThreadProcessId(window, Some(&mut pid));
        if pid == GetCurrentProcessId() {
            window
        } else {
            HWND::default()
        }
    }
}

/// Have Windows ask for an administrator now, and, allowed, start the
/// host's update: the service's engine runs `helio-host update-engine`,
/// which waits for process `waiter` and then runs the new engine's
/// `enable` from `app_dir` (its log is the host's: an administrator
/// writes it). Declined: nothing has started, and the update stops.
fn update_host(waiter: u32, app_dir: &Path) -> Result<(), String> {
    use windows::core::{HSTRING, PCWSTR};
    use windows::Win32::Foundation::{CloseHandle, ERROR_CANCELLED};
    use windows::Win32::UI::Shell::{
        ShellExecuteExW, SEE_MASK_FLAG_NO_UI, SEE_MASK_NOASYNC, SEE_MASK_NOCLOSEPROCESS,
        SHELLEXECUTEINFOW,
    };
    use windows::Win32::UI::WindowsAndMessaging::SW_HIDE;

    let engine = service_engine()?;
    if !engine.is_file() {
        return Err(format!(
            "hosting is on, but its engine is not in {}: turn hosting off and on in Helio, then \
             update",
            engine.parent().unwrap_or(&engine).display()
        ));
    }
    tracing::info!("hosting is on: Windows is asked to let the host's engine be updated");
    let (verb, file) = (HSTRING::from("runas"), HSTRING::from(engine.as_os_str()));
    let args = HSTRING::from(update_engine_args(waiter, app_dir));
    let mut info = SHELLEXECUTEINFOW {
        cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOCLOSEPROCESS | SEE_MASK_NOASYNC | SEE_MASK_FLAG_NO_UI,
        hwnd: own_window_in_front(),
        lpVerb: PCWSTR(verb.as_ptr()),
        lpFile: PCWSTR(file.as_ptr()),
        lpParameters: PCWSTR(args.as_ptr()),
        nShow: SW_HIDE.0,
        ..Default::default()
    };
    // SAFETY: `info` is filled as ShellExecuteExW documents, its strings live
    // until it returns; the process handle it gives is closed below.
    unsafe {
        if let Err(e) = ShellExecuteExW(&mut info) {
            return Err(if e.code() == ERROR_CANCELLED.to_hresult() {
                "Windows was not allowed to update the host, so nothing changed: try again, \
                 and choose Yes when Windows asks"
                    .into()
            } else {
                format!("Windows could not start the host's update: {}", e.message())
            });
        }
        if !info.hProcess.is_invalid() {
            let _ = CloseHandle(info.hProcess);
        }
    }
    Ok(())
}

/// `update-engine PID ENGINE`, quoted as Windows splits a command line (a
/// path has no `"` in it, and this one ends in a file's name, not a `\`).
fn update_engine_args(waiter: u32, app_dir: &Path) -> String {
    format!(
        "update-engine {waiter} \"{}\"",
        app_dir.join("helio-host.exe").display()
    )
}

/// `install::rehearse`: the checked installer copied into `folder`, not
/// run. Run, it would install over the Helio that is installed.
pub(crate) fn rehearse(setup: &Path, folder: &Path) -> Result<(), String> {
    let name = setup.file_name().ok_or("the installer has no name")?;
    std::fs::copy(setup, folder.join(name))
        .map(drop)
        .map_err(|e| format!("{}: {e}", folder.display()))
}

/// Wait until process `pid` has exited, or `timeout`.
pub(crate) fn wait_for_exit(pid: u32, timeout: Duration) {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{
        OpenProcess, WaitForSingleObject, PROCESS_SYNCHRONIZE,
    };
    // SAFETY: the handle is the process's, opened just for waiting, and
    // closed after.
    unsafe {
        let Ok(process) = OpenProcess(PROCESS_SYNCHRONIZE, false, pid) else {
            // Gone already (or never ours to see).
            return;
        };
        WaitForSingleObject(process, timeout.as_millis().min(u32::MAX as u128) as u32);
        let _ = CloseHandle(process);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(dir: &Path) -> String {
        waiter(&Waiter {
            app_pid: 4242,
            handed_over: &dir.join(r"helio-update\Helio-1.0.1\handed-over"),
            setup: &dir.join(r"helio-update\Helio-1.0.1\Helio-1.0.1-windows-x86_64-setup.exe"),
            setup_log: &dir.join(r"Roaming\Helio\update-setup.log"),
            app: Path::new(r"C:\Users\o'brien\AppData\Local\Programs\Helio\Helio.exe"),
            log: &dir.join(r"Roaming\Helio\update.log"),
        })
    }

    #[test]
    fn the_waiter_runs_the_installer_silently_for_helio_then_opens_helio() {
        let script = sample(Path::new(r"C:\Users\o'brien\AppData"));
        assert!(script.contains("Wait-Process -Id 4242 -Timeout 60"));
        assert!(script.contains(
            r#"-ArgumentList '/VERYSILENT /SUPPRESSMSGBOXES /NORESTART /NOCANCEL /HELIOUPDATE /LOG="C:\Users\o''brien\AppData\Roaming\Helio\update-setup.log"'"#
        ));
        // Nothing runs unless the app handed the update over.
        let handed = script.find("handed-over").unwrap();
        assert!(handed < script.find("Start-Process -FilePath").unwrap());
        assert!(script.ends_with(
            "Start-Process -FilePath 'C:\\Users\\o''brien\\AppData\\Local\\Programs\\Helio\\Helio.exe' \
             -WorkingDirectory 'C:\\Users\\o''brien\\AppData\\Local\\Programs\\Helio'\nexit $code\n"
        ));
    }

    #[test]
    fn the_waiters_script_is_powershell_that_parses() {
        let dir = tempfile::tempdir().unwrap();
        let script = sample(dir.path());
        let file = dir.path().join("waiter.ps1");
        std::fs::write(&file, &script).unwrap();
        // The script as Windows PowerShell's parser reads it, run nowhere.
        let check = format!(
            "$e = $null; [void][System.Management.Automation.Language.Parser]::ParseFile({}, \
             [ref]$null, [ref]$e); if ($e.Count) {{ $e | ForEach-Object {{ $_.Message }}; exit 1 }}",
            ps_quote(&file.to_string_lossy())
        );
        let out = Command::new(powershell())
            .args(["-NoProfile", "-NonInteractive", "-Command", &check])
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}\n{script}",
            String::from_utf8_lossy(&out.stdout)
        );
    }

    #[test]
    fn the_hosts_update_names_the_waiter_and_the_new_engine_whole() {
        // As helio-host's update-engine reads them: PID ENGINE, no more.
        assert_eq!(
            update_engine_args(
                77,
                Path::new(r"C:\Users\Jo Doe\AppData\Local\Programs\Helio"),
            ),
            r#"update-engine 77 "C:\Users\Jo Doe\AppData\Local\Programs\Helio\helio-host.exe""#
        );
    }

    #[test]
    fn a_folder_is_the_same_whatever_its_case_or_a_trailing_separator() {
        let installed = Path::new(r"C:\Users\o'brien\AppData\Local\Programs\Helio");
        assert!(same_folder(
            Path::new(r"c:\users\O'BRIEN\appdata\local\programs\helio\"),
            installed
        ));
        assert!(!same_folder(
            Path::new(r"C:\Users\o'brien\Downloads\Helio-0.9.5-windows-x86_64"),
            installed
        ));
    }
}
