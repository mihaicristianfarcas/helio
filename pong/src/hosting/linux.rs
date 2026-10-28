//! Hosting on Linux: the systemd user unit `pong.service`
//! (`tools/linux/pong.service`, which Ping's `install.sh` puts in
//! `~/.config/systemd/user`, not enabled), started in the user's desktop
//! session, where the host can reach the display.
//!
//! On and off are `systemctl --user enable --now` and `disable --now`: the
//! user's own service manager, never root. Both wait for systemd to have
//! started or stopped the host before they return.

use std::process::Output;

use super::{Error, State};

const UNIT: &str = "pong.service";

fn systemctl(args: &[&str]) -> Result<Output, String> {
    std::process::Command::new("systemctl")
        .arg("--user")
        .args(args)
        .output()
        .map_err(|e| format!("systemctl did not run: {e}"))
}

fn said(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).trim().to_string()
}

/// Whether systemd knows the unit at all (its file is installed).
fn installed() -> Result<bool, String> {
    let out = systemctl(&["show", UNIT, "--property=LoadState"])?;
    Ok(String::from_utf8_lossy(&out.stdout).trim() == "LoadState=loaded")
}

pub fn enable() -> Result<String, Error> {
    if !installed()? {
        return Err(Error::Failed(format!(
            "The host's unit is not installed ({UNIT} in ~/.config/systemd/user): \
                install Ping with the install.sh in its download, which installs it."
        )));
    }
    let out = systemctl(&["enable", "--now", UNIT])?;
    if !out.status.success() {
        return Err(Error::Failed(format!(
            "systemd did not start the host: {}",
            said(&out)
        )));
    }
    Ok(
        "Hosting is on: the host runs in your desktop session, now and at each login \
        (systemctl --user status pong)."
            .into(),
    )
}

pub fn disable() -> Result<String, Error> {
    if !installed()? {
        return Ok("Hosting is off (the host's unit is not installed).".into());
    }
    let out = systemctl(&["disable", "--now", UNIT])?;
    if !out.status.success() {
        return Err(Error::Failed(format!(
            "systemd did not stop the host: {}",
            said(&out)
        )));
    }
    Ok("Hosting is off: the host is stopped, and no longer starts at login.".into())
}

pub fn state() -> State {
    let properties =
        "--property=LoadState,UnitFileState,ActiveState,SubState,Result,ExecMainStatus";
    match systemctl(&["show", UNIT, properties]) {
        Ok(out) if out.status.success() => {
            super::systemd_state(&String::from_utf8_lossy(&out.stdout))
        }
        Ok(out) => State::Failed(format!("systemd did not say: {}", said(&out))),
        Err(e) => State::Failed(e),
    }
}
