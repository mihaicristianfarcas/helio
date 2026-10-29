//! The host's program, `helio-host`, as Helio runs it: where it is, and its
//! hosting switch, `enable` and `disable`, run again through UAC on Windows
//! when it needs an administrator. The app's hosting switch
//! (helio-app's `host::switch`) runs it so.

use std::path::{Path, PathBuf};

/// `helio-host enable` or `helio-host disable` without an administrator, on Windows
/// (`helio-host/src/hosting.rs`'s `NEEDS_ELEVATION`).
pub const NEEDS_ELEVATION: i32 = 740;

/// How the switch went wrong.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Switched {
    /// Windows: it needs an administrator (nothing was changed).
    NeedsElevation,
    /// Windows: the administrator was not given.
    Declined,
    Failed(String),
}

impl std::fmt::Display for Switched {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Switched::NeedsElevation => write!(f, "Windows needs an administrator for this."),
            Switched::Declined => write!(
                f,
                "Windows did not allow it, so nothing changed. Try again and choose Yes."
            ),
            Switched::Failed(e) => write!(f, "{e}"),
        }
    }
}

/// `program enable` or `program disable`.
pub fn switch(program: &Path, on: bool) -> Result<(), Switched> {
    let out = command(program)
        .arg(if on { "enable" } else { "disable" })
        .output()
        .map_err(|e| Switched::Failed(format!("{}: {e}", program.display())))?;
    match out.status.code() {
        Some(0) => Ok(()),
        Some(NEEDS_ELEVATION) => Err(Switched::NeedsElevation),
        _ => Err(Switched::Failed(said(&out.stderr, &out.stdout))),
    }
}

/// What a command printed when it failed: its error output, else its
/// output.
pub fn said(stderr: &[u8], stdout: &[u8]) -> String {
    let text = String::from_utf8_lossy(if stderr.iter().any(|b| !b.is_ascii_whitespace()) {
        stderr
    } else {
        stdout
    });
    text.trim().to_string()
}

/// Why there is no host to run, for a person.
pub fn no_program() -> String {
    if cfg!(target_os = "macos") {
        "Helio Host is not inside this app (Contents/Helpers): download Helio \
         again."
            .into()
    } else if cfg!(windows) {
        "helio-host.exe, the host, is not beside this app or in C:\\Program Files\\Helio Host."
            .into()
    } else {
        "helio-host is not beside this app or on the PATH.".into()
    }
}

/// A command run without a console window of its own (Windows shows one
/// for a console program started by a window app).
pub fn command(program: &Path) -> std::process::Command {
    #[allow(unused_mut)]
    let mut c = std::process::Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        /// `CREATE_NO_WINDOW`.
        const NO_WINDOW: u32 = 0x0800_0000;
        c.creation_flags(NO_WINDOW);
    }
    c
}

/// The host's program, where an install or a build puts it:
/// `HELIO_HOST_PROGRAM` (checks), then
///
/// - macOS: `Helio Host.app` inside this app's bundle (`Contents/Helpers`);
///   for a build run from `target/debug`, inside the `target/Helio.app`
///   that `tools/build-helio-app` makes, whose helper macOS has granted the
///   permissions to. Never a bare `helio-host`: the host runs only from a bundle
///   (`helio-host/src/hosting/mac.rs`).
/// - Windows: `helio-host.exe` beside this app, or the engine's own copy in
///   `C:\Program Files\Helio Host`.
/// - Linux: `helio-host` beside this app, or on the PATH.
pub fn host_program() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("HELIO_HOST_PROGRAM") {
        return Some(PathBuf::from(p));
    }
    let exe = std::env::current_exe().ok()?;
    candidates(&exe).into_iter().find(|p| p.is_file())
}

