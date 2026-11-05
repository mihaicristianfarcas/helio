//! `helio` on Windows: `bin\helio.exe` in Helio's folder, which Settings >
//! General puts on the PATH (`command`). A console program
//! that runs `..\Helio.exe cli ARGS`, the app's program as the command line
//! (helio-app's `command_line`), and waits for it.
//!
//! Not Helio.exe itself: it is a window program (no console behind the
//! app), which a terminal neither waits for nor gives its console. Not a
//! `helio.exe` beside it: Windows' file names ignore case, so that would be
//! Helio.exe; and a folder on the PATH must not hold Helio.exe, or typing
//! `helio` opens the app. Not the command line itself either: a stream needs
//! FFmpeg's DLLs, which Windows loads from beside the program, and they are
//! beside Helio.exe.
//!
//! The app gets this program's standard handles (a terminal, a pipe, a
//! file), takes its console, so that Ctrl-C reaches it, and gives it its
//! exit code. Ctrl-C does not end this program: it waits for the app to end,
//! as the terminal waits for it. A job ends the app with this program, if it
//! is killed.
//!
//! Elsewhere `helio` is a link to the app's program, and this does nothing.

use std::process::ExitCode;

#[cfg(windows)]
fn main() -> ExitCode {
    use std::os::windows::io::AsRawHandle;

    use windows::core::BOOL;
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::System::Console::{SetConsoleCtrlHandler, CTRL_BREAK_EVENT, CTRL_C_EVENT};
    use windows::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
        SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
        JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    };

    /// Ctrl-C and Ctrl-Break are the app's to act on; anything else (the
    /// console closing) ends this program as it would.
    unsafe extern "system" fn leave_to_the_app(event: u32) -> BOOL {
        BOOL::from(event == CTRL_C_EVENT || event == CTRL_BREAK_EVENT)
    }

    let app = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent()?.parent().map(|dir| dir.join("Helio.exe")));
    let Some(app) = app.filter(|app| app.is_file()) else {
        eprintln!(
            "helio: Helio.exe is not in the folder above this one: install Helio again \
                (`helio` is bin\\helio.exe in Helio's folder)"
        );
        return ExitCode::FAILURE;
    };
    // SAFETY: the handler is a plain function that touches nothing, valid for
    // the program's life.
    let _ = unsafe { SetConsoleCtrlHandler(Some(leave_to_the_app), true) };
    // SAFETY: a job of this process's own, with no name and the default
    // security; its handle stays open, unclosed, until the process ends,
    // which is what ends the app with it.
    let job = unsafe { CreateJobObjectW(None, None) }.ok();
    if let Some(job) = job {
        let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        // SAFETY: `limits` is the structure the class names, and lives
        // across the call.
        let _ = unsafe {
            SetInformationJobObject(
                job,
                JobObjectExtendedLimitInformation,
                std::ptr::from_ref(&limits).cast(),
                std::mem::size_of_val(&limits) as u32,
            )
        };
    }
    let mut child = match std::process::Command::new(&app)
        .arg("cli")
        .args(std::env::args_os().skip(1))
        .spawn()
    {
        Ok(child) => child,
        Err(e) => {
            eprintln!("helio: {}: {e}", app.display());
            return ExitCode::FAILURE;
        }
    };
    if let Some(job) = job {
        // SAFETY: the child's handle is open while `child` lives.
        let _ = unsafe { AssignProcessToJobObject(job, HANDLE(child.as_raw_handle())) };
    }
    match child.wait() {
        Ok(status) => std::process::exit(status.code().unwrap_or(1)),
        Err(e) => {
            eprintln!("helio: {}: {e}", app.display());
            ExitCode::FAILURE
        }
    }
}

#[cfg(not(windows))]
fn main() -> ExitCode {
    eprintln!("helio-console is `helio` on Windows; elsewhere `helio` is a link to the app");
    ExitCode::FAILURE
}
