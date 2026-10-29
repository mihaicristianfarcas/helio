//! Hosting on Windows: `HelioHost`, run from the engine's own copy in
//! `C:\Program Files\Helio Host`.
//!
//! The service runs as LocalSystem (`service`, which says why), so what it
//! runs must be something no user can replace. The app is installed per
//! user, in a folder its user can write to; a service started from there
//! would run, as SYSTEM, whatever that user (or anything running as them)
//! put in its place. So `helio-host enable`, as an administrator:
//!
//! 1. copies the engine (`ENGINE_FILES`, from beside this `helio-host.exe`) into
//!    `Program Files\Helio Host`, whose permissions it sets itself: only SYSTEM
//!    and Administrators may change it, Users may read and run it;
//! 2. checks the copies' Authenticode signature (`signature`): the copies,
//!    not the originals, which their user could swap after the check;
//! 3. stops the service if it runs, puts the copies in place, registers the
//!    service to run them, opens the firewall to them, and starts it.
//!
//! `helio-host disable` stops and removes the service, its firewall rules and the
//! copy. Without an administrator both change nothing and exit with
//! [`NEEDS_ELEVATION`](super::NEEDS_ELEVATION).
//!
//! The folder is found by its known-folder ID, not `%ProgramFiles%`: an
//! elevated process keeps the environment of whoever started it, and a
//! `ProgramFiles` pointing into a user's folder would put the service there.

use std::path::{Path, PathBuf};
use std::time::Duration;

use windows::core::{w, HSTRING, PCWSTR};
use windows::Win32::Foundation::{LocalFree, HLOCAL};
use windows::Win32::Security::Authorization::{
    ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
};
use windows::Win32::Security::{
    SetFileSecurityW, DACL_SECURITY_INFORMATION, OWNER_SECURITY_INFORMATION,
    PROTECTED_DACL_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR,
};
use windows::Win32::Storage::FileSystem::{MoveFileExW, MOVEFILE_DELAY_UNTIL_REBOOT};
use windows::Win32::System::Com::CoTaskMemFree;
use windows::Win32::UI::Shell::{FOLDERID_ProgramFiles, SHGetKnownFolderPath, KF_FLAG_DEFAULT};

use super::signature;
use super::{Error, State};
use crate::service;

/// The engine: what the service's copy needs, from beside `helio-host.exe`. A
/// DLL the host loaded from beside itself would be listed here; it loads
/// none (NVENC and NVAPI come with the driver, in System32).
const ENGINE_FILES: [&str; 1] = ["helio-host.exe"];

/// A copy on its way in, checked before it replaces the one in place.
const STAGED: &str = "new";
/// A running `helio-host.exe` cannot be deleted, only renamed: `disable` run from
/// the copy itself renames it to this, for Windows to delete at the next
/// restart (a later `enable` deletes it sooner).
const REMOVED: &str = "removed";

/// Security descriptors, in SDDL. `O:BA`: owned by Administrators, not by
/// the administrator who ran `enable` (an owner may always change the
/// permissions, elevated or not). `P`: protected, nothing inherited. SY
/// SYSTEM and BA Administrators: full control; BU Users: read and run
/// (0x1200a9). `OICI`: inherited by what is created inside.
const FOLDER: PCWSTR = w!("O:BAD:P(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)(A;OICI;0x1200a9;;;BU)");
const FILE: PCWSTR = w!("O:BAD:P(A;;FA;;;SY)(A;;FA;;;BA)(A;;0x1200a9;;;BU)");

/// Windows keeps a program's file busy for a moment after its process has
/// ended, and says nothing when it lets go: a copy over it, or its
/// deletion, is tried again this many times, this far apart: 5 s in all.
const TRIES: u32 = 20;
const BETWEEN_TRIES: Duration = Duration::from_millis(250);

/// `C:\Program Files\Helio Host`, where the service's engine lives.
fn install_dir() -> Result<PathBuf, String> {
    // SAFETY: the shell allocates the path; it is copied, then freed once.
    unsafe {
        let path = SHGetKnownFolderPath(&FOLDERID_ProgramFiles, KF_FLAG_DEFAULT, None)
            .map_err(|e| format!("cannot find Program Files: {}", e.message()))?;
        let text = path.to_string();
        CoTaskMemFree(Some(path.0 as *const _));
        Ok(PathBuf::from(text.map_err(|e| e.to_string())?).join("Helio Host"))
    }
}

