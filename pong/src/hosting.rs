//! Hosting on or off: `pong enable`, `pong disable` and `pong status`.
//!
//! Hosting is one switch, off by default. On, the host runs whenever the
//! user is signed in (on Windows, from boot), kept running the way each
//! system keeps a program running:
//!
//! - Windows: `PongService` (`service`), run from the engine's own copy in
//!   `C:\Program Files\Pong`, a folder only administrators can change
//!   (`hosting/windows.rs` says why). Turning hosting on or off needs an
//!   administrator: without one, the command changes nothing and exits with
//!   [`NEEDS_ELEVATION`], and the app runs it again through UAC.
//! - macOS: a LaunchAgent that runs `pong host` from a copy of the helper
//!   app this `pong` is in (`hosting/mac.rs`).
//! - Linux: the systemd user unit `pong.service`
//!   (`tools/linux/pong.service`), `systemctl --user enable|disable --now`
//!   (`hosting/linux.rs`).
//!
//! Everything a host exposes lives in `pong host`: the tunnel port, the
//! pairing listener, the web UI and API, presence on the DHT, STUN, the
//! router's port mapping and the mDNS advertisement. Off, no `pong host`
//! runs, so none of them does; nothing in the client starts any of them.
//!
//! `pong status` says off, starting, on or failed (with why), and for on,
//! whether the host's web API answers on this computer. `--json` is for the
//! app. What launchd and systemd print is read here, not in their modules,
//! so that the reading is tested on every system.

use std::path::Path;

/// `pong enable` or `pong disable` run on Windows without an administrator:
/// Windows's own `ERROR_ELEVATION_REQUIRED`, so the app knows to run the
/// same command again elevated (`ShellExecuteEx` with `runas`) rather than
/// show an error. Nothing was changed.
pub const NEEDS_ELEVATION: i32 = 740;

/// What runs, as far as hosting goes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum State {
    /// Hosting is off: nothing starts the host.
    Off,
    /// The system is starting the host.
    Starting,
    /// The host runs.
    On,
    /// Hosting is on, but the host does not run; why.
    Failed(String),
}

/// What `pong status` reports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Status {
    pub state: State,
    /// For a host that runs: whether its web API answers on this computer.
    pub api: Option<bool>,
}

impl Status {
    /// For the app: `{"state": "off" | "starting" | "on" | "failed"}`, with
    /// `"api"` for on and `"reason"` for failed.
    pub fn json(&self) -> serde_json::Value {
        use serde_json::json;
        match &self.state {
            State::Off => json!({ "state": "off" }),
            State::Starting => json!({ "state": "starting" }),
            State::On => json!({ "state": "on", "api": self.api.unwrap_or(false) }),
            State::Failed(reason) => json!({ "state": "failed", "reason": reason }),
        }
    }

    /// For a person.
    pub fn text(&self, web_port: u16) -> String {
        match (&self.state, self.api) {
            (State::Off, _) => "off".into(),
            (State::Starting, _) => "starting".into(),
            (State::On, Some(true)) => {
                format!("on; the web UI answers at https://localhost:{web_port}")
            }
            (State::On, _) => {
                format!("on, but the web UI does not answer at https://localhost:{web_port}")
            }
            (State::Failed(reason), _) => format!("failed: {reason}"),
        }
    }
}

/// Why `pong enable` or `pong disable` did not do what it was asked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// Windows: run it again as an administrator. Nothing was changed.
    /// (Elsewhere hosting is the user's own business, and never needs one.)
    #[cfg_attr(not(windows), allow(dead_code))]
    NeedsElevation,
    /// What went wrong, for a person.
    Failed(String),
}

impl From<String> for Error {
    fn from(e: String) -> Self {
        Error::Failed(e)
    }
}

impl From<&str> for Error {
    fn from(e: &str) -> Self {
        Error::Failed(e.into())
    }
}

/// The process's exit code for what `pong enable` or `pong disable` did:
/// 0 done, [`NEEDS_ELEVATION`], or 1 for anything else.
pub fn exit_code(result: &Result<String, Error>) -> i32 {
    match result {
        Ok(_) => 0,
        Err(Error::NeedsElevation) => NEEDS_ELEVATION,
        Err(Error::Failed(_)) => 1,
    }
}

#[cfg(windows)]
mod signature;
#[cfg(windows)]
#[path = "hosting/windows.rs"]
mod system;
#[cfg(target_os = "macos")]
#[path = "hosting/mac.rs"]
mod system;
#[cfg(target_os = "linux")]
#[path = "hosting/linux.rs"]
mod system;

