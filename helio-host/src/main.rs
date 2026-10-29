//! Helio Host's program: what the system and Helio run. People do not type
//! it: `helio host` (helio-cli) is the command for this computer as a host,
//! and runs `enable`, `disable` and `status` here, as the app's hosting
//! switch does.
//!
//!   helio-host [host]                run the host in this session (what
//!                                    hosting starts: the LaunchAgent, the
//!                                    systemd unit, the service)
//!   helio-host enable | disable      hosting on (the host runs at every start,
//!                                    from now) or off (see `hosting`)
//!   helio-host status [--json]       off, starting, on or failed; for on,
//!                                    whether the web API answers
//!
//! On Windows, also:
//!
//!   helio-host service               the service itself (started by Windows)
//!   helio-host clipboard-agent       clipboard sharing as the signed-in user
//!                                    (started by the host for a session)
//!
//! On Windows, DLLs come only from System32 and from beside `helio-host.exe`
//! (`restrict_dll_search`).

#[cfg(windows)]
mod apps;
#[cfg(windows)]
mod audio;
#[cfg(any(target_os = "macos", target_os = "linux"))]
#[path = "unix/audio.rs"]
mod audio;
mod bitrate;
#[cfg(windows)]
mod clipagent;
mod config;
#[cfg(windows)]
mod endsession;
#[cfg(windows)]
mod gamepad;
mod host;
mod hosting;
#[cfg(target_os = "macos")]
#[path = "mac/icon.rs"]
mod icon;
mod negotiate;
mod netif;
#[cfg(windows)]
mod nvprefs;
mod pairing;
#[cfg(target_os = "macos")]
#[path = "mac/permissions.rs"]
mod permissions;
mod pipeline;
#[cfg(windows)]
mod platform;
#[cfg(target_os = "macos")]
#[path = "mac/platform.rs"]
mod platform;
#[cfg(target_os = "linux")]
#[path = "linux/platform.rs"]
mod platform;
#[cfg(target_os = "linux")]
#[path = "linux/portal.rs"]
mod portal;
#[cfg(target_os = "macos")]
#[path = "mac/power.rs"]
mod power;
#[cfg(target_os = "linux")]
#[path = "linux/power.rs"]
mod power;
mod presence;
mod priority;
mod sender;
#[cfg(windows)]
mod service;
mod session;
#[cfg(target_os = "macos")]
#[path = "mac/sound.rs"]
mod sound;
#[cfg(windows)]
mod tuning;
#[cfg(windows)]
mod video;
#[cfg(any(target_os = "macos", target_os = "linux"))]
#[path = "unix/video.rs"]
mod video;
mod web;

use std::process::ExitCode;

// The host's data, shared with `helio host` (helio-host-data).
use helio_host_data::{clients, private};

fn init_logging(dir: &std::path::Path) -> tracing_appender::non_blocking::WorkerGuard {
    init_logging_named(dir, "helio-host.log")
}

fn init_logging_named(
    dir: &std::path::Path,
    name: &str,
) -> tracing_appender::non_blocking::WorkerGuard {
    use tracing_subscriber::prelude::*;
    let logs = dir.join("logs");
    let _ = std::fs::create_dir_all(&logs);
    let file = tracing_appender::rolling::daily(&logs, name);
    let (file, guard) = tracing_appender::non_blocking(file);
    let filter = || {
        tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
            "info,mainline=error,zbus=error,helio-host=info,helio_transport=info,helio_display=info,helio_input=info,\
                helio_capture=info,helio_encode=info"
                .into()
        })
    };
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(std::io::stderr)
                .with_filter(filter()),
        )
        .with(
            tracing_subscriber::fmt::layer()
                .with_ansi(false)
                .with_writer(file)
                .with_filter(filter()),
        )
        .init();
    guard
}

/// DLLs from System32 and from beside `helio-host.exe` only, never from PATH or
/// the current folder: the host runs as SYSTEM, and a DLL planted in a
/// folder on PATH would run as SYSTEM with it. Beside `helio-host.exe` is
/// `C:\Program Files\Helio Host` for the service and the host (`hosting`), which
/// only administrators can change. What the host loads by name, NVENC
/// (`nvEncodeAPI64.dll`) and NVAPI, the driver puts in System32.
///
/// From the first line of `main`. The DLLs `helio-host.exe` imports are loaded
/// before it: from System32 only, in an MSVC build (`build.rs`).
#[cfg(windows)]
fn restrict_dll_search() {
    use windows::Win32::System::LibraryLoader::{
        SetDefaultDllDirectories, LOAD_LIBRARY_SEARCH_APPLICATION_DIR, LOAD_LIBRARY_SEARCH_SYSTEM32,
    };
    // SAFETY: changes where later loads look, and nothing else; no other
    // thread runs yet.
    unsafe {
        SetDefaultDllDirectories(LOAD_LIBRARY_SEARCH_SYSTEM32 | LOAD_LIBRARY_SEARCH_APPLICATION_DIR)
    }
    .expect("SetDefaultDllDirectories exists on every Windows helio-host runs on (8 and later)");
}