/// The engine the service runs: `C:\Program Files\Helio Host\helio-host.exe`.
fn engine() -> Result<PathBuf, String> {
    Ok(install_dir()?.join(ENGINE_FILES[0]))
}

/// Whether two paths are the same file or folder (both must exist).
fn same(a: &Path, b: &Path) -> bool {
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

/// This program, when it is the engine in Program Files: the only `helio-host`
/// the service and the host launch into the console session (the host,
/// and the clipboard helper), so that nothing they start comes from a
/// folder a user can write to. Anything else is refused, and says why.
pub fn trusted_engine() -> Result<PathBuf, String> {
    let engine = engine()?;
    let here = std::env::current_exe().map_err(|e| e.to_string())?;
    if same(&here, &engine) {
        Ok(engine)
    } else {
        Err(format!(
            "this helio-host runs from {}, not {}: as SYSTEM, only that copy is started \
                (turning hosting on puts it there: helio host enable)",
            here.display(),
            engine.display()
        ))
    }
}

/// Replace `path`'s owner and access list with `sddl`.
fn secure(path: &Path, sddl: PCWSTR) -> Result<(), String> {
    let mut sd = PSECURITY_DESCRIPTOR::default();
    // SAFETY: `sddl` is a NUL-terminated constant; `sd` receives a
    // descriptor the system allocates, used only until it is freed below.
    unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(sddl, SDDL_REVISION_1, &mut sd, None)
            .map_err(|e| e.message())?;
        let set = SetFileSecurityW(
            &HSTRING::from(path.as_os_str()),
            OWNER_SECURITY_INFORMATION
                | DACL_SECURITY_INFORMATION
                | PROTECTED_DACL_SECURITY_INFORMATION,
            sd,
        );
        let _ = LocalFree(Some(HLOCAL(sd.0)));
        set.ok().map_err(|e| {
            format!(
                "{}: cannot set its permissions: {}",
                path.display(),
                e.message()
            )
        })
    }
}

/// `name` with `suffix` after it: `helio-host.exe.new`.
fn with_suffix(dir: &Path, name: &str, suffix: &str) -> PathBuf {
    dir.join(format!("{name}.{suffix}"))
}

/// Run `op` until it works or [`TRIES`] runs out (see there).
fn retry<T>(mut op: impl FnMut() -> std::io::Result<T>) -> std::io::Result<T> {
    let mut tries = 1;
    loop {
        match op() {
            Err(_) if tries < TRIES => {
                tries += 1;
                std::thread::sleep(BETWEEN_TRIES);
            }
            done => return done,
        }
    }
}

/// Copy the engine from `source` into `dir`, staged (`helio-host.exe.new`), with
/// the folder's and the copies' permissions set.
fn stage(source: &Path, dir: &Path) -> Result<Vec<PathBuf>, String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    secure(dir, FOLDER)?;
    let mut staged = Vec::new();
    for name in ENGINE_FILES {
        let from = source.join(name);
        let to = with_suffix(dir, name, STAGED);
        // The bytes only: a copy (CopyFile) would also take the download's
        // mark of the web and other streams the user's file carries.
        let copied = std::fs::File::open(&from).and_then(|mut f| {
            let mut out = std::fs::File::create(&to)?;
            std::io::copy(&mut f, &mut out)?;
            out.sync_all()
        });
        staged.push(to.clone());
        if let Err(e) = copied
            .map_err(|e| format!("{}: {e}", from.display()))
            .and_then(|()| secure(&to, FILE))
        {
            unstage(&staged);
            return Err(e);
        }
    }
    Ok(staged)
}

fn unstage(staged: &[PathBuf]) {
    for f in staged {
        let _ = std::fs::remove_file(f);
    }
}