#[cfg(windows)]
pub use system::trusted_engine;
#[cfg(target_os = "macos")]
pub use system::watch_app;

/// `pong enable` (`on`) or `pong disable`: do it, say what happened, and
/// exit with [`exit_code`].
pub fn switch(on: bool) -> ! {
    let result = if on {
        system::enable()
    } else {
        system::disable()
    };
    match &result {
        Ok(done) => println!("{done}"),
        Err(Error::NeedsElevation) => eprintln!(
            "pong {} needs an administrator: run it again from an elevated terminal \
                (exit code {NEEDS_ELEVATION}; nothing was changed)",
            if on { "enable" } else { "disable" }
        ),
        Err(Error::Failed(e)) => eprintln!("{e}"),
    }
    std::process::exit(exit_code(&result))
}

/// `pong status [--json]`: what runs, and whether the host answers.
pub fn print_status(dir: &Path, json: bool) -> std::process::ExitCode {
    let state = system::state();
    let web_port = crate::web::probe::web_port(dir);
    let api = (state == State::On).then(|| crate::web::probe::answers(dir, web_port));
    let status = Status { state, api };
    if json {
        println!("{}", status.json());
    } else {
        println!("{}", status.text(web_port));
    }
    std::process::ExitCode::SUCCESS
}

/// The app bundle `exe` is the main program of (`X.app/Contents/MacOS/…`),
/// where a Mac host must run from: macOS grants Screen Recording and
/// Accessibility to an app, by its bundle and signature.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub fn bundle_of(exe: &Path) -> Option<&Path> {
    let macos = exe.parent()?;
    let contents = macos.parent()?;
    let bundle = contents.parent()?;
    (macos.file_name()? == "MacOS"
        && contents.file_name()? == "Contents"
        && bundle.extension()? == "app")
        .then_some(bundle)
}

/// The app that holds the helper app `helper`
/// (`X.app/Contents/Helpers/Pong Host.app`): the one a Mac host comes from.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub fn app_holding(helper: &Path) -> Option<&Path> {
    let helpers = helper.parent()?;
    let app = helpers.parent()?.parent()?;
    (helper.extension()? == "app"
        && helpers.file_name()? == "Helpers"
        && helpers.parent()?.file_name()? == "Contents"
        && app.extension()? == "app")
        .then_some(app)
}

/// How long launchd lets the host take to stop before it kills it: the
/// host ends a session properly and puts the displays back first, and
/// launchd's own default is 5 s (`exit timeout = 5` in `launchctl print`,
/// macOS 26.5). The same 15 s PongService gives the host on Windows.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub const STOP_GRACE_S: u64 = 15;

/// The LaunchAgent `label` that runs `exe host` (then `args`) at login and
/// again whenever it exits, with `env` in its environment.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub fn launch_agent(label: &str, exe: &Path, args: &[String], env: &[(&str, String)]) -> String {
    fn xml(text: &str) -> String {
        text.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
    }
    let environment = if env.is_empty() {
        String::new()
    } else {
        let mut d = String::from("\t<key>EnvironmentVariables</key>\n\t<dict>\n");
        for (k, v) in env {
            d.push_str(&format!(
                "\t\t<key>{}</key>\n\t\t<string>{}</string>\n",
                xml(k),
                xml(v)
            ));
        }
        d.push_str("\t</dict>\n");
        d
    };
    let args: String = args
        .iter()
        .map(|a| format!("\t\t<string>{}</string>\n", xml(a)))
        .collect();
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \
         \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
         <plist version=\"1.0\">\n\
         <dict>\n\
         \t<key>Label</key>\n\
         \t<string>{label}</string>\n\
         \t<key>ProgramArguments</key>\n\
         \t<array>\n\
         \t\t<string>{exe}</string>\n\
         \t\t<string>host</string>\n\
         {args}\
         \t</array>\n\
         {environment}\
         \t<key>RunAtLoad</key>\n\
         \t<true/>\n\
         \t<key>KeepAlive</key>\n\
         \t<true/>\n\
         \t<key>ExitTimeOut</key>\n\
         \t<integer>{STOP_GRACE_S}</integer>\n\
         \t<key>ProcessType</key>\n\
         \t<string>Interactive</string>\n\
         </dict>\n\
         </plist>\n",
        label = xml(label),
        exe = xml(&exe.to_string_lossy()),
    )
}

