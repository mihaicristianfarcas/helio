//! Installing an update on a Mac: the release's app bundle in place of the
//! running one, once it is known to be signed, helper and all, by the same
//! team's Developer ID; or, for a copy Homebrew installed, `brew upgrade
//! --cask`. Either way hosting stays as it was: the host, which runs from a
//! copy of the app's helper outside the app, sees the app replaced and
//! moves onto the new helper by itself (`helio-host/src/hosting/mac.rs`).

use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use super::sh_quote;
use crate::{APP, CASK};

/// How this copy is updated.
pub(crate) enum Method {
    /// Swap the bundle in `folder` (where the running one is).
    Bundles { own: PathBuf, folder: PathBuf },
    /// Homebrew installed it: `brew upgrade --cask`, run by a script that
    /// waits for the app to quit (the cask's uninstall step quits it) and
    /// opens it again after.
    Homebrew { own: PathBuf, brew: PathBuf },
}

/// The bundle a release's archive holds.
fn bundle_name() -> String {
    format!("{APP}.app")
}

/// The app bundle this program runs from.
fn own_bundle() -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    // X.app/Contents/MacOS/x
    exe.ancestors()
        .nth(3)
        .filter(|b| b.extension().is_some_and(|e| e == "app"))
        .map(Path::to_path_buf)
        .ok_or_else(|| "It does not run from an app bundle: download the update instead.".into())
}

/// Homebrew's `brew`, when it installed `cask`.
fn homebrew(cask: &str) -> Option<PathBuf> {
    let prefixes = std::env::var_os("HOMEBREW_PREFIX")
        .map(PathBuf::from)
        .into_iter()
        .chain(["/opt/homebrew", "/usr/local"].map(PathBuf::from));
    prefixes
        .into_iter()
        .find(|p| p.join("Caskroom").join(cask).is_dir())
        .map(|p| p.join("bin/brew"))
        .filter(|b| b.is_file())
}

pub(crate) fn method() -> Result<Method, String> {
    let own = own_bundle()?;
    // Homebrew's copy is the one in Applications (its `appdir`); another
    // copy elsewhere is not the one it would upgrade.
    let in_applications = own.parent().is_some_and(|f| {
        f == Path::new("/Applications")
            || std::env::var_os("HOME").is_some_and(|h| f == Path::new(&h).join("Applications"))
    });
    if let Some(brew) = homebrew(CASK).filter(|_| in_applications) {
        return Ok(Method::Homebrew { own, brew });
    }
    let folder = own
        .parent()
        .ok_or("the app's folder is unknown")?
        .to_path_buf();
    // The bundle is replaced in its folder: that takes writing there
    // (/Applications: an administrator's account).
    let probe = folder.join(format!(".{CASK}-update-probe"));
    std::fs::create_dir(&probe)
        .and_then(|()| std::fs::remove_dir(&probe))
        .map_err(|_| {
            format!(
                "This account cannot change {}: an administrator can install the update.",
                folder.display()
            )
        })?;
    Ok(Method::Bundles { own, folder })
}

/// Whether the release's archive is downloaded first: Homebrew gets its own.
pub(crate) fn needs_archive(method: &Method) -> bool {
    matches!(method, Method::Bundles { .. })
}

