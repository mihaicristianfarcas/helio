//! Pong's icon in the menu bar, for a host opened by hand on a Mac.
//!
//! Pong.app is the host alone: a background app (LSUIElement) with no
//! window or icon of its own. Its icon and window are Pong Control.app,
//! installed beside it, which starts at login by a login item of its own
//! (General > "Show Pong's icon at login" in its window). Opened from the
//! Finder, Launchpad or Spotlight, Pong.app starts Pong Control too, in the
//! background, so the icon is there whenever someone started Pong; without
//! it, the host ran with nothing on screen to say so. The host started at
//! login by its LaunchAgent (`pong host`) leaves the icon to that login
//! item, which the user may have turned off.

use std::path::PathBuf;

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

/// Pong Control.app beside the Pong.app this runs in.
fn control_app() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    // Pong.app/Contents/MacOS/pong
    let bundle = exe.parent()?.parent()?.parent()?;
    if bundle.extension()? != "app" {
        return None;
    }
    let control = bundle.parent()?.join("Pong Control.app");
    control.is_dir().then_some(control)
}

/// Start Pong Control in the background: its icon, without its window.
pub fn show() {
    let Some(app) = control_app() else {
        tracing::info!("no Pong Control.app beside Pong.app: no icon in the menu bar");
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
        Ok(s) if s.success() => tracing::info!("Pong's icon in the menu bar"),
        Ok(s) => tracing::warn!(status = %s, "Pong Control did not start: no icon in the menu bar"),
        Err(e) => tracing::warn!(error = %e, "Pong Control did not start: no icon in the menu bar"),
    }
}

#[cfg(test)]
mod tests {
    use super::opened_by_hand;

    #[test]
    fn the_finder_names_no_command_and_the_login_agent_does() {
        assert!(opened_by_hand(&[]));
        assert!(opened_by_hand(&["-psn_0_1234567".into()]));
        assert!(!opened_by_hand(&["host".into()]));
        assert!(!opened_by_hand(&["identity".into()]));
    }
}
