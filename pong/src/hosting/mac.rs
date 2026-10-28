//! Hosting on a Mac: the LaunchAgent `dev.pingpong.Pong`, which runs
//! `pong host` whenever the user is logged in and starts it again if it
//! exits.
//!
//! The host runs from an app bundle, never from a bare executable: macOS
//! grants Screen Recording and Accessibility to an app, by its bundle and
//! signature, so a host outside one could neither capture nor control
//! anything. The bundle is the helper inside Ping.app
//! (`Contents/Helpers/Pong Host.app`, `tools/build-ping-app`), which this
//! `pong` is the main program of; `pong enable` from anywhere else fails
//! and says so.
//!
//! The agent does not run the helper where it is, but a copy of it in the
//! host's data folder ([`installed`]), which `enable` makes and `disable`
//! removes, as Windows runs the engine's copy in Program Files. Looks
//! removable, is not: macOS files Screen Recording for code inside another
//! app under that app. Run from inside Ping.app, the host's request named
//! Ping, and the grant was kept under `dev.pingpong.Ping`, so Ping held it;
//! from `Contents/Library/LoginItems` the same; from outside any app, it is
//! the helper's own, "Pong Host" (`dev.pingpong.Pong`). Accessibility named
//! the helper either way (macOS 26.5, Developer ID and development
//! signatures alike).
//!
//! The agent is in `~/Library/LaunchAgents` and the copy in the user's own
//! data folder: nothing here needs an administrator. The Homebrew cask's
//! uninstall runs `pong disable`, then unloads the same label
//! (`tools/update-casks`).

use std::path::{Path, PathBuf};
use std::process::Output;
use std::time::Duration;

use super::{Error, State};

const LABEL: &str = "dev.pingpong.Pong";

/// What `pong enable` hands on to the host it starts: a data folder of its
/// own (a check's), so that the host and `pong status` read the same one;
/// and `PONG_NO_PROMPTS`, for checks run while someone works at the Mac.
const HANDED_ON: [&str; 2] = ["PONG_DATA_DIR", "PONG_NO_PROMPTS"];

/// launchctl's exit codes for "this agent is not loaded": `bootout` says 3
/// (No such process), `print` 113 (Could not find service), on macOS 26.
const NOT_LOADED: [i32; 2] = [3, 113];

/// The copy's name in the data folder.
const COPY: &str = "Pong Host.app";

/// The copy of the helper the agent runs: `~/Library/Application
/// Support/Pong/Pong Host.app`, or in `PONG_DATA_DIR`.
fn installed() -> PathBuf {
    crate::config::data_dir().join(COPY)
}

fn plist() -> Result<PathBuf, String> {
    let home = std::env::var_os("HOME")
        .filter(|h| !h.is_empty())
        .ok_or("There is no home folder to keep the host's LaunchAgent in.")?;
    Ok(PathBuf::from(home)
        .join("Library/LaunchAgents")
        .join(format!("{LABEL}.plist")))
}

/// This user's login session, as launchd names it.
fn domain() -> String {
    // SAFETY: getuid cannot fail and touches nothing.
    format!("gui/{}", unsafe { libc::getuid() })
}

fn launchctl(args: &[&str]) -> Result<Output, String> {
    std::process::Command::new("/bin/launchctl")
        .args(args)
        .output()
        .map_err(|e| format!("launchctl did not run: {e}"))
}

fn said(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).trim().to_string()
}

/// The host's executable: this `pong`, when it is an app bundle's.
fn host() -> Result<(PathBuf, PathBuf), String> {
    let exe = std::env::current_exe()
        .and_then(std::fs::canonicalize)
        .map_err(|e| format!("cannot tell where this pong is: {e}"))?;
    match super::bundle_of(&exe) {
        Some(bundle) => Ok((exe.clone(), bundle.to_path_buf())),
        None => Err(format!(
            "The host runs from its app bundle, so that macOS's Screen Recording and \
                Accessibility permissions apply to it, and this pong is not in one \
                ({}). Turn hosting on in Ping, which runs the pong inside it \
                (Ping.app/Contents/Helpers/Pong Host.app/Contents/MacOS/pong), or build the \
                app with tools/build-ping-app.",
            exe.display()
        )),
    }
}

/// `bundle` copied to `to`, replacing what was there: ditto, which keeps
/// the bundle's signature as it is. The copy also keeps the app's
/// quarantine flag, which by then says the user opened that app (a process
/// run from a quarantined file passes the flag on to the files it writes,
/// `--noqtn` or not): launchd runs it without asking (macOS 26.5, installed
/// by Homebrew). Copied beside first and renamed into place, so a copy that
/// broke off leaves none half made.
fn copy_bundle(bundle: &Path, to: &Path) -> Result<(), String> {
    let dir = to.parent().ok_or("the host's folder is unknown")?;
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let new = dir.join(format!(".{COPY}.new"));
    let _ = std::fs::remove_dir_all(&new);
    let out = std::process::Command::new("/usr/bin/ditto")
        .arg(bundle)
        .arg(&new)
        .output()
        .map_err(|e| format!("ditto did not run: {e}"))?;
    if !out.status.success() {
        let _ = std::fs::remove_dir_all(&new);
        return Err(format!("the host was not copied: {}", said(&out)));
    }
    remove_bundle(to)?;
    std::fs::rename(&new, to).map_err(|e| format!("{}: {e}", to.display()))
}