/// ditto, as the Finder unpacks a zip: extended attributes and the
/// bundles' signatures as they were packed.
pub(crate) fn unpack(archive: &Path, into: &Path) -> Result<(), String> {
    let out = Command::new("/usr/bin/ditto")
        .arg("-x")
        .arg("-k")
        .arg(archive)
        .arg(into)
        .output()
        .map_err(|e| format!("ditto did not run: {e}"))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(format!(
            "the archive did not unpack: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ))
    }
}

/// The Developer ID team that signed `bundle`.
pub(crate) fn team_of(bundle: &Path) -> Result<String, String> {
    let out = Command::new("/usr/bin/codesign")
        .args(["-d", "--verbose=2"])
        .arg(bundle)
        .output()
        .map_err(|e| format!("codesign did not run: {e}"))?;
    // codesign describes the signature on stderr.
    let text = String::from_utf8_lossy(&out.stderr);
    text.lines()
        .find_map(|l| l.strip_prefix("TeamIdentifier="))
        .map(str::trim)
        .filter(|t| !t.is_empty() && *t != "not set")
        .filter(|t| t.chars().all(|c| c.is_ascii_alphanumeric()))
        .map(str::to_string)
        .ok_or_else(|| {
            format!(
                "{} is not signed with a Developer ID, so an update cannot be checked \
                    against it: download the update instead.",
                bundle.display()
            )
        })
}

/// Whether `bundle` is whole and signed by `team`'s Developer ID (a chain
/// to Apple's root, the team in the certificate).
pub(crate) fn verify(bundle: &Path, team: &str) -> Result<(), String> {
    let requirement =
        format!("=anchor apple generic and certificate leaf[subject.OU] = \"{team}\"");
    let out = Command::new("/usr/bin/codesign")
        .args(["--verify", "--deep", "--strict", "-R"])
        .arg(&requirement)
        .arg(bundle)
        .output()
        .map_err(|e| format!("codesign did not run: {e}"))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(format!(
            "{} is not signed by this app's maker: {}",
            bundle
                .file_name()
                .map(|n| n.to_string_lossy())
                .unwrap_or_default(),
            String::from_utf8_lossy(&out.stderr).trim()
        ))
    }
}

/// Put `new` where `target` is, keeping the old bundle aside until the new
/// one is in place; `aside` is where (in the same folder, so moving it
/// there is a rename).
fn swap(new: &Path, target: &Path, aside: &Path) -> Result<(), String> {
    let _ = std::fs::remove_dir_all(aside);
    let had = target.exists();
    if had {
        std::fs::rename(target, aside)
            .map_err(|e| format!("{} could not be moved: {e}", target.display()))?;
    }
    // From the temporary folder: on another volume, a copy.
    let placed = std::fs::rename(new, target).or_else(|_| {
        let out = Command::new("/usr/bin/ditto")
            .arg(new)
            .arg(target)
            .output()
            .map_err(|e| e.to_string())?;
        if out.status.success() {
            Ok(())
        } else {
            Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
        }
    });
    if let Err(e) = placed {
        let _ = std::fs::remove_dir_all(target);
        if had {
            let _ = std::fs::rename(aside, target);
        }
        return Err(format!(
            "{} could not be put in place: {e}",
            target.display()
        ));
    }
    Ok(())
}

/// The bundle in `unpacked` in place of the running one, after checking
/// it; or Homebrew's upgrade, started.
pub(crate) fn place(
    method: &Method,
    unpacked: &Path,
    stage: &Path,
    dir: &Path,
) -> Result<(), String> {
    match method {
        Method::Bundles { own, folder } => {
            let team = team_of(own)?;
            let name = bundle_name();
            let new = unpacked.join(&name);
            if !new.is_dir() {
                return Err(format!("the archive has no {name}"));
            }
            // --deep: the helper inside, which the host runs from, too.
            verify(&new, &team)?;
            replace(unpacked, folder)?;
            relaunch(own)
        }
        Method::Homebrew { own, brew } => homebrew_upgrade(own, brew, stage, dir),
    }
}

/// Swap the bundle, keeping the old one until the new one is in place.
pub(crate) fn replace(unpacked: &Path, folder: &Path) -> Result<(), String> {
    let name = bundle_name();
    let aside = folder.join(format!(".{name}.old"));
    swap(&unpacked.join(&name), &folder.join(&name), &aside)?;
    let _ = std::fs::remove_dir_all(aside);
    Ok(())
}

/// Open the new copy once this one has quit. LaunchServices starts it
/// with its own environment: a data folder this copy was pointed at
/// (`HELIO_DATA_DIR`, `HELIO_HOST_DATA_DIR`) is passed on.
fn relaunch(own: &Path) -> Result<(), String> {
    let mut open = Command::new("/usr/bin/open");
    open.arg("-n");
    for var in ["HELIO_DATA_DIR", "HELIO_HOST_DATA_DIR"] {
        if let Some(value) = std::env::var_os(var).filter(|v| !v.is_empty()) {
            let mut pair = std::ffi::OsString::from(format!("{var}="));
            pair.push(value);
            open.arg("--env").arg(pair);
        }
    }
    open.arg(own)
        .args(["--args", "--after-update", &std::process::id().to_string()])
        .spawn()
        .map(drop)
        .map_err(|e| format!("the new copy did not start: {e}"))
}

