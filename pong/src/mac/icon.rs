//! The host's icon in the menu bar, for a host opened by hand on a Mac.
//!
//! Pong Host.app is the host alone: a background app (LSUIElement) with no
//! window or icon of its own. Its icon and window are Ping's, the app that
//! holds it (`Ping.app/Contents/Helpers/Pong Host.app`), which starts at
//! login by a login item of its own (General > Open at login). Opened by
//! hand (from the Finder, inside Ping.app, or by Spotlight), the helper
//! starts Ping too, in the background, so the icon is there whenever someone
//! started the host; without it, the host ran with nothing on screen to say
//! so. The host started at login by its LaunchAgent (`pong host`) leaves the
//! icon to that login item, which the user may have turned off.

use std::path::{Path, PathBuf};

/// Started by Launch Services (the Finder, `open`), not by a command line
/// or the LaunchAgent, which both name a command: no arguments, or only the
/// process serial number older macOS versions pass.
pub fn opened_by_hand(args: &[String]) -> bool {
    match args {
        [] => true,
        [only] => only.starts_with("-psn_"),
        _ => false,
    }
}

/// Ping.app: the app the helper this runs in is inside of
/// (`Ping.app/Contents/Helpers/Pong Host.app`).
fn ping_app() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    holder(&exe).filter(|app| app.is_dir())
}

/// The app that holds the helper whose program is `exe`.
fn holder(exe: &Path) -> Option<PathBuf> {
    // Pong Host.app/Contents/MacOS/pong
    let helper = crate::hosting::bundle_of(exe)?;
    crate::hosting::app_holding(helper).map(Path::to_path_buf)
}

/// Start Ping in the background: its icon, without its window.
pub fn show() {
    let Some(app) = ping_app() else {
        tracing::info!("no Ping.app holding this host: no icon in the menu bar");
        return;
    };
    // `-n`, a new copy every time: a copy that runs already would otherwise
    // get a reopen event and open its window. The new one finds it running
    // and leaves without a word (`--background`). `-g`: not in front.
    let mut open = std::process::Command::new("/usr/bin/open");
    open.args(["-n", "-g"]);
    // An app Launch Services starts has none of this process's environment:
    // a host with a data folder of its own (a check's) gets an icon for it.
    if let Some(dir) = std::env::var_os("PONG_DATA_DIR") {
        let mut env = std::ffi::OsString::from("PONG_DATA_DIR=");
        env.push(dir);
        open.arg("--env").arg(env);
    }
    let started = open
        .arg("-a")
        .arg(&app)
        .args(["--args", "--background"])
        .status();
    match started {
        Ok(s) if s.success() => tracing::info!("the host's icon in the menu bar"),
        Ok(s) => tracing::warn!(status = %s, "Ping did not start: no icon in the menu bar"),
        Err(e) => tracing::warn!(error = %e, "Ping did not start: no icon in the menu bar"),
    }
}

#[cfg(test)]
mod tests {
    use super::{holder, opened_by_hand};
    use std::path::Path;

    #[test]
    fn the_finder_names_no_command_and_the_login_agent_does() {
        assert!(opened_by_hand(&[]));
        assert!(opened_by_hand(&["-psn_0_1234567".into()]));
        assert!(!opened_by_hand(&["host".into()]));
        assert!(!opened_by_hand(&["identity".into()]));
    }

    #[test]
    fn the_icon_is_the_app_that_holds_the_host() {
        assert_eq!(
            holder(Path::new(
                "/Applications/Ping.app/Contents/Helpers/Pong Host.app/Contents/MacOS/pong"
            ))
            .as_deref(),
            Some(Path::new("/Applications/Ping.app"))
        );
        // A helper on its own, and a build: no app to start.
        assert_eq!(
            holder(Path::new("/tmp/Pong Host.app/Contents/MacOS/pong")),
            None
        );
        assert_eq!(holder(Path::new("/src/target/debug/pong")), None);
    }
}
