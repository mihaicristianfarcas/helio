//! Hosting, on or off: the switch in Settings → Hosting, setup and This
//! computer. It runs the host's own commands, `helio-host enable`, `helio-host disable`
//! and `helio-host status --json` (`helio-host/src/hosting.rs`), and does nothing of
//! its own to the system.
//!
//! - Windows: turning hosting on or off needs an administrator. Run as the
//!   user, `helio-host` changes nothing and exits with 740
//!   (`ERROR_ELEVATION_REQUIRED`); the same command then runs again
//!   through UAC (`ShellExecuteEx` with `runas`), and the app says it waits
//!   for Windows meanwhile.
//! - macOS: `helio-host` is the one in `Helio Host.app`, the helper inside this
//!   app that macOS grants Screen Recording and Accessibility to.
//! - Linux: the systemd user unit, no administrator.
//!
//! Finding the host's program and running it, through UAC too, is
//! `helio_cli::engine`'s, which `helio host` shares.
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
use helio_cli::engine::{
    command, host_program, no_program, said, switch, switch_elevated, Switched,
};

use super::link::Link;
use super::worker::Conn;

/// What `helio-host status --json` says.
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
    /// `helio-host enable` or `helio-host disable` (`on`) runs.
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
    /// `helio-host`, when there is one to run.
    program: Option<PathBuf>,
    /// Switches made: a state asked for before the last one is stale.
    switches: u64,
    /// A check made the state up (`hosting=…` in HELIO_UI_DEMO): nothing is run.
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
            switches: 0,
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

    /// Ask `helio-host status --json` again. Not while a switch is made: it asks
    /// once it is done.
    pub fn refresh(&mut self, cx: &mut Context<Self>) {
        if self.pretend || self.busy.is_some() {
            return;
        }
        let Some(program) = self.program.clone() else {
            self.set_state(State::Off, cx);
            return;
        };
        let switches = self.switches;
        let task = cx
            .background_executor()
            .spawn(async move { status(&program) });
        cx.spawn(async move |this, cx| {
            let state = task.await;
            let _ = this.update(cx, |this, cx| {
                if this.switches == switches && this.busy.is_none() {
                    this.take_status(state, cx);
                }
            });
        })
        .detach();
    }

    /// What `helio-host status` said, taken in.
    fn take_status(&mut self, said: Result<State, String>, cx: &mut Context<Self>) {
        match said {
            Ok(state) => self.set_state(state, cx),
            Err(e) => {
                tracing::warn!(error = e, "hosting: no status");
                self.set_state(State::Off, cx);
            }
        }
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
                let program = program.clone();
                done = cx
                    .background_executor()
                    .spawn(async move { switch_elevated(&program, on) })
                    .await;
            }
            // The state after, asked before the switch is done with: till
            // then the switch shows where it is going, not where it was
            // (it went back for a moment, then forward again).
            let state = cx
                .background_executor()
                .spawn(async move { status(&program) })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.busy = None;
                this.switches += 1;
                match done {
                    Ok(()) => tracing::info!(on, "hosting: switched"),
                    Err(e) => {
                        let e = e.to_string();
                        tracing::warn!(on, error = e, "hosting: not switched");
                        this.error = Some(e);
                    }
                }
                this.take_status(state, cx);
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
/// in HELIO_UI_DEMO.
fn pretended() -> Option<(State, Option<Busy>)> {
    let spec = std::env::var("HELIO_UI_DEMO").ok()?;
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

/// What `helio-host status --json` printed.
pub fn parse_status(text: &str) -> Result<State, String> {
    let v: serde_json::Value =
        serde_json::from_str(text.trim()).map_err(|e| format!("helio-host status: {e}"))?;
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
        other => return Err(format!("helio-host status: unknown state {other:?}")),
    })
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
}