fn remove_bundle(bundle: &Path) -> Result<(), String> {
    match std::fs::remove_dir_all(bundle) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
            Err(format!("{}: {e}", bundle.display()))
        }
        _ => Ok(()),
    }
}

pub fn enable() -> Result<String, Error> {
    let (exe, bundle) = host()?;
    let copy = installed();
    let program = copy
        .join("Contents/MacOS")
        .join(exe.file_name().ok_or("this pong has no name")?);
    // One already loaded (an older one, or this one) goes first: launchd
    // refuses to load a second of a label, and its program is about to be
    // replaced. Not loaded is the usual case.
    stop()?;
    if bundle != copy {
        copy_bundle(&bundle, &copy)?;
    }
    let env: Vec<(&str, String)> = HANDED_ON
        .iter()
        .filter_map(|k| Some((*k, std::env::var(k).ok()?)))
        .collect();
    let path = plist()?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    std::fs::write(&path, super::launch_agent(LABEL, &program, &env))
        .map_err(|e| format!("{}: {e}", path.display()))?;
    let out = launchctl(&["bootstrap", &domain(), &path.to_string_lossy()])?;
    if !out.status.success() {
        return Err(Error::Failed(format!(
            "launchd did not start the host: {}",
            said(&out)
        )));
    }
    Ok(format!(
        "Hosting is on: the host runs from {} now and whenever you are logged in.",
        copy.display()
    ))
}

/// The host's process, while launchd runs it.
fn running_pid() -> Option<i32> {
    let out = launchctl(&["print", &format!("{}/{LABEL}", domain())]).ok()?;
    out.status
        .success()
        .then(|| super::launchd_pid(&String::from_utf8_lossy(&out.stdout)))?
}

/// Wait for process `pid` to end, at most `timeout`; whether it has.
/// kqueue says when it exits, so nothing is polled.
fn wait_for_exit(pid: i32, timeout: Duration) -> bool {
    // SAFETY: the queue is this function's own and closed once; `change`
    // and `event` live across the call that reads and fills them.
    unsafe {
        let queue = libc::kqueue();
        if queue < 0 {
            return false;
        }
        let change = libc::kevent {
            ident: pid as libc::uintptr_t,
            filter: libc::EVFILT_PROC,
            flags: libc::EV_ADD | libc::EV_ONESHOT,
            fflags: libc::NOTE_EXIT,
            data: 0,
            udata: std::ptr::null_mut(),
        };
        let mut event: libc::kevent = std::mem::zeroed();
        let wait = libc::timespec {
            tv_sec: timeout.as_secs() as libc::time_t,
            tv_nsec: 0,
        };
        let n = libc::kevent(queue, &change, 1, &mut event, 1, &wait);
        let errno = std::io::Error::last_os_error().raw_os_error();
        libc::close(queue);
        match n {
            // Exited; or gone before it could be watched (ESRCH).
            1 => event.flags & libc::EV_ERROR == 0 || event.data == libc::ESRCH as isize,
            -1 => errno == Some(libc::ESRCH),
            _ => false,
        }
    }
}

/// Unload the agent and wait for its host to stop. bootout asks the host to
/// stop (SIGTERM: it ends a session properly and puts the displays back)
/// and returns at once, while the host stops (macOS 26.5): wait for it, as
/// long as launchd would before killing it, and a little more. Loading the
/// label again before then fails ("Bootstrap failed: 5: Input/output
/// error"). Not loaded: nothing to do.
fn stop() -> Result<(), String> {
    let pid = running_pid();
    let out = launchctl(&["bootout", &format!("{}/{LABEL}", domain())])?;
    if !out.status.success() && !NOT_LOADED.contains(&out.status.code().unwrap_or(-1)) {
        return Err(format!("launchd did not stop the host: {}", said(&out)));
    }
    match pid {
        Some(pid) if !wait_for_exit(pid, Duration::from_secs(super::STOP_GRACE_S + 5)) => Err(
            format!("the host (process {pid}) is still stopping: try again in a moment"),
        ),
        _ => Ok(()),
    }
}

pub fn disable() -> Result<String, Error> {
    let stopped = stop();
    let path = plist()?;
    match std::fs::remove_file(&path) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
            return Err(Error::Failed(format!("{}: {e}", path.display())));
        }
        _ => {}
    }
    // The copy goes too, the running host's program or not: the process
    // keeps what it has open.
    remove_bundle(&installed())?;
    stopped?;
    Ok("Hosting is off: the host is stopped, and no longer starts at login.".into())
}

pub fn state() -> State {
    let installed = plist().is_ok_and(|p| Path::new(&p).exists());
    match launchctl(&["print", &format!("{}/{LABEL}", domain())]) {
        Ok(out) if out.status.success() => {
            super::launchd_state(&String::from_utf8_lossy(&out.stdout))
        }
        Ok(_) if installed => State::Failed(
            "the host's LaunchAgent is there but not loaded: it starts at your next \
                login, or now with pong enable"
                .into(),
        ),
        Ok(_) => State::Off,
        Err(e) => State::Failed(e),
    }
}