fn main() -> ExitCode {
    #[cfg(windows)]
    restrict_dll_search();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let dir = config::data_dir();
    // The host's helper app opened by hand (a Mac): the host, with its icon.
    #[cfg(target_os = "macos")]
    let opened_by_hand = icon::opened_by_hand(&args);
    #[cfg(not(target_os = "macos"))]
    let opened_by_hand = false;
    let command = match args.first() {
        _ if opened_by_hand => "host",
        Some(c) => c.as_str(),
        None => "host",
    };
    // On Windows the data folder is the host's alone (see `private`): made
    // so by the host and the service, which log what that did. Not by the
    // clipboard helper, which runs as the signed-in user; nor by the
    // hosting switch and its status, which the app runs, unelevated for
    // status and often before the host ever ran: a folder this created
    // would be that user's own.
    let secured = if matches!(command, "host" | "service") {
        private::secure_data_dir(&dir)
    } else {
        None
    };
    match command {
        // Clipboard sharing as the signed-in user, for the host (Windows).
        #[cfg(windows)]
        "clipboard-agent" => clipagent::run(&args),
        "host" => run_host(dir, &args, opened_by_hand, secured),
        #[cfg(windows)]
        "service" => run_service(&dir, secured),
        "enable" => hosting::switch(true),
        "disable" => hosting::switch(false),
        "status" => hosting::print_status(&dir, args.iter().any(|a| a == "--json")),
        other => {
            eprintln!(
                "helio-host: no command {other:?}. helio-host is what the system and Helio run \
                    (host, enable, disable, status); this computer's hosting is `helio host` \
                    (helio host --help)."
            );
            ExitCode::FAILURE
        }
    }
}

/// `helio-host host`: run the host in this session until it is asked to stop.
/// `args`: the command line (on a Mac, `--app PATH` from the LaunchAgent).
/// `opened_by_hand`: the host's helper app opened by hand (a Mac), which
/// brings its icon up too. `secured`: what making the data folder private did
/// that the log should say.
fn run_host(
    dir: std::path::PathBuf,
    args: &[String],
    opened_by_hand: bool,
    secured: Option<String>,
) -> ExitCode {
    let _guard = init_logging(&dir);
    if let Some(said) = secured {
        tracing::warn!("{said}");
    }
    // Run from its copy, the host stops with the app it came with.
    #[cfg(target_os = "macos")]
    if !hosting::watch_app(args) {
        return ExitCode::SUCCESS;
    }
    #[cfg(not(target_os = "macos"))]
    let _ = args;
    // Before the host: one already running (the LaunchAgent's) makes this
    // one fail to start, and the icon is still what was asked for.
    #[cfg(target_os = "macos")]
    if opened_by_hand {
        icon::show();
    }
    #[cfg(not(target_os = "macos"))]
    let _ = opened_by_hand;
    #[cfg(windows)]
    helio_capture::gpu::declare_dpi_aware();
    let host = match host::Host::open(dir) {
        Ok(host) => host,
        Err(e) => {
            tracing::error!("{e}");
            return ExitCode::FAILURE;
        }
    };
    #[cfg(windows)]
    {
        let h = host.clone();
        std::thread::spawn(move || {
            service::wait_for_stop_request();
            tracing::info!("the service asked the host to stop");
            h.shutdown();
        });
        endsession::watch(host.clone());
    }
    // Ctrl-C or a kill: end the session properly, so the client hears the
    // host is going.
    #[cfg(unix)]
    {
        #[cfg(target_os = "macos")]
        permissions::check();
        let h = host.clone();
        if let Err(e) = ctrlc::set_handler(move || {
            tracing::info!("asked to stop");
            h.shutdown();
        }) {
            tracing::warn!(
                error = %e,
                "cannot catch Ctrl-C; a stop will not end the session cleanly"
            );
        }
    }
    host.start_services();
    host.serve();
    #[cfg(windows)]
    endsession::stopped();
    ExitCode::SUCCESS
}

/// `helio-host service`: the Windows service, which keeps `helio-host host` running.
#[cfg(windows)]
fn run_service(dir: &std::path::Path, secured: Option<String>) -> ExitCode {
    let _guard = init_logging_named(dir, "helio-host-service.log");
    if let Some(said) = secured {
        tracing::warn!("{said}");
    }
    match service::run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}\n(`helio-host service` is started by Windows; use `helio host enable`)");
            ExitCode::FAILURE
        }
    }
}
