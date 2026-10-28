//! Hosting, on or off: the switch in Settings → Hosting, setup and This
//! computer. It runs the host's own commands, `pong enable`, `pong disable`
//! and `pong status --json` (`pong/src/hosting.rs`), and does nothing of
//! its own to the system.
//!
//! - Windows: turning hosting on or off needs an administrator. Run as the
//!   user, `pong` changes nothing and exits with 740
//!   (`ERROR_ELEVATION_REQUIRED`); the same command then runs again
//!   through UAC (`ShellExecuteEx` with `runas`), and the app says it waits
//!   for Windows meanwhile.
//! - macOS: `pong` is the one in `Pong.app`, the host helper macOS grants
//!   Screen Recording and Accessibility to (`host_program`).
//! - Linux: the systemd user unit, no administrator.
//!
//! The state is asked again only when something says it may have changed:
//! the app starting, a switch done, the host's API coming or going (the
//! link's connection), the window coming forward. Nothing looks on a timer.
//!
//! While hosting is on (or starting, or failed), a [`Link`] to the host's
//! API runs: the host's pages and the tray icon read it. Off, none does,
//! and nothing on this computer talks to a host.

use std::path::{Path, PathBuf};

use gpui::{App, AppContext, Context, Entity};

use super::link::Link;
use super::worker::Conn;

/// `pong enable` or `pong disable` without an administrator, on Windows
/// (`pong/src/hosting.rs`'s `NEEDS_ELEVATION`).
const NEEDS_ELEVATION: i32 = 740;

/// What `pong status --json` says.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum State {
    /// Not asked yet.
    #[default]
    Unknown,
    Off,
    /// The system is starting the host.
    Starting,
    /// The host runs; whether its API answers.
    On {
        api: bool,
    },
    /// Hosting is on, but the host does not run: why.
    Failed(String),
}

impl State {
    /// Hosting is switched on (whether or not the host runs yet).
    pub fn enabled(&self) -> bool {
        matches!(self, State::Starting | State::On { .. } | State::Failed(_))
    }
}

/// A switch being made.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Busy {
    /// `pong enable` or `pong disable` (`on`) runs.
    Switching { on: bool },
    /// Windows asks whether the app may (UAC); then it runs elevated.
    Elevating { on: bool },
}

pub struct Hosting {
    pub state: State,
    pub busy: Option<Busy>,
    /// Why the last switch did not happen, for a person.
    pub error: Option<String>,
    /// The host's API, while hosting is on.
    pub link: Option<Entity<Link>>,
    /// The connection last seen on the link: its changes ask the state again.
    seen: Option<Conn>,
    /// `pong`, when there is one to run.
    program: Option<PathBuf>,
    /// A check made the state up (`hosting=…` in PING_UI_DEMO): nothing is run.
    pretend: bool,
}

impl Hosting {
    /// The switch, and its first look at the state.
    pub fn start(cx: &mut App) -> Entity<Hosting> {
        let pretended = pretended();
        let hosting = cx.new(|_| Hosting {
            state: State::Unknown,
            busy: None,
            error: None,
            link: None,
            seen: None,
            program: host_program(),
            pretend: pretended.is_some(),
        });
        hosting.update(cx, |h, cx| match pretended {
            Some((state, busy)) => {
                h.busy = busy;
                h.set_state(state, cx);
            }
            None => h.refresh(cx),
        });
        hosting
    }

    /// Where the host's program is, if there is one.
    pub fn program(&self) -> Option<&Path> {
        self.program.as_deref()
    }

    /// Ask `pong status --json` again.
    pub fn refresh(&mut self, cx: &mut Context<Self>) {
        if self.pretend {
            return;
        }
        let Some(program) = self.program.clone() else {
            self.set_state(State::Off, cx);
            return;
        };
        let task = cx
            .background_executor()
            .spawn(async move { status(&program) });
        cx.spawn(async move |this, cx| {
            let state = task.await;
            let _ = this.update(cx, |this, cx| match state {
                Ok(state) => this.set_state(state, cx),
                Err(e) => {
                    tracing::warn!(error = e, "hosting: no status");
                    this.set_state(State::Off, cx);
                }
            });
        })
        .detach();
    }