/// Where [`host_program`] looks, in order, for an app whose program is
/// `exe`.
fn candidates(exe: &Path) -> Vec<PathBuf> {
    let dir = exe.parent().map(Path::to_path_buf).unwrap_or_default();
    let mut out = Vec::new();
    if cfg!(target_os = "macos") {
        let helper = Path::new("Contents/Helpers/Helio Host.app/Contents/MacOS/helio-host");
        // X.app/Contents/MacOS/X: the bundle is three folders up.
        match dir.parent().and_then(Path::parent) {
            Some(bundle) if bundle.extension().is_some_and(|e| e == "app") => {
                out.push(bundle.join(helper));
            }
            _ => {
                if let Some(target) = dir.parent() {
                    out.push(target.join("Helio.app").join(helper));
                }
            }
        }
    } else if cfg!(windows) {
        out.push(dir.join("helio-host.exe"));
        let programs = std::env::var_os("ProgramFiles")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(r"C:\Program Files"));
        out.push(programs.join("Helio Host").join("helio-host.exe"));
    } else {
        out.push(dir.join("helio-host"));
        if let Some(path) = std::env::var_os("PATH") {
            out.extend(std::env::split_paths(&path).map(|p| p.join("helio-host")));
        }
    }
    out
}

/// `program enable` or `disable` through UAC: Windows asks the user, then
/// it runs elevated; this waits for it.
#[cfg(windows)]
pub fn switch_elevated(program: &Path, on: bool) -> Result<(), Switched> {
    use windows::core::{HSTRING, PCWSTR};
    use windows::Win32::Foundation::{CloseHandle, ERROR_CANCELLED};
    use windows::Win32::System::Threading::{GetExitCodeProcess, WaitForSingleObject, INFINITE};
    use windows::Win32::UI::Shell::{
        ShellExecuteExW, SEE_MASK_NOASYNC, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW,
    };
    use windows::Win32::UI::WindowsAndMessaging::SW_HIDE;

    let (verb, file) = (HSTRING::from("runas"), HSTRING::from(program.as_os_str()));
    let args = HSTRING::from(if on { "enable" } else { "disable" });
    let mut info = SHELLEXECUTEINFOW {
        cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOCLOSEPROCESS | SEE_MASK_NOASYNC,
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
                Switched::Declined
            } else {
                Switched::Failed(format!(
                    "Windows could not start helio-host: {}",
                    e.message()
                ))
            });
        }
        let process = info.hProcess;
        if process.is_invalid() {
            return Err(Switched::Failed(
                "Windows started helio-host without a handle to it.".into(),
            ));
        }
        WaitForSingleObject(process, INFINITE);
        let mut code = 1u32;
        let got = GetExitCodeProcess(process, &mut code);
        let _ = CloseHandle(process);
        got.map_err(|e| Switched::Failed(e.message()))?;
        match code {
            0 => Ok(()),
            _ => Err(Switched::Failed(format!(
                "helio-host {} failed (exit code {code}). Run it in an administrator terminal to see \
                 why.",
                if on { "enable" } else { "disable" }
            ))),
        }
    }
}

/// Elsewhere hosting never needs an administrator.
#[cfg(not(windows))]
pub fn switch_elevated(_: &Path, _: bool) -> Result<(), Switched> {
    Err(Switched::NeedsElevation)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_failure_says_what_the_command_printed() {
        assert_eq!(said(b"  \n", b"from stdout\n"), "from stdout");
        assert_eq!(said(b"why\n", b"other"), "why");
    }

    #[test]
    fn the_host_is_the_one_that_came_with_this_app() {
        if cfg!(target_os = "macos") {
            // Only the helper inside this app: never another copy's.
            let found = candidates(Path::new("/Applications/Helio.app/Contents/MacOS/Helio"));
            assert_eq!(
                found,
                [Path::new(
                    "/Applications/Helio.app/Contents/Helpers/Helio Host.app/Contents/MacOS/helio-host"
                )]
            );
            let dev = candidates(Path::new("/src/target/debug/helio-app"));
            assert_eq!(
                dev,
                [Path::new(
                    "/src/target/Helio.app/Contents/Helpers/Helio Host.app/Contents/MacOS/helio-host"
                )]
            );
        } else if cfg!(windows) {
            let found = candidates(Path::new(r"C:\Apps\Helio\Helio.exe"));
            assert_eq!(found[0], Path::new(r"C:\Apps\Helio\helio-host.exe"));
        } else {
            let found = candidates(Path::new("/opt/helio/helio-app"));
            assert_eq!(found[0], Path::new("/opt/helio/helio-host"));
        }
    }
}
