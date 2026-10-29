//! Hosting on a Mac: the LaunchAgent `dev.mihaicristian.helio.host`, which runs
//! `helio-host host` whenever the user is logged in and starts it again if it
//! exits.
//!
//! The host runs from an app bundle, never from a bare executable: macOS
//! grants Screen Recording and Accessibility to an app, by its bundle and
//! signature, so a host outside one could neither capture nor control
//! anything. The bundle is the helper inside Helio.app
//! (`Contents/Helpers/Helio Host.app`, `tools/build-helio-app`), which this
//! `helio-host` is the main program of; `helio-host enable` from anywhere else fails
//! and says so.
//!
//! The agent does not run the helper where it is, but a copy of it in the
//! host's data folder ([`installed`]), which `enable` makes and `disable`
//! removes, as Windows runs the engine's copy in Program Files. Looks
//! removable, is not: macOS files Screen Recording for code inside another
//! app under that app. Run from inside Helio.app, the host's request named
//! Helio, and the grant was kept under `dev.mihaicristian.helio`, so Helio held it;
//! from `Contents/Library/LoginItems` the same; from outside any app, it is
//! the helper's own, "Helio Host" (`dev.mihaicristian.helio.host`). Accessibility named
//! the helper either way (macOS 26.5, Developer ID and development
//! signatures alike).
//!
//! A copy outside the app has to follow it. So the agent names the app the
//! copy came from (`helio-host host --app PATH`), and the host watches that app
//! ([`watch_app`]):
//!
//! - replaced (an update: Homebrew's, the app's own, a download dragged over
//!   it), the host moves onto the new app's helper, once that is known to
//!   be signed as the running copy is and Gatekeeper accepts it, and
//!   restarts on it;
//! - gone (uninstalled, dragged to the Trash), the host turns hosting off,
//!   rather than start at every login with no app to turn it off.
//!
//! Every update path ends the same way, with hosting as it was, and needs
//! nothing of the package manager: Homebrew upgrades a cask by uninstalling
//! and installing it, tells neither step which it is doing, and runs its
//! install steps in a sandbox with a home folder of its own, where no
//! LaunchAgent could be loaded (`tools/update-casks` says more).
//!
//! The agent is in `~/Library/LaunchAgents` and the copy in the user's own
//! data folder: nothing here needs an administrator.

use std::path::{Path, PathBuf};
use std::process::Output;
use std::time::Duration;

use super::{Error, State};

const LABEL: &str = "dev.mihaicristian.helio.host";

/// What `helio-host enable` hands on to the host it starts: a data folder of its
/// own (a check's), so that the host and `helio-host status` read the same one;
/// and `HELIO_HOST_NO_PROMPTS`, for checks run while someone works at the Mac.
const HANDED_ON: [&str; 2] = ["HELIO_HOST_DATA_DIR", "HELIO_HOST_NO_PROMPTS"];

/// launchctl's exit codes for "this agent is not loaded": `bootout` says 3
/// (No such process), `print` 113 (Could not find service), on macOS 26.
const NOT_LOADED: [i32; 2] = [3, 113];

/// The copy's name in the data folder.
const COPY: &str = "Helio Host.app";

/// The copy of the helper the agent runs: `~/Library/Application
/// Support/Helio Host/Helio Host.app`, or in `HELIO_HOST_DATA_DIR`.
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

