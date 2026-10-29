//! The host's icon in the menu bar, for a host opened by hand on a Mac.
//!
//! Helio Host.app is the host alone: a background app (LSUIElement) with no
//! window or icon of its own. Its icon and window are Helio's, the app that
//! holds it (`Helio.app/Contents/Helpers/Helio Host.app`), which starts at
//! login by a login item of its own (General > Open at login). Opened by
//! hand (from the Finder, inside Helio.app, or by Spotlight), the helper
//! starts Helio too, in the background, so the icon is there whenever someone
//! started the host; without it, the host ran with nothing on screen to say
//! so. The host started at login by its LaunchAgent (`helio-host host`) leaves the
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

/// Helio.app: the app the helper this runs in is inside of
/// (`Helio.app/Contents/Helpers/Helio Host.app`).
fn helio_app() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    holder(&exe).filter(|app| app.is_dir())
}

/// The app that holds the helper whose program is `exe`.
fn holder(exe: &Path) -> Option<PathBuf> {
    // Helio Host.app/Contents/MacOS/helio-host
    let helper = crate::hosting::bundle_of(exe)?;
    crate::hosting::app_holding(helper).map(Path::to_path_buf)
}

/// Start Helio in the background: its icon, without its window.
pub fn show() {
    let Some(app) = helio_app() else {
        tracing::info!("no Helio.app holding this host: no icon in the menu bar");
        return;
    };
    // `-n`, a new copy every time: a copy that runs already would otherwise
    // get a reopen event and open its window. The new one finds it running
    // and leaves without a word (`--background`). `-g`: not in front.
    let mut open = std::process::Command::new("/usr/bin/open");
    open.args(["-n", "-g"]);
    // An app Launch Services starts has none of this process's environment:
    // a host with a data folder of its own (a check's) gets an icon for it.
    if let Some(dir) = std::env::var_os("HELIO_HOST_DATA_DIR") {
        let mut env = std::ffi::OsString::from("HELIO_HOST_DATA_DIR=");
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
        Ok(s) => tracing::warn!(status = %s, "Helio did not start: no icon in the menu bar"),
        Err(e) => tracing::warn!(error = %e, "Helio did not start: no icon in the menu bar"),
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
                "/Applications/Helio.app/Contents/Helpers/Helio Host.app/Contents/MacOS/helio-host"
            ))
            .as_deref(),
            Some(Path::new("/Applications/Helio.app"))
        );
        // A helper on its own, and a build: no app to start.
        assert_eq!(
            holder(Path::new("/tmp/Helio Host.app/Contents/MacOS/helio-host")),
            None
        );
        assert_eq!(holder(Path::new("/src/target/debug/helio-host")), None);
    }
}
