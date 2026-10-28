//! Starting the host from its window, on a Mac: a copy installed by a
//! package manager has had no script run for it, so the window turns
//! hosting on itself, with `pong enable` (the host's LaunchAgent, which the
//! host writes and loads; `pong/src/hosting/mac.rs`).
//!
//! Only a Mac for now: on Windows `pong enable` needs an administrator, on
//! Linux the unit is installed with the host; the window says how.

/// The host's executable: in `Pong.app`, beside the bundle this app is in.
#[cfg(target_os = "macos")]
fn host_binary() -> Option<std::path::PathBuf> {
    let exe = std::env::current_exe().ok()?;
    // Pong Control.app/Contents/MacOS/Pong Control
    let bundle = exe.parent()?.parent()?.parent()?;
    if bundle.extension()? != "app" {
        return None;
    }
    let host = bundle.parent()?.join("Pong.app/Contents/MacOS/pong");
    host.is_file().then_some(host)
}

/// Whether the window can start the host itself.
pub fn can_start() -> bool {
    #[cfg(target_os = "macos")]
    return host_binary().is_some();
    #[cfg(not(target_os = "macos"))]
    false
}

/// Turn hosting on: the host runs now, and whenever the user is logged in.
pub fn start() -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let host = host_binary().ok_or("Pong.app is not beside this app.")?;
        tracing::info!(host = %host.display(), "turning hosting on");
        let out = std::process::Command::new(&host)
            .arg("enable")
            .output()
            .map_err(|e| format!("{}: {e}", host.display()))?;
        if out.status.success() {
            Ok(())
        } else {
            Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
        }
    }
    #[cfg(not(target_os = "macos"))]
    Err("The host is not started from its window on this system.".into())
}