/// The host's executable: this `helio-host`, when it is an app bundle's.
fn host() -> Result<(PathBuf, PathBuf), String> {
    let exe = std::env::current_exe()
        .and_then(std::fs::canonicalize)
        .map_err(|e| format!("cannot tell where this helio-host is: {e}"))?;
    match super::bundle_of(&exe) {
        Some(bundle) => Ok((exe.clone(), bundle.to_path_buf())),
        None => Err(format!(
            "The host runs from its app bundle, so that macOS's Screen Recording and \
                Accessibility permissions apply to it, and this helio-host is not in one \
                ({}). Turn hosting on in Helio, which runs the helio-host inside it \
                (Helio.app/Contents/Helpers/Helio Host.app/Contents/MacOS/helio-host), or build the \
                app with tools/build-helio-app.",
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
        .join(exe.file_name().ok_or("this helio-host has no name")?);
    // One already loaded (an older one, or this one) goes first: launchd
    // refuses to load a second of a label, and its program is about to be
    // replaced. Not loaded is the usual case.
    stop()?;
    if bundle != copy {
        copy_bundle(&bundle, &copy)?;
    }
    // The app this helper is inside, which the host watches; none for a
    // helper on its own.
    let args: Vec<String> = super::app_holding(&bundle)
        .map(|app| vec!["--app".into(), app.to_string_lossy().into_owned()])
        .unwrap_or_default();
    let env: Vec<(&str, String)> = HANDED_ON
        .iter()
        .filter_map(|k| Some((*k, std::env::var(k).ok()?)))
        .collect();
    let path = plist()?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    std::fs::write(&path, super::launch_agent(LABEL, &program, &args, &env))
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

/// After the app was changed, how long the host waits before it looks
/// again. Updates put the new app in place a moment after the old one
/// goes: Homebrew's upgrade deletes what is in the app folder and moves the
/// new app's contents in (the whole upgrade took 2 to 3 s, the download's
/// check included, Homebrew 7.0 on macOS 26.5), the app's own update swaps
/// the folder by two renames, and replacing it in the Finder is one. 15 s
/// is far longer than any of those, and short enough that a host whose app
/// was uninstalled stops soon after.
const APP_GONE_AFTER: Duration = Duration::from_secs(15);

/// `helio-host host --app APP`, started by the agent: follow APP, or turn hosting
/// off if it is gone, now or later. Returns false when this process is not
/// to go on (APP gone, or this host replaced by APP's newer helper);
/// otherwise watches APP on a thread of its own. Without `--app` (a host
/// started by hand, a helper on its own): true, and nothing is watched.
pub fn watch_app(args: &[String]) -> bool {
    let Some(app) = args
        .iter()
        .position(|a| a == "--app")
        .and_then(|i| args.get(i + 1))
        .map(PathBuf::from)
    else {
        return true;
    };
    if !app.is_dir() {
        retire(&app);
        return false;
    }
    // Updated while no host ran (logged out, say): onto the new helper
    // before anything listens, in this same process.
    if !same_program(&helper_in(&app), &installed()) && follow(&app) {
        restart_in_place(args);
    }
    let spawned = std::thread::Builder::new()
        .name("app-watch".into())
        .spawn(move || loop {
            if !wait_for_change(&app) {
                // Gone before it could be watched again (just after an
                // update the host did not follow, say).
                if !app.is_dir() {
                    retire(&app);
                } else {
                    tracing::warn!(app = %app.display(), "cannot watch the app the host came with");
                }
                return;
            }
            // A single wait until it is due to look again, not polling.
            std::thread::sleep(APP_GONE_AFTER);
            if !app.is_dir() {
                retire(&app);
                return;
            }
            // The same build put back (a download dragged over it again):
            // nothing to restart for.
            if same_program(&helper_in(&app), &installed()) {
                continue;
            }
            if follow(&app) {
                // SAFETY: a signal to this process, which handles SIGTERM
                // by stopping the host as `helio-host disable` would; launchd
                // (KeepAlive) then starts the new copy.
                unsafe { libc::kill(libc::getpid(), libc::SIGTERM) };
                return;
            }
        });
    if let Err(e) = spawned {
        tracing::warn!(error = %e, "cannot watch the app the host came with");
    }
    true
}

/// Block until `path` (a folder) is moved, renamed or deleted, or what is
/// in it changes; false if it cannot be watched. kqueue says when, so
/// nothing is polled. What is in it: Homebrew's upgrade keeps the app
/// folder and replaces its `Contents` (Homebrew's `cask/artifact/moved.rb`:
/// macOS takes a deleted app folder for an uninstalled app), so the folder
/// is neither moved nor deleted, only written (`NOTE_WRITE`: an entry
/// removed or added).
fn wait_for_change(path: &Path) -> bool {
    use std::os::unix::ffi::OsStrExt;
    let Ok(c_path) = std::ffi::CString::new(path.as_os_str().as_bytes()) else {
        return false;
    };
    // SAFETY: `c_path` is a valid C string; the descriptor and the queue
    // are this function's own and closed once each; `change` and `event`
    // live across the call that reads and fills them.
    unsafe {
        let fd = libc::open(c_path.as_ptr(), libc::O_EVTONLY);
        if fd < 0 {
            return false;
        }
        let queue = libc::kqueue();
        if queue < 0 {
            libc::close(fd);
            return false;
        }
        let change = libc::kevent {
            ident: fd as libc::uintptr_t,
            filter: libc::EVFILT_VNODE,
            flags: libc::EV_ADD | libc::EV_CLEAR,
            fflags: libc::NOTE_DELETE | libc::NOTE_RENAME | libc::NOTE_REVOKE | libc::NOTE_WRITE,
            data: 0,
            udata: std::ptr::null_mut(),
        };
        let mut event: libc::kevent = std::mem::zeroed();
        let n = loop {
            let n = libc::kevent(queue, &change, 1, &mut event, 1, std::ptr::null());
            if n >= 0 || std::io::Error::last_os_error().kind() != std::io::ErrorKind::Interrupted {
                break n;
            }
        };
        libc::close(queue);
        libc::close(fd);
        n == 1 && event.flags & libc::EV_ERROR == 0
    }
}

/// The helper inside `app`, which the host's copy is made from (named as
/// the copy is).
fn helper_in(app: &Path) -> PathBuf {
    app.join("Contents/Helpers").join(COPY)
}

/// Whether the bundles `a` and `b` have the same program, byte for byte (a
/// few MB each, read at start and when the app was replaced).
fn same_program(a: &Path, b: &Path) -> bool {
    let program = |bundle: &Path| std::fs::read(bundle.join("Contents/MacOS/helio-host")).ok();
    matches!((program(a), program(b)), (Some(a), Some(b)) if a == b)
}

/// What `bundle`'s signature requires of code that is to count as it: its
/// designated requirement (`codesign -d -r-`), the one macOS keeps TCC's
/// grants by.
fn requirement_of(bundle: &Path) -> Result<String, String> {
    let out = std::process::Command::new("/usr/bin/codesign")
        .args(["-d", "-r-"])
        .arg(bundle)
        .output()
        .map_err(|e| format!("codesign did not run: {e}"))?;
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    designated(&text).ok_or_else(|| format!("{} has no designated requirement", bundle.display()))
}

/// The designated requirement in what `codesign -d -r-` printed.
fn designated(said: &str) -> Option<String> {
    said.lines()
        .find_map(|l| l.trim().strip_prefix("designated => "))
        .map(str::to_string)
        .filter(|r| !r.is_empty())
}

/// Whether `bundle` is whole and meets `requirement`.
fn satisfies(bundle: &Path, requirement: &str) -> Result<(), String> {
    let out = std::process::Command::new("/usr/bin/codesign")
        .args(["--verify", "--strict", "-R"])
        .arg(format!("={requirement}"))
        .arg(bundle)
        .output()
        .map_err(|e| format!("codesign did not run: {e}"))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(said(&out))
    }
}

/// Whether `path` is still marked as downloaded and never opened (its
/// quarantine flag), which has Gatekeeper check it before it runs.
fn quarantined(path: &Path) -> bool {
    std::process::Command::new("/usr/bin/xattr")
        .args(["-p", "com.apple.quarantine"])
        .arg(path)
        .output()
        .is_ok_and(|out| out.status.success())
}

/// Gatekeeper's verdict on `app`, when its `helper` came in a download no
/// one has opened (quarantined, as Homebrew's upgrade leaves it): what
/// opening it would have checked beyond the signature, its notarization
/// among it. The app, not the helper: the app carries the stapled ticket,
/// so this holds offline too (0.6 s on this Mac, macOS 26.5).
fn gatekeeper_accepts(app: &Path, helper: &Path) -> Result<(), String> {
    if !quarantined(helper) {
        return Ok(());
    }
    let out = std::process::Command::new("/usr/sbin/spctl")
        .args(["--assess", "--type", "execute"])
        .arg(app)
        .output()
        .map_err(|e| format!("spctl did not run: {e}"))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(format!("Gatekeeper refuses it: {}", said(&out)))
    }
}

/// The app at `app` replaced (an update): its helper copied over the
/// host's copy, if it meets the running copy's designated requirement (so
/// it is the same app's, and keeps the permissions macOS granted by that
/// requirement) and Gatekeeper accepts the app. The copy's quarantine flag
/// then goes, since Gatekeeper has had its say: launchd would otherwise
/// wait on Gatekeeper's dialog for it, which no one sees for a host
/// launchd starts (an app Homebrew upgraded comes marked as never opened).
/// Whether the copy was replaced: the caller restarts the host on it. Not:
/// the host goes on as it is, and the log says why.
fn follow(app: &Path) -> bool {
    let helper = helper_in(app);
    let copy = installed();
    let checked = requirement_of(&copy)
        .and_then(|r| satisfies(&helper, &r))
        .and_then(|()| gatekeeper_accepts(app, &helper));
    if let Err(why) = checked {
        tracing::warn!(
            app = %app.display(),
            why,
            "the app was replaced, but its host is not one this host may run: \
                the host goes on as it is"
        );
        return false;
    }
    if let Err(e) = copy_bundle(&helper, &copy) {
        tracing::warn!(error = e, "the app was replaced; its host was not copied");
        return false;
    }
    let cleared = std::process::Command::new("/usr/bin/xattr")
        .args(["-dr", "com.apple.quarantine"])
        .arg(&copy)
        .status();
    if !cleared.is_ok_and(|s| s.success()) {
        tracing::warn!("the host's new copy keeps its quarantine flag");
    }
    tracing::info!(app = %app.display(), "the app was replaced: the host restarts on its helper");
    true
}

/// Run the host's copy again in this process (exec), with `args`: at
/// start, before anything listens, launchd sees the same process.
fn restart_in_place(args: &[String]) {
    use std::os::unix::process::CommandExt;
    let program = installed().join("Contents/MacOS/helio-host");
    let e = std::process::Command::new(&program)
        .arg("host")
        .args(args.iter().skip_while(|a| *a != "--app"))
        .exec();
    tracing::warn!(error = %e, "the host's new copy did not start; this one goes on");
}

/// Hosting off from inside the host, its app gone: the agent's file and
/// the copy removed, then launchd asked to unload the agent, which stops
/// this host as `helio-host disable` does (SIGTERM: the session ends properly).
/// A host launchd does not run is asked to stop directly.
fn retire(app: &Path) {
    tracing::warn!(
        app = %app.display(),
        "the app this host came with is gone: hosting off"
    );
    if let Ok(path) = plist() {
        let _ = std::fs::remove_file(path);
    }
    if let Err(e) = remove_bundle(&installed()) {
        tracing::warn!(error = e, "the host's copy stays");
    }
    let unloaded = launchctl(&["bootout", &format!("{}/{LABEL}", domain())])
        .is_ok_and(|out| out.status.success());
    if !unloaded {
        // SAFETY: a signal to this process, which handles SIGTERM by
        // stopping the host.
        unsafe { libc::kill(libc::getpid(), libc::SIGTERM) };
    }
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
                login, or now with helio host enable"
                .into(),
        ),
        Ok(_) => State::Off,
        Err(e) => State::Failed(e),
    }
}

#[cfg(test)]
mod tests {
    use super::designated;

    #[test]
    fn the_requirement_a_new_host_must_meet_is_read_from_codesign() {
        let said = "Executable=/A/Helio Host.app/Contents/MacOS/helio-host\n\
            designated => identifier \"dev.mihaicristian.helio.host\" and anchor apple generic and \
            certificate leaf[subject.OU] = \"ABCDE12345\"\n";
        assert_eq!(
            designated(said).as_deref(),
            Some(
                "identifier \"dev.mihaicristian.helio.host\" and anchor apple generic and \
                 certificate leaf[subject.OU] = \"ABCDE12345\""
            )
        );
        // Unsigned, or codesign's complaint instead.
        assert_eq!(designated("Executable=/x\n"), None);
        assert_eq!(designated("/x: code object is not signed at all\n"), None);
    }
}
