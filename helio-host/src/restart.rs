//! Restarting the host on its own, for what it takes in only when it starts:
//! its name and ports (the listeners and the LAN advertisement are made
//! once), the NVIDIA driver settings (applied at start, `nvprefs`), its
//! clients when the command line changed them, and on a Mac a Screen
//! Recording grant (macOS answers a running process what it answered when
//! the process first asked).
//!
//! Sunshine restarts at once when its web UI's Apply is pressed
//! (`confighttp.cpp`'s `restart`). Helio Host restarts at once only when no
//! session runs; otherwise when the session ends, so a change made while
//! someone streams (from the stream itself, often) does not cut the stream.
//!
//! The restart is the host's ordinary stop (the session ends properly and
//! the displays are put back), then:
//!
//! - macOS and Linux: the program again, in this process (`exec`). launchd
//!   and systemd see the same process go on; systemd would not start a host
//!   that exited cleanly again (`Restart=on-failure`), and launchd waits up
//!   to 10 s before starting one that exited soon after it started.
//! - Windows: the host exits with [`EXIT_CODE`], and HelioHost starts it
//!   again at once (`service`), instead of the 3 s it waits after a host
//!   that stopped for another reason.

use std::process::ExitCode;
use std::sync::atomic::{AtomicU8, Ordering};

use parking_lot::Mutex;

/// The exit code that asks HelioHost to start the host again at once:
/// `EX_TEMPFAIL` (sysexits.h), "try again", which nothing else here exits
/// with.
pub const EXIT_CODE: u8 = 75;

/// When a restart that was asked for happens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum When {
    /// No session runs: the host stops now and starts again.
    Now,
    /// When the session that runs ends.
    AfterSession,
}

impl When {
    /// As the API says it.
    pub fn word(self) -> &'static str {
        match self {
            When::Now => "now",
            When::AfterSession => "after_session",
        }
    }
}

const NONE: u8 = 0;
const DUE: u8 = 1;
const NOW: u8 = 2;

/// Whether a restart is due, shared by whoever asks (the API, the
/// session thread) and the receive loop, which acts on it.
#[derive(Default)]
pub struct Restart {
    state: AtomicU8,
    why: Mutex<Option<String>>,
}

impl Restart {
    /// Ask for a restart, for `why` (said in the log). `running`: whether a
    /// session runs as this is asked. A session that ends meanwhile still
    /// brings it on: the due mark is set before `running` is looked at, and
    /// [`Restart::session_ended`] looks at the mark after the session is
    /// marked gone.
    pub fn request(&self, why: &str, running: impl FnOnce() -> bool) -> When {
        *self.why.lock() = Some(why.to_string());
        // Never back from NOW to DUE.
        let _ = self
            .state
            .compare_exchange(NONE, DUE, Ordering::SeqCst, Ordering::SeqCst);
        if self.now() {
            return When::Now;
        }
        if running() {
            tracing::info!(why, "the host restarts when the session ends");
            When::AfterSession
        } else {
            self.state.store(NOW, Ordering::SeqCst);
            tracing::info!(why, "the host restarts");
            When::Now
        }
    }

    /// The session ended: a restart that waited for it is due now.
    pub fn session_ended(&self) {
        if self
            .state
            .compare_exchange(DUE, NOW, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
        {
            tracing::info!(
                why = self.why.lock().as_deref().unwrap_or(""),
                "the session ended: the host restarts"
            );
        }
    }

    /// Whether the host is to stop now and start again.
    pub fn now(&self) -> bool {
        self.state.load(Ordering::SeqCst) == NOW
    }

    /// A restart asked for and not yet begun, or begun (None: neither).
    pub fn pending(&self) -> Option<When> {
        match self.state.load(Ordering::SeqCst) {
            DUE => Some(When::AfterSession),
            NOW => Some(When::Now),
            _ => None,
        }
    }
}

/// Start the host again, once it has stopped (see the module's doc). `log`
/// is the log's writer, flushed before the program is replaced. Returns
/// only if it could not: the exit code that still gets the host started
/// again by what runs it.
pub fn start_again(log: tracing_appender::non_blocking::WorkerGuard) -> ExitCode {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        let program = program();
        tracing::info!(program = %program.display(), "starting again");
        drop(log);
        let e = std::process::Command::new(&program)
            .args(std::env::args_os().skip(1))
            .exec();
        // The log's writer is gone: stderr reaches launchd's or the journal.
        eprintln!(
            "helio-host could not start again ({e}); exiting for launchd or systemd to start it"
        );
        ExitCode::from(EXIT_CODE)
    }
    #[cfg(windows)]
    {
        tracing::info!("exiting for HelioHost to start the host again");
        drop(log);
        ExitCode::from(EXIT_CODE)
    }
}

/// This program, by the path it was started with when that is whole (the
/// LaunchAgent's and the systemd unit's are): on Linux the running file's
/// own path reads "... (deleted)" once an update has replaced it.
#[cfg(unix)]
fn program() -> std::path::PathBuf {
    std::env::args_os()
        .next()
        .map(std::path::PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| std::env::current_exe().ok())
        .unwrap_or_else(|| "helio-host".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_restart_asked_while_idle_is_due_at_once() {
        let r = Restart::default();
        assert_eq!(r.request("name", || false), When::Now);
        assert!(r.now());
        assert_eq!(r.pending(), Some(When::Now));
    }

    #[test]
    fn a_restart_asked_during_a_session_waits_for_its_end() {
        let r = Restart::default();
        assert_eq!(r.request("name", || true), When::AfterSession);
        assert!(!r.now());
        assert_eq!(r.pending(), Some(When::AfterSession));
        r.session_ended();
        assert!(r.now());
    }

    #[test]
    fn a_session_ending_with_no_restart_asked_changes_nothing() {
        let r = Restart::default();
        r.session_ended();
        assert!(!r.now());
        assert_eq!(r.pending(), None);
    }

    #[test]
    fn a_session_that_ends_while_the_restart_is_asked_still_brings_it_on() {
        // The session thread marks the session gone and calls
        // session_ended between the due mark and the look at `running`,
        // which still saw the session.
        let r = Restart::default();
        let when = r.request("port", || {
            r.session_ended();
            true
        });
        assert_eq!(when, When::AfterSession);
        assert!(r.now());
    }

    #[test]
    fn a_restart_begun_is_not_put_off_by_a_later_ask() {
        let r = Restart::default();
        r.request("name", || false);
        assert_eq!(r.request("port", || true), When::Now);
        assert!(r.now());
    }
}