/// Check each engine file's signature. A failure refuses when this build
/// requires signatures, and is printed as a warning when it does not.
fn check(files: &[PathBuf]) -> Result<(), String> {
    for (file, name) in files.iter().zip(ENGINE_FILES) {
        if let Err(problem) = signature::check(file) {
            if signature::REQUIRED {
                return Err(format!(
                    "Hosting stays off: {name}, as copied to {}, is refused: {problem}.",
                    file.parent().unwrap_or(file).display()
                ));
            }
            eprintln!("warning: {name}: {problem} (this build does not require a signature)");
        }
    }
    Ok(())
}

/// Delete what an earlier `disable` could not (see [`REMOVED`]).
fn sweep(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        if entry.path().extension().is_some_and(|e| e == REMOVED) {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

pub fn enable() -> Result<String, Error> {
    if !crate::private::elevated() {
        return Err(Error::NeedsElevation);
    }
    let here = std::env::current_exe().map_err(|e| e.to_string())?;
    let source = here.parent().ok_or("helio-host.exe is in no folder")?;
    let dir = install_dir()?;
    // The installed copy run again (an installer's second run): nothing to
    // copy; it is checked where it is.
    let in_place = same(source, &dir);
    let files: Vec<PathBuf> = if in_place {
        ENGINE_FILES.iter().map(|f| dir.join(f)).collect()
    } else {
        stage(source, &dir)?
    };
    if let Err(e) = check(&files) {
        if !in_place {
            unstage(&files);
            // The folder `stage` made, when hosting was not on: a refused
            // engine leaves nothing in Program Files.
            let _ = std::fs::remove_dir(&dir);
        }
        return Err(Error::Failed(e));
    }
    sweep(&dir);
    let was_running = service::stop();
    if !in_place {
        for (staged, name) in files.iter().zip(ENGINE_FILES) {
            if let Err(e) = retry(|| std::fs::rename(staged, dir.join(name))) {
                unstage(&files);
                // The host keeps running on the engine in place rather than
                // stay stopped until someone notices.
                if was_running {
                    service::start();
                }
                return Err(Error::Failed(format!(
                    "cannot put {name} in {}: {e}",
                    dir.display()
                )));
            }
        }
    }
    let engine = engine()?;
    service::register(&engine)?;
    Ok(format!(
        "Hosting is on: HelioHost runs {} from boot, before anyone signs in.",
        engine.display()
    ))
}

pub fn disable() -> Result<String, Error> {
    if !crate::private::elevated() {
        return Err(Error::NeedsElevation);
    }
    service::unregister()?;
    let dir = install_dir()?;
    let here = std::env::current_exe().map_err(|e| e.to_string())?;
    let mut left = None;
    for name in ENGINE_FILES {
        let file = dir.join(name);
        let _ = std::fs::remove_file(with_suffix(&dir, name, STAGED));
        if !file.exists() {
            continue;
        }
        if same(&file, &here) {
            // This program: renamed out of the way, deleted at the next
            // restart. Not under its own name, which a later `enable` will
            // have filled again by then.
            let away = with_suffix(&dir, &format!("{name}.{}", std::process::id()), REMOVED);
            std::fs::rename(&file, &away).map_err(|e| format!("{}: {e}", file.display()))?;
            // SAFETY: the path is a NUL-terminated string alive for the call.
            let _ = unsafe {
                MoveFileExW(
                    &HSTRING::from(away.as_os_str()),
                    PCWSTR::null(),
                    MOVEFILE_DELAY_UNTIL_REBOOT,
                )
            };
            left = Some(away);
        } else {
            retry(|| std::fs::remove_file(&file))
                .map_err(|e| format!("cannot delete {}: {e}", file.display()))?;
        }
    }
    // The folder goes once nothing else is in it (an installer may keep
    // Helio Host's window there).
    let _ = std::fs::remove_dir(&dir);
    Ok(match left {
        None => format!(
            "Hosting is off: HelioHost, its firewall rules and {} are removed.",
            engine()?.display()
        ),
        Some(away) => format!(
            "Hosting is off: HelioHost and its firewall rules are removed. This helio-host.exe \
                is the engine itself: it is now {}, and goes at the next restart.",
            away.display()
        ),
    })
}

pub fn state() -> State {
    service::state()
}