    /// Turn hosting on or off.
    pub fn turn(&mut self, on: bool, cx: &mut Context<Self>) {
        if self.busy.is_some() {
            return;
        }
        self.error = None;
        if self.pretend {
            self.set_state(
                if on {
                    State::On { api: true }
                } else {
                    State::Off
                },
                cx,
            );
            return;
        }
        let Some(program) = self.program.clone() else {
            self.error = Some(no_program());
            cx.notify();
            return;
        };
        tracing::info!(on, program = %program.display(), "hosting: switching");
        self.busy = Some(Busy::Switching { on });
        cx.notify();
        let first = cx.background_executor().spawn({
            let program = program.clone();
            async move { switch(&program, on) }
        });
        cx.spawn(async move |this, cx| {
            let mut done = first.await;
            if done == Err(Switched::NeedsElevation) {
                let _ = this.update(cx, |this, cx| {
                    this.busy = Some(Busy::Elevating { on });
                    cx.notify();
                });
                tracing::info!(on, "hosting: asking Windows for an administrator");
                done = cx
                    .background_executor()
                    .spawn(async move { switch_elevated(&program, on) })
                    .await;
            }
            let _ = this.update(cx, |this, cx| {
                this.busy = None;
                match done {
                    Ok(()) => tracing::info!(on, "hosting: switched"),
                    Err(e) => {
                        let e = e.to_string();
                        tracing::warn!(on, error = e, "hosting: not switched");
                        this.error = Some(e);
                    }
                }
                this.refresh(cx);
                cx.notify();
            });
        })
        .detach();
    }

    fn set_state(&mut self, state: State, cx: &mut Context<Self>) {
        if state != self.state {
            tracing::info!(?state, "hosting");
        }
        let enabled = state.enabled();
        self.state = state;
        match (enabled, &self.link) {
            (true, None) => {
                let link = Link::start(cx);
                // The host's API coming or going may be hosting failing or
                // coming back: ask again then.
                cx.observe(&link, |this, link, cx| {
                    let conn = link.read(cx).snap.conn.clone();
                    let changed = this.seen.as_ref().is_some_and(|seen| {
                        seen != &conn && (*seen == Conn::Ready || conn == Conn::Ready)
                    });
                    this.seen = Some(conn);
                    if changed {
                        this.refresh(cx);
                    }
                    cx.notify();
                })
                .detach();
                self.link = Some(link);
            }
            (false, Some(_)) => {
                // Dropping the link stops its worker.
                self.link = None;
                self.seen = None;
            }
            _ => {}
        }
        cx.notify();
    }
}

/// What a check makes up: `hosting=off|on|starting|failed|switching|elevating`
/// in PING_UI_DEMO.
fn pretended() -> Option<(State, Option<Busy>)> {
    let spec = std::env::var("PING_UI_DEMO").ok()?;
    let value = spec
        .split(',')
        .find_map(|part| part.trim().strip_prefix("hosting="))?;
    Some(match value {
        "on" => (State::On { api: true }, None),
        "starting" => (State::Starting, None),
        "failed" => (State::Failed("the host exited with code 3".into()), None),
        "switching" => (State::Off, Some(Busy::Switching { on: true })),
        "elevating" => (State::Off, Some(Busy::Elevating { on: true })),
        _ => (State::Off, None),
    })
}

/// How the switch went wrong.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Switched {
    /// Windows: it needs an administrator (nothing was changed).
    NeedsElevation,
    /// Windows: the administrator was not given.
    #[cfg_attr(not(windows), allow(dead_code))]
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
fn switch(program: &Path, on: bool) -> Result<(), Switched> {
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

/// `program status --json`, read.
fn status(program: &Path) -> Result<State, String> {
    let out = command(program)
        .args(["status", "--json"])
        .output()
        .map_err(|e| format!("{}: {e}", program.display()))?;
    if !out.status.success() {
        return Err(said(&out.stderr, &out.stdout));
    }
    parse_status(&String::from_utf8_lossy(&out.stdout))
}

/// What `pong status --json` printed.
pub fn parse_status(text: &str) -> Result<State, String> {
    let v: serde_json::Value =
        serde_json::from_str(text.trim()).map_err(|e| format!("pong status: {e}"))?;
    Ok(match v.get("state").and_then(|s| s.as_str()) {
        Some("off") => State::Off,
        Some("starting") => State::Starting,
        Some("on") => State::On {
            api: v.get("api").and_then(|a| a.as_bool()).unwrap_or(false),
        },
        Some("failed") => State::Failed(
            v.get("reason")
                .and_then(|r| r.as_str())
                .unwrap_or("the host does not run")
                .to_string(),
        ),
        other => return Err(format!("pong status: unknown state {other:?}")),
    })
}

/// What a command printed when it failed: its error output, else its
/// output.
fn said(stderr: &[u8], stdout: &[u8]) -> String {
    let text = String::from_utf8_lossy(if stderr.iter().any(|b| !b.is_ascii_whitespace()) {
        stderr
    } else {
        stdout
    });
    text.trim().to_string()
}

fn no_program() -> String {
    if cfg!(target_os = "macos") {
        "Pong.app, the host, is not beside this app or in Applications.".into()
    } else if cfg!(windows) {
        "pong.exe, the host, is not beside this app or in C:\\Program Files\\Pong.".into()
    } else {
        "pong, the host, is not beside this app or on the PATH.".into()
    }
}

/// A command run without a console window of its own (Windows shows one
/// for a console program started by a window app).
fn command(program: &Path) -> std::process::Command {
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
/// `PING_HOST_PROGRAM` (checks), then
///
/// - macOS: `Pong.app` inside this app's bundle (`Contents/Helpers`),
///   beside it, in /Applications, or beside the folder the program runs
///   from (`target/Pong.app` from `tools/build-pong-app`, for a build run
///   from `target/debug`).
/// - Windows: `pong.exe` beside this app, or the engine's own copy in
///   `C:\Program Files\Pong`.
/// - Linux: `pong` beside this app, or on the PATH.
pub fn host_program() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("PING_HOST_PROGRAM") {
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
        let helper = Path::new("Pong.app/Contents/MacOS/pong");
        // X.app/Contents/MacOS/X: the bundle is three folders up.
        if let Some(bundle) = dir.parent().and_then(Path::parent) {
            if bundle.extension().is_some_and(|e| e == "app") {
                out.push(bundle.join("Contents/Helpers").join(helper));
                if let Some(beside) = bundle.parent() {
                    out.push(beside.join(helper));
                }
            }
        }
        out.push(Path::new("/Applications").join(helper));
        if let Some(up) = dir.parent() {
            out.push(up.join(helper));
        }
    } else if cfg!(windows) {
        out.push(dir.join("pong.exe"));
        let programs = std::env::var_os("ProgramFiles")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(r"C:\Program Files"));
        out.push(programs.join("Pong").join("pong.exe"));
    } else {
        out.push(dir.join("pong"));
        if let Some(path) = std::env::var_os("PATH") {
            out.extend(std::env::split_paths(&path).map(|p| p.join("pong")));
        }
    }
    out
}