/// A top-level `name = value` of `launchctl print` (one tab in: the blocks
/// nested in it have `state` lines of their own).
fn launchd_field<'a>(print: &'a str, name: &str) -> Option<&'a str> {
    print.lines().find_map(|line| {
        let line = line.strip_prefix('\t')?;
        if line.starts_with('\t') {
            return None;
        }
        line.strip_prefix(name)?.strip_prefix(" = ").map(str::trim)
    })
}

/// The process of a loaded agent that runs, from `launchctl print`.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub fn launchd_pid(print: &str) -> Option<i32> {
    launchd_field(print, "pid")?.parse().ok()
}

/// The host's state from `launchctl print gui/UID/LABEL` of a loaded agent.
///
/// launchd says `running`; `xpcproxy` while it spawns the process (just
/// after `bootstrap`); `SIGTERMed` while the process stops after a
/// `bootout`; or, for a KeepAlive agent between two starts, `spawn
/// scheduled` (it starts it again after its 10 s throttle), with the last
/// exit: `last exit code = 3`, `(never exited)`, or a signal in `last
/// terminating signal`. Read from macOS 26.5.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub fn launchd_state(print: &str) -> State {
    let exited = launchd_field(print, "last exit code")
        .and_then(|c| c.split(':').next()?.trim().parse::<i32>().ok())
        .filter(|c| *c != 0)
        .map(|c| format!("the host exited with code {c}"))
        .or_else(|| {
            launchd_field(print, "last terminating signal")
                .map(|s| format!("the host was ended by a signal ({s})"))
        });
    match (launchd_field(print, "state"), exited) {
        (Some("running"), _) => State::On,
        (Some(s), _) if s.starts_with("SIG") => State::Failed("the host is stopping".into()),
        (Some("xpcproxy"), _) => State::Starting,
        (Some("spawn scheduled" | "spawning"), None) => State::Starting,
        (Some("spawn scheduled" | "spawning"), Some(why)) => {
            State::Failed(format!("{why}; launchd starts it again"))
        }
        (Some(_), Some(why)) => State::Failed(why),
        (Some(other), None) => State::Failed(format!("launchd says the host is {other}")),
        (None, _) => State::Failed("launchd did not say what the host is doing".into()),
    }
}

