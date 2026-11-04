//! Installing an update on Linux: the release archive's `install.sh`, as
//! the first install ran it (`tools/linux/install-release`). It puts the
//! app and the host's engine in `~/.local/bin` (over running ones:
//! `install` replaces a file rather than writing into it) and, while
//! hosting is on (the host's user unit is enabled), restarts the host on
//! the new engine.
//!
//! The archive is staged in the user's cache rather than the temp folder:
//! `/tmp` is shared with every other user of the computer, and `install.sh`
//! runs from what is unpacked.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use super::run_logged;

/// How this copy is updated: the programs go to `bin`.
pub(crate) struct Method {
    bin: PathBuf,
}

/// The program that opens the app again.
const APP_PROGRAM: &str = "helio-app";

pub(crate) fn method() -> Result<Method, String> {
    // Where install.sh puts the programs.
    let bin = std::env::var_os("XDG_BIN_HOME")
        .filter(|b| !b.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME")
                .filter(|h| !h.is_empty())
                .map(|h| PathBuf::from(h).join(".local/bin"))
        })
        .ok_or("There is no home folder to install to.")?;
    Ok(Method { bin })
}

pub(crate) fn needs_package(_method: &Method) -> bool {
    true
}

/// The folder updates are staged in, checked to be this user's alone.
pub(crate) fn stage_root() -> Result<PathBuf, String> {
    let root = stage_root_in(std::env::var_os("XDG_CACHE_HOME"), std::env::var_os("HOME"))
        .ok_or("There is no home folder to download the update to.")?;
    // SAFETY: geteuid has no arguments and cannot fail.
    private_dir(&root, unsafe { libc::geteuid() })?;
    Ok(root)
}

/// In `$XDG_CACHE_HOME`, else `~/.cache`. The XDG base directory rules say to
/// ignore a path that is not absolute.
fn stage_root_in(cache: Option<OsString>, home: Option<OsString>) -> Option<PathBuf> {
    let absolute = |dir: Option<OsString>| dir.map(PathBuf::from).filter(|d| d.is_absolute());
    let base = absolute(cache).or_else(|| Some(absolute(home)?.join(".cache")))?;
    Some(base.join("helio-update"))
}

/// Make sure `dir` is a real folder of the user `me` that nobody else can
/// enter, creating it (0700) if it is not there. A link is refused even if it
/// is this user's: where it leads is not checked. `helio-clipboard` checks its
/// own folder in the same way.
fn private_dir(dir: &Path, me: u32) -> Result<(), String> {
    use std::os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt};
    let fail = |why: &str| format!("{}: {why}", dir.display());
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(dir)
        .map_err(|e| fail(&e.to_string()))?;
    let found = std::fs::symlink_metadata(dir).map_err(|e| fail(&e.to_string()))?;
    if !found.is_dir() || found.uid() != me {
        return Err(fail(
            "is not a folder of your own, so the update was not put there. Remove it and try again.",
        ));
    }
    if found.mode() & 0o077 != 0 {
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))
            .map_err(|e| fail(&e.to_string()))?;
    }
    Ok(())
}

