//! The host's program, `helio-host`, as Helio runs it: where it is, and its
//! hosting switch, `enable` and `disable`, run again through UAC on Windows
//! when it needs an administrator. The app's hosting switch
//! (helio-app's `host::switch`) and `helio host` run it so.

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

/// `program enable` or `program disable`, and what it said it did.
pub fn switch(program: &Path, on: bool) -> Result<String, Switched> {
    let out = command(program)
        .args(switch_args(on))
        .output()
        .map_err(|e| Switched::Failed(format!("{}: {e}", program.display())))?;
    match out.status.code() {
        Some(0) => Ok(String::from_utf8_lossy(&out.stdout).trim().to_string()),
        Some(NEEDS_ELEVATION) => Err(Switched::NeedsElevation),
        _ => Err(Switched::Failed(said(&out.stderr, &out.stdout))),
    }
}

/// `enable` or `disable`; on Windows, turning hosting on, with `--owner`
/// and this user's SID: the elevated `enable` may run as an administrator
/// who typed their password for this user, and it is this user's Helio,
/// not elevated, that reads the host's token (`helio-host`'s
/// `hosting::owner`).
fn switch_args(on: bool) -> Vec<String> {
    #[allow(unused_mut)]
    let mut args = vec![if on { "enable" } else { "disable" }.to_string()];
    #[cfg(windows)]
    if on {
        if let Some(sid) = user_sid() {
            args.extend(["--owner".to_string(), sid]);
        }
    }
    args
}

/// This process's user, as their SID's text (`S-1-5-21-…`).
#[cfg(windows)]
fn user_sid() -> Option<String> {
    use windows::core::PWSTR;
    use windows::Win32::Foundation::{CloseHandle, LocalFree, HANDLE, HLOCAL};
    use windows::Win32::Security::Authorization::ConvertSidToStringSidW;
    use windows::Win32::Security::{GetTokenInformation, TokenUser, TOKEN_QUERY, TOKEN_USER};
    use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
    // Room for a TOKEN_USER and its SID (at most 76 bytes), aligned for it.
    let mut buf = [0u64; 32];
    let mut len = 0u32;
    // SAFETY: the token is this process's own, closed once; `buf` is a local
    // of the size given, aligned for TOKEN_USER, and read as one only after
    // GetTokenInformation filled it; the string ConvertSidToStringSidW
    // allocates is copied, then freed once.
    unsafe {
        let mut token = HANDLE::default();
        OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token).ok()?;
        let got = GetTokenInformation(
            token,
            TokenUser,
            Some(buf.as_mut_ptr().cast()),
            std::mem::size_of_val(&buf) as u32,
            &mut len,
        );
        let _ = CloseHandle(token);
        got.ok()?;
        let user = &*(buf.as_ptr() as *const TOKEN_USER);
        let mut text = PWSTR::null();
        ConvertSidToStringSidW(user.User.Sid, &mut text).ok()?;
        let sid = text.to_string().ok();
        let _ = LocalFree(Some(HLOCAL(text.0.cast())));
        sid
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
    // Run as `helio`, the program may be a link to the app's (the Mac app's
    // Contents/Resources/bin/helio, Linux's ~/.local/bin/helio), which macOS
    // names as it was started: the app is where the link leads.
    let exe = std::env::current_exe().ok()?;
    let exe = exe.canonicalize().unwrap_or(exe);
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

/// Whether the hosting switch recorded who turned hosting on
/// (`C:\Program Files\Helio Host\owner`, readable by every user): not on a
/// host turned on by a Helio from before it did.
#[cfg(windows)]
pub fn owner_recorded() -> bool {
    let programs = std::env::var_os("ProgramFiles")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Program Files"));
    programs.join("Helio Host").join("owner").is_file()
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
    let args = HSTRING::from(switch_args(on).join(" "));
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
        let verb = if on { "enable" } else { "disable" };
        match code {
            0 => Ok(()),
            _ => Err(Switched::Failed(format!(
                "helio-host {verb} failed (exit code {code}). Run `helio host {verb}` in an \
                 administrator terminal to see why."
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
