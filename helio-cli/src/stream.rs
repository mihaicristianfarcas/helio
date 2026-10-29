//! `helio stream` and `helio watch`: the stream in a window of the platform's
//! own, until it ends.

use std::process::ExitCode;
#[cfg(any(target_os = "macos", windows))]
use std::sync::Arc;

#[cfg(any(target_os = "macos", windows))]
use helio_client::session::Session;
use helio_client::session::{EndCallback, StreamRequest};

use crate::script::run_script;

/// The request the flags make (checked already by `args::parse`), from
/// this display's mode.
fn request(flags: &[String], watch: bool) -> Result<StreamRequest, String> {
    crate::args::request(flags, &helio_client::session::native_mode(), watch)
}

#[cfg(target_os = "linux")]
pub fn run(name: &str, flags: &[String], watch: bool) -> ExitCode {
    let request = match request(flags, watch) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::FAILURE;
        }
    };
    let hooks = helio_client::linux::Hooks {
        stdin_quit: false,
        started: Some(Box::new(|input, controls, quit: EndCallback| {
            if let Ok(script) = std::env::var("HELIO_TEST_INPUT") {
                let quit = quit.clone();
                std::thread::spawn(move || run_script(&script, &input, Some(controls), quit));
            }
            let _ = ctrlc::set_handler(move || quit(None));
        })),
    };
    match helio_client::linux::run(name, &request, hooks) {
        Ok(None) => ExitCode::SUCCESS,
        Ok(Some(r)) => {
            eprintln!("{r}");
            ExitCode::FAILURE
        }
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}

/// Scripted input (HELIO_TEST_INPUT="wait 3000; key 01; move 100 0;
/// click; wheel 120") drives the stream without hands, for end-to-end
/// tests; Ctrl-C or SIGTERM end the session properly, so the host
/// restores its displays now rather than after its timeout.
#[cfg(any(target_os = "macos", windows))]
fn attach(session: &Session, on_end: &EndCallback) {
    if let (Ok(script), Some(input)) = (std::env::var("HELIO_TEST_INPUT"), session.input()) {
        let (controls, quit) = (session.controls(), on_end.clone());
        std::thread::spawn(move || run_script(&script, &input, controls, quit));
    }
    let quit = on_end.clone();
    let _ = ctrlc::set_handler(move || quit(None));
}

#[cfg(target_os = "macos")]
pub fn run(name: &str, flags: &[String], watch: bool) -> ExitCode {
    use std::cell::RefCell;

    use objc2::MainThreadMarker;
    use objc2_app_kit::{NSApplication, NSApplicationActivationPolicy};

    thread_local! {
        static SESSION: RefCell<Option<Session>> = const { RefCell::new(None) };
    }

    let Some(mtm) = MainThreadMarker::new() else {
        return ExitCode::FAILURE;
    };
    let app = NSApplication::sharedApplication(mtm);
    app.setActivationPolicy(NSApplicationActivationPolicy::Regular);
    let request = match request(flags, watch) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::FAILURE;
        }
    };

    // A reason means the stream ended without the user quitting: exit 1,
    // so scripts can tell (NSApplication's terminate would always exit 0).
    let on_end: EndCallback = Arc::new(|reason| {
        if let Some(r) = &reason {
            eprintln!("{r}");
        }
        let failed = reason.is_some();
        dispatch2::DispatchQueue::main().exec_async(move || {
            SESSION.with(|s| {
                if let Some(mut s) = s.borrow_mut().take() {
                    s.close();
                }
            });
            if failed {
                std::process::exit(1);
            }
            if let Some(mtm) = MainThreadMarker::new() {
                NSApplication::sharedApplication(mtm).terminate(None);
            }
        });
    });
    match helio_client::session::start(name, &request, on_end.clone()) {
        Ok(session) => {
            attach(&session, &on_end);
            SESSION.with(|s| *s.borrow_mut() = Some(session));
        }
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::FAILURE;
        }
    }
    app.run();
    ExitCode::SUCCESS
}

#[cfg(windows)]
pub fn run(name: &str, flags: &[String], watch: bool) -> ExitCode {
    let request = match request(flags, watch) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::FAILURE;
        }
    };
    let (tx, rx) = crossbeam_channel::bounded::<Option<String>>(1);
    let on_end: EndCallback = Arc::new(move |reason| {
        let _ = tx.try_send(reason);
    });
    let mut session = match helio_client::session::start(name, &request, on_end.clone()) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::FAILURE;
        }
    };
    attach(&session, &on_end);
    let reason = rx.recv().unwrap_or(None);
    session.close();
    match reason {
        Some(r) => {
            eprintln!("{r}");
            ExitCode::FAILURE
        }
        None => ExitCode::SUCCESS,
    }
}