/// The archive unpacked in `stage` (its `unpacked` folder, returned).
pub(crate) fn open(archive: &Path, stage: &Path) -> Result<PathBuf, String> {
    let into = stage.join("unpacked");
    std::fs::create_dir_all(&into).map_err(|e| format!("{}: {e}", into.display()))?;
    let out = Command::new("tar")
        .arg("-xzf")
        .arg(archive)
        .arg("-C")
        .arg(&into)
        .output()
        .map_err(|e| format!("tar did not run: {e}"))?;
    if out.status.success() {
        Ok(into)
    } else {
        Err(format!(
            "the archive did not unpack: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ))
    }
}

/// Run the archive's install.sh, then start the new copy, with this copy's
/// environment but for a UI demo's steps, which were for this copy.
pub(crate) fn place(
    method: &Method,
    unpacked: &Path,
    _stage: &Path,
    dir: &Path,
) -> Result<(), String> {
    // The archive is one folder: Helio-0.9.0-linux-x86_64/.
    let folder = std::fs::read_dir(unpacked)
        .map_err(|e| e.to_string())?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .find(|p| p.join("install.sh").is_file())
        .ok_or("the archive has no install.sh")?;
    let mut install = Command::new("bash");
    install.arg(folder.join("install.sh")).current_dir(&folder);
    run_logged(install, dir)?;
    let new = method.bin.join(APP_PROGRAM);
    Command::new(&new)
        .args(["--after-update", &std::process::id().to_string()])
        .env_remove("HELIO_UI_DEMO")
        .spawn()
        .map(drop)
        .map_err(|e| format!("{} did not start: {e}", new.display()))
}

/// `install::rehearse`: the archive's folder copied to `folder`, its
/// install.sh not run (it would restart the user's own host).
pub(crate) fn rehearse(unpacked: &Path, folder: &Path) -> Result<(), String> {
    let out = Command::new("cp")
        .arg("-a")
        .arg(format!("{}/.", unpacked.display()))
        .arg(folder)
        .output()
        .map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

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
    use std::os::unix::fs::{MetadataExt, PermissionsExt};

    use super::*;

    fn me() -> u32 {
        // SAFETY: geteuid has no arguments and cannot fail.
        unsafe { libc::geteuid() }
    }

    fn mode(path: &Path) -> u32 {
        std::fs::symlink_metadata(path).unwrap().mode() & 0o777
    }

    #[test]
    fn updates_are_staged_in_the_users_cache_not_the_shared_temp_folder() {
        assert_eq!(
            stage_root_in(None, Some("/home/someone".into())),
            Some(PathBuf::from("/home/someone/.cache/helio-update"))
        );
        assert_eq!(
            stage_root_in(Some("/caches/mine".into()), Some("/home/someone".into())),
            Some(PathBuf::from("/caches/mine/helio-update"))
        );
    }

    #[test]
    fn a_cache_home_that_is_not_absolute_is_ignored() {
        assert_eq!(
            stage_root_in(Some("caches".into()), Some("/home/someone".into())),
            Some(PathBuf::from("/home/someone/.cache/helio-update"))
        );
        assert_eq!(stage_root_in(Some("".into()), Some("".into())), None);
        assert_eq!(stage_root_in(None, None), None);
    }

    #[test]
    fn a_new_stage_only_its_user_can_enter_is_made() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("cache/helio-update");
        private_dir(&dir, me()).unwrap();
        assert_eq!(mode(&dir), 0o700);
    }

    #[test]
    fn a_stage_others_could_enter_is_closed() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::set_permissions(tmp.path(), std::fs::Permissions::from_mode(0o777)).unwrap();
        private_dir(tmp.path(), me()).unwrap();
        assert_eq!(mode(tmp.path()), 0o700);
    }

    #[test]
    fn a_stage_that_is_a_link_is_refused_and_what_it_leads_to_is_left_alone() {
        let tmp = tempfile::tempdir().unwrap();
        let elsewhere = tmp.path().join("elsewhere");
        std::fs::create_dir(&elsewhere).unwrap();
        std::fs::set_permissions(&elsewhere, std::fs::Permissions::from_mode(0o755)).unwrap();
        let link = tmp.path().join("helio-update");
        std::os::unix::fs::symlink(&elsewhere, &link).unwrap();
        assert!(private_dir(&link, me()).is_err());
        assert_eq!(mode(&elsewhere), 0o755);
    }

    #[test]
    fn a_stage_that_is_a_file_is_refused() {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("helio-update");
        std::fs::write(&file, "").unwrap();
        assert!(private_dir(&file, me()).is_err());
    }

    #[test]
    fn a_stage_of_another_users_is_refused_and_left_as_it_is() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::set_permissions(tmp.path(), std::fs::Permissions::from_mode(0o755)).unwrap();
        assert!(private_dir(tmp.path(), me() + 1).is_err());
        assert_eq!(mode(tmp.path()), 0o755);
    }

    fn script(path: &Path, text: &str) {
        std::fs::write(path, format!("#!/bin/sh\n{text}\n")).unwrap();
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    #[test]
    fn the_archives_install_sh_runs_then_the_new_app_starts_after_this_one() {
        let tmp = tempfile::tempdir().unwrap();
        let (unpacked, bin, data) = (
            tmp.path().join("unpacked"),
            tmp.path().join("bin"),
            tmp.path().join("data"),
        );
        let folder = unpacked.join("Helio-0.10.0-linux-x86_64");
        for d in [&folder, &bin, &data] {
            std::fs::create_dir_all(d).unwrap();
        }
        let installed = tmp.path().join("installed");
        script(
            &folder.join("install.sh"),
            &format!("echo installing; touch {}", installed.display()),
        );
        let started = tmp.path().join("started");
        script(
            &bin.join("helio-app"),
            &format!("echo \"$@\" > {}", started.display()),
        );
        place(&Method { bin }, &unpacked, tmp.path(), &data).unwrap();
        assert!(installed.exists());
        assert_eq!(
            std::fs::read_to_string(super::super::log_path(&data)).unwrap(),
            "installing\n"
        );
        // The new copy is started, told to wait for this one.
        let deadline = Instant::now() + Duration::from_secs(5);
        while !started.exists() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
        }
        std::thread::sleep(Duration::from_millis(100));
        assert_eq!(
            std::fs::read_to_string(&started).unwrap().trim(),
            format!("--after-update {}", std::process::id())
        );
    }

    #[test]
    fn an_install_sh_that_fails_says_why_and_starts_nothing() {
        let tmp = tempfile::tempdir().unwrap();
        let (unpacked, bin, data) = (
            tmp.path().join("unpacked"),
            tmp.path().join("bin"),
            tmp.path().join("data"),
        );
        let folder = unpacked.join("Helio-0.10.0-linux-x86_64");
        for d in [&folder, &bin, &data] {
            std::fs::create_dir_all(d).unwrap();
        }
        script(
            &folder.join("install.sh"),
            "echo 'These libraries are missing:' >&2; echo '  libavcodec.so.60' >&2; exit 1",
        );
        let err = place(&Method { bin }, &unpacked, tmp.path(), &data).unwrap_err();
        assert_eq!(err, "libavcodec.so.60");
    }
}