/// The host's state from `systemctl --user show pong.service` with the
/// properties `LoadState`, `UnitFileState`, `ActiveState`, `SubState`,
/// `Result` and `ExecMainStatus`.
///
/// Between two starts after a failure (`Restart=on-failure`) the unit is
/// `activating` in `auto-restart`: that is reported as failed, with why.
/// Enabled but inactive is a unit that did not start with the session (no
/// desktop yet, or stopped by hand): failed too, since hosting is on.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub fn systemd_state(show: &str) -> State {
    let get = |name: &str| {
        show.lines()
            .find_map(|l| l.strip_prefix(name)?.strip_prefix('='))
            .unwrap_or("")
            .trim()
    };
    let why = || {
        let status = get("ExecMainStatus");
        match get("Result") {
            "exit-code" => format!("the host exited with code {status}"),
            "signal" | "core-dump" => format!("the host was ended by signal {status}"),
            "start-limit-hit" => "the host failed too often in a row; systemd gave up \
                restarting it (journalctl --user -u pong)"
                .into(),
            "" | "success" => "the host is not running".into(),
            other => format!("systemd says {other}"),
        }
    };
    let enabled = get("UnitFileState") == "enabled";
    if get("LoadState") != "loaded" {
        return State::Off;
    }
    match get("ActiveState") {
        "active" | "reloading" => State::On,
        "activating" if get("SubState") == "auto-restart" => {
            State::Failed(format!("{}; systemd starts it again", why()))
        }
        "activating" => State::Starting,
        "failed" => State::Failed(why()),
        "deactivating" if enabled => State::Failed("the host is stopping".into()),
        "inactive" if enabled => State::Failed(format!(
            "{}; it starts with your next desktop session, or now with \
                systemctl --user start pong",
            why()
        )),
        _ => State::Off,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn needing_an_administrator_has_an_exit_code_of_its_own() {
        assert_eq!(exit_code(&Ok("on".into())), 0);
        assert_eq!(exit_code(&Err(Error::NeedsElevation)), 740);
        assert_eq!(exit_code(&Err(Error::Failed("no".into()))), 1);
        // Windows's ERROR_ELEVATION_REQUIRED, which the app looks for.
        #[cfg(windows)]
        assert_eq!(
            NEEDS_ELEVATION as u32,
            windows::Win32::Foundation::ERROR_ELEVATION_REQUIRED.0
        );
    }

    #[test]
    fn the_app_reads_the_state_and_only_what_goes_with_it() {
        use serde_json::json;
        // Compared as values: the order of the keys depends on serde_json's
        // features, which other crates in the workspace choose.
        let on = Status {
            state: State::On,
            api: Some(true),
        };
        assert_eq!(on.json(), json!({ "state": "on", "api": true }));
        let off = Status {
            state: State::Off,
            api: None,
        };
        assert_eq!(off.json(), json!({ "state": "off" }));
        let failed = Status {
            state: State::Failed("the host exited with code 3".into()),
            api: None,
        };
        assert_eq!(
            failed.json(),
            json!({ "state": "failed", "reason": "the host exited with code 3" })
        );
        assert!(Status {
            state: State::On,
            api: Some(false)
        }
        .text(47802)
        .contains("does not answer at https://localhost:47802"));
    }

    #[test]
    fn a_mac_host_runs_only_as_an_apps_main_program() {
        // The helper inside the app (tools/build-ping-app).
        let helper =
            Path::new("/Applications/Ping.app/Contents/Helpers/Pong Host.app/Contents/MacOS/pong");
        assert_eq!(
            bundle_of(helper),
            Some(Path::new(
                "/Applications/Ping.app/Contents/Helpers/Pong Host.app"
            ))
        );
        // A build, and things that look like a bundle without being one.
        assert_eq!(bundle_of(Path::new("/repo/target/release/pong")), None);
        assert_eq!(bundle_of(Path::new("/x/Pong/Contents/MacOS/pong")), None);
        assert_eq!(
            bundle_of(Path::new("/x/Pong.app/Contents/Resources/pong")),
            None
        );
        assert_eq!(bundle_of(Path::new("pong")), None);
    }

    #[test]
    fn a_mac_host_knows_the_app_it_came_with() {
        let helper = Path::new("/Applications/Ping.app/Contents/Helpers/Pong Host.app");
        assert_eq!(
            app_holding(helper),
            Some(Path::new("/Applications/Ping.app"))
        );
        // Its copy, outside any app, and things that look like a helper.
        assert_eq!(
            app_holding(Path::new(
                "/Users/o/Library/Application Support/Pong/Pong Host.app"
            )),
            None
        );
        assert_eq!(
            app_holding(Path::new(
                "/Applications/Ping.app/Contents/Library/Pong Host.app"
            )),
            None
        );
        assert_eq!(
            app_holding(Path::new("/x/Ping/Contents/Helpers/Pong Host.app")),
            None
        );
    }

    #[test]
    fn the_launch_agent_keeps_the_host_running_with_the_environment_handed_on() {
        let text = launch_agent(
            "dev.pingpong.Pong",
            Path::new("/Applications/A & B.app/Contents/MacOS/pong"),
            &["--app".into(), "/Applications/A & B.app".into()],
            &[("PONG_DATA_DIR", "/tmp/<check>".into())],
        );
        assert!(text.contains("<string>dev.pingpong.Pong</string>"));
        assert!(text.contains(
            "\t\t<string>/Applications/A &amp; B.app/Contents/MacOS/pong</string>\n\
             \t\t<string>host</string>\n\
             \t\t<string>--app</string>\n\
             \t\t<string>/Applications/A &amp; B.app</string>\n\
             \t</array>"
        ));
        assert!(text.contains("<key>PONG_DATA_DIR</key>\n\t\t<string>/tmp/&lt;check&gt;</string>"));
        assert!(text.contains("<key>KeepAlive</key>\n\t<true/>"));
        assert!(text.contains("<key>RunAtLoad</key>\n\t<true/>"));
        assert!(text.contains("<key>ExitTimeOut</key>\n\t<integer>15</integer>"));
        let bare = launch_agent(
            "dev.pingpong.Pong",
            Path::new("/A.app/Contents/MacOS/pong"),
            &[],
            &[],
        );
        assert!(!bare.contains("EnvironmentVariables"));
    }

    /// What `launchctl print` said on macOS 26.5, trimmed.
    const RUNNING: &str = "gui/501/dev.pingpong.Pong = {\n\
        \tactive count = 1\n\
        \tpath = /Users/someone/Library/LaunchAgents/dev.pingpong.Pong.plist\n\
        \ttype = LaunchAgent\n\
        \tstate = running\n\
        \n\
        \tprogram = /Applications/Ping.app/Contents/Helpers/Pong Host.app/Contents/MacOS/pong\n\
        \truns = 1\n\
        \tpid = 64735\n\
        \tlast exit code = (never exited)\n\
        \n\
        \tendpoints = {\n\
        \t\tstate = active\n\
        \t}\n\
        }\n";
    const RESPAWNING: &str = "gui/501/dev.pingpong.Pong = {\n\
        \tactive count = 0\n\
        \tstate = spawn scheduled\n\
        \truns = 1\n\
        \tlast exit code = 3\n\
        \tsockets = {\n\
        \t\tstate = active\n\
        \t}\n\
        }\n";

    #[test]
    fn launchd_running_is_on_whatever_nested_blocks_say() {
        assert_eq!(launchd_state(RUNNING), State::On);
        assert_eq!(launchd_pid(RUNNING), Some(64735));
        assert_eq!(launchd_pid(RESPAWNING), None);
    }

    #[test]
    fn launchd_spawning_is_starting_and_a_stopping_host_says_so() {
        // Just after `bootstrap`, and just after `bootout` (macOS 26.5).
        let spawning = RUNNING.replace("state = running", "state = xpcproxy");
        assert_eq!(launchd_state(&spawning), State::Starting);
        let stopping = RUNNING.replace("state = running", "state = SIGTERMed");
        assert_eq!(
            launchd_state(&stopping),
            State::Failed("the host is stopping".into())
        );
    }

    #[test]
    fn launchd_starting_again_after_a_failure_is_failed_with_the_exit_code() {
        assert_eq!(
            launchd_state(RESPAWNING),
            State::Failed("the host exited with code 3; launchd starts it again".into())
        );
        let first = RESPAWNING.replace("last exit code = 3", "last exit code = (never exited)");
        assert_eq!(launchd_state(&first), State::Starting);
        // Newer launchd names the code too.
        let named = RESPAWNING.replace("= 3", "= 78: EX_CONFIG");
        assert!(matches!(launchd_state(&named), State::Failed(w) if w.contains("code 78")));
        let signal = "\tstate = not running\n\tlast terminating signal = Killed: 9\n";
        assert_eq!(
            launchd_state(signal),
            State::Failed("the host was ended by a signal (Killed: 9)".into())
        );
        assert!(matches!(launchd_state(""), State::Failed(_)));
    }

    fn show(active: &str, sub: &str, result: &str, status: u8, file: &str) -> String {
        format!(
            "LoadState=loaded\nUnitFileState={file}\nActiveState={active}\nSubState={sub}\n\
             Result={result}\nExecMainStatus={status}\n"
        )
    }

    #[test]
    fn systemd_running_is_on_and_activating_is_starting() {
        let on = show("active", "running", "success", 0, "enabled");
        assert_eq!(systemd_state(&on), State::On);
        // Started by hand, not enabled: it runs all the same.
        let by_hand = show("active", "running", "success", 0, "disabled");
        assert_eq!(systemd_state(&by_hand), State::On);
        let starting = show("activating", "start", "success", 0, "enabled");
        assert_eq!(systemd_state(&starting), State::Starting);
    }

    #[test]
    fn systemd_failures_say_why() {
        let restarting = show("activating", "auto-restart", "exit-code", 1, "enabled");
        assert_eq!(
            systemd_state(&restarting),
            State::Failed("the host exited with code 1; systemd starts it again".into())
        );
        let gave_up = show("failed", "failed", "start-limit-hit", 1, "enabled");
        assert!(matches!(systemd_state(&gave_up), State::Failed(w) if w.contains("gave up")));
        let killed = show("failed", "failed", "signal", 9, "enabled");
        assert_eq!(
            systemd_state(&killed),
            State::Failed("the host was ended by signal 9".into())
        );
        let not_started = show("inactive", "dead", "success", 0, "enabled");
        assert!(matches!(
            systemd_state(&not_started),
            State::Failed(w) if w.starts_with("the host is not running; it starts with")
        ));
    }

    #[test]
    fn systemd_off_is_a_unit_not_enabled_and_not_running_or_not_installed() {
        let off = show("inactive", "dead", "success", 0, "disabled");
        assert_eq!(systemd_state(&off), State::Off);
        let stopping = show("deactivating", "stop-sigterm", "success", 0, "disabled");
        assert_eq!(systemd_state(&stopping), State::Off);
        let missing = "LoadState=not-found\nUnitFileState=\nActiveState=inactive\n\
            SubState=dead\nResult=success\nExecMainStatus=0\n";
        assert_eq!(systemd_state(missing), State::Off);
    }
}