/// Start the script that has Homebrew upgrade the cask once this app has
/// quit, and opens the app again.
fn homebrew_upgrade(own: &Path, brew: &Path, stage: &Path, dir: &Path) -> Result<(), String> {
    let script = homebrew_script(std::process::id(), own, brew, &super::log_path(dir));
    let path = stage.join("update.sh");
    std::fs::write(&path, script).map_err(|e| e.to_string())?;
    tracing::info!(cask = CASK, "upgrading with Homebrew once the app has quit");
    // Its own process group: nothing sent to the app's reaches it.
    Command::new("/bin/sh")
        .arg(&path)
        .process_group(0)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map(drop)
        .map_err(|e| format!("the Homebrew script did not start: {e}"))
}

/// The script that, once process `pid` (the app) has quit, has Homebrew
/// upgrade the cask (its output to `log`), and opens `own` again. The host
/// follows the new app by itself, and the cask leaves the app's login item
/// alone (`tools/update-casks`).
fn homebrew_script(pid: u32, own: &Path, brew: &Path, log: &Path) -> String {
    let prefix_bin = brew.parent().unwrap_or(Path::new("/opt/homebrew/bin"));
    format!(
        "#!/bin/sh\n\
         # {APP}'s update by Homebrew (helio-update), once {APP} has quit.\n\
         while kill -0 {pid} 2>/dev/null; do sleep 0.2; done\n\
         PATH={path}:/usr/bin:/bin:/usr/sbin:/sbin {brew} upgrade --cask {CASK} >>{log} 2>&1\n\
         open -n {own}\n",
        path = sh_quote(&prefix_bin.to_string_lossy()),
        brew = sh_quote(&brew.to_string_lossy()),
        log = sh_quote(&log.to_string_lossy()),
        own = sh_quote(&own.to_string_lossy()),
    )
}

/// `install::rehearse`: the bundle checked against the team of the one in
/// `folder` (or, the first time, against its own) and swapped in.
pub(crate) fn rehearse(unpacked: &Path, folder: &Path) -> Result<(), String> {
    let name = bundle_name();
    let new = unpacked.join(&name);
    let present = folder.join(&name);
    let team = team_of(if present.exists() { &present } else { &new })?;
    verify(&new, &team)?;
    replace(unpacked, folder)
}

/// Nothing is left behind on a Mac: the old bundle goes once the new one
/// is in place.
pub(crate) fn tidy() {}

/// A Mac lists an app by where it is (Applications): there is no entry to
/// keep.
pub(crate) fn keep_listed(_version: &str) {}

/// Wait until process `pid` has exited, or `timeout`.
pub(crate) fn wait_for_exit(pid: u32, timeout: Duration) {
    let Ok(pid) = libc::pid_t::try_from(pid) else {
        return;
    };
    let until = Instant::now() + timeout;
    // SAFETY: signal 0 only asks whether the process exists.
    while unsafe { libc::kill(pid, 0) } == 0 && Instant::now() < until {
        std::thread::sleep(Duration::from_millis(100));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_homebrew_script_is_a_shell_script_that_parses() {
        let dir = tempfile::tempdir().unwrap();
        let script = homebrew_script(
            4242,
            Path::new("/Applications/Helio.app"),
            Path::new("/opt/homebrew/bin/brew"),
            Path::new("/Users/o'brien/Library/Application Support/Helio/update.log"),
        );
        let path = dir.path().join("update.sh");
        std::fs::write(&path, &script).unwrap();
        let out = Command::new("/bin/sh")
            .arg("-n")
            .arg(&path)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}\n{script}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(script.contains("kill -0 4242"));
        assert!(script.contains("upgrade --cask helio >>'/Users/o'\\''brien/"));
        // The host follows the new app by itself, and the cask keeps the
        // login item: the script touches neither.
        assert!(!script.contains("LaunchAgents") && !script.contains("enable"));
        assert!(script.contains(r"open -n '/Applications/Helio.app'"));
    }
}