/// `program enable` or `disable` through UAC: Windows asks the user, then
/// it runs elevated; this waits for it.
#[cfg(windows)]
fn switch_elevated(program: &Path, on: bool) -> Result<(), Switched> {
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
                Switched::Failed(format!("Windows could not start pong: {}", e.message()))
            });
        }
        let process = info.hProcess;
        if process.is_invalid() {
            return Err(Switched::Failed(
                "Windows started pong without a handle to it.".into(),
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
                "pong {} failed (exit code {code}). Run it in an administrator terminal to see \
                 why.",
                if on { "enable" } else { "disable" }
            ))),
        }
    }
}

/// Elsewhere hosting never needs an administrator.
#[cfg(not(windows))]
fn switch_elevated(_: &Path, _: bool) -> Result<(), Switched> {
    Err(Switched::NeedsElevation)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_status_the_host_prints_is_read() {
        assert_eq!(parse_status(r#"{"state":"off"}"#), Ok(State::Off));
        assert_eq!(
            parse_status("{\"state\": \"on\", \"api\": true}\n"),
            Ok(State::On { api: true })
        );
        assert_eq!(
            parse_status(r#"{"state":"failed","reason":"the host exited with code 3"}"#),
            Ok(State::Failed("the host exited with code 3".into()))
        );
        assert!(parse_status(r#"{"state":"sideways"}"#).is_err());
        assert!(parse_status("not json").is_err());
        assert!(State::Failed(String::new()).enabled() && !State::Unknown.enabled());
    }

    #[test]
    fn a_failure_says_what_the_command_printed() {
        assert_eq!(said(b"  \n", b"from stdout\n"), "from stdout");
        assert_eq!(said(b"why\n", b"other"), "why");
    }

    #[test]
    fn the_host_is_looked_for_beside_this_app_first() {
        if cfg!(target_os = "macos") {
            let found = candidates(Path::new("/Applications/Ping.app/Contents/MacOS/Ping"));
            assert_eq!(
                found[0],
                Path::new("/Applications/Ping.app/Contents/Helpers/Pong.app/Contents/MacOS/pong")
            );
            assert_eq!(
                found[1],
                Path::new("/Applications/Pong.app/Contents/MacOS/pong")
            );
            let dev = candidates(Path::new("/src/target/debug/ping-app"));
            assert!(dev.contains(&PathBuf::from("/src/target/Pong.app/Contents/MacOS/pong")));
        } else if cfg!(windows) {
            let found = candidates(Path::new(r"C:\Apps\Ping\Ping.exe"));
            assert_eq!(found[0], Path::new(r"C:\Apps\Ping\pong.exe"));
        } else {
            let found = candidates(Path::new("/opt/ping/ping-app"));
            assert_eq!(found[0], Path::new("/opt/ping/pong"));
        }
    }
}
