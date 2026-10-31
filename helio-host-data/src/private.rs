//! Keeping the host's data folder to the host, on Windows.
//!
//! Helio Host's data folder there is under `C:\ProgramData`, whose default
//! permissions let every local user read what is in it and create files in
//! it. The folder holds the host's private keys, its rendezvous keys, the web
//! UI's TLS key and credentials, and the paired clients: readable, they let a
//! local user impersonate the host; and a file planted before the host first
//! wrote it (`web-credentials.toml`) would be an admin account.
//!
//! So the folder is private by default: only SYSTEM, Administrators and the
//! folder's owner may read or write it, and everything created in it inherits
//! that. Two files are readable by every user, because Helio Host's window -- which
//! is not elevated -- needs them to find and trust the host: `config.toml`
//! (the web port) and `web-cert.pem` (the certificate to pin).
//!
//! And the folder must be the host's from the start. Any user may create
//! `C:\ProgramData\Helio Host` before the host first runs, and fill it: with an
//! admin account for the host's API, a paired device of their own, the host's
//! keys. Its owner would also keep the right to change its permissions. So
//! the host trusts the folder only when SYSTEM or Administrators own it; it
//! creates it owned by Administrators, private from its first instant (not
//! private a moment later, when something could already be in it); and a
//! folder someone else made is moved aside, whole, for a person to look at,
//! and the host starts afresh. Only an administrator (or the host, as
//! SYSTEM) creates or moves it: anyone else's `helio host` command leaves it
//! be (and does not run: helio-cli's `host`).
//!
//! Looks removable, is not: the reparse-point checks. Any user may also
//! make the folder a junction (no privilege needed), and Windows reads an
//! owner and sets permissions through it: a junction to a folder SYSTEM
//! owns and users may write in would pass the owner check, and the host's
//! permissions would be written over that folder's. So a folder that is a
//! link is moved aside like one someone else made (the link moves, not
//! what it points to), and a link inside the folder is deleted rather than
//! given permissions. An owner that cannot be read counts as someone
//! else's. When the folder still cannot be made the host's (a file in it
//! held open stops the move), [`secure_data_dir`] says so and the host
//! uses nothing in it, not even for its log.
//! A data folder chosen with `HELIO_HOST_DATA_DIR` is its chooser's business, and
//! is only made private.
//!
//! On macOS and Linux the host runs as its user, in that user's own folder,
//! and secrets are written 0600: nothing to do here.

use std::path::Path;

/// Files in the data folder every local user may read.
#[cfg_attr(not(windows), allow(dead_code))]
const PUBLIC_FILES: [&str; 2] = ["config.toml", "web-cert.pem"];

/// Make `dir` the host's and private (see the module's doc), along with
/// what is already in it. Called when the host starts, before it creates
/// anything there.
///
/// A caller who may not change the folder (a user who is not an
/// administrator, listing clients) changes nothing. Returns what it did
/// that the log should say, as it runs before logging starts; an error when
/// the host's own folder could not be made the host's, which the caller
/// must then not read or write at all.
pub fn secure_data_dir(dir: &Path) -> Result<Option<String>, String> {
    imp::secure_data_dir(dir)
}

/// Whether this process has an administrator's rights (elevated, or
/// SYSTEM).
#[cfg(windows)]
pub fn elevated() -> bool {
    imp::elevated()
}

/// Let every local user read `path` (one of [`PUBLIC_FILES`], after it is
/// written: a new file is born private).
pub fn make_public(path: &Path) {
    imp::make_public(path)
}

/// A file only SYSTEM and Administrators can read, not even its owner: the
/// host's tokens, which a window that is not elevated must sign in for.
pub fn restrict_to_admins(path: &Path) -> std::io::Result<()> {
    imp::restrict_to_admins(path)
}

#[cfg(windows)]
mod imp {
    use std::path::Path;

    use windows::core::{w, BOOL, HSTRING, PCWSTR};
    use windows::Win32::Foundation::{LocalFree, HLOCAL};
    use windows::Win32::Security::Authorization::{
        ConvertStringSecurityDescriptorToSecurityDescriptorW, GetNamedSecurityInfoW,
        SDDL_REVISION_1, SE_FILE_OBJECT,
    };
    use windows::Win32::Security::{
        CheckTokenMembership, CreateWellKnownSid, IsWellKnownSid, SetFileSecurityW,
        WinBuiltinAdministratorsSid, WinLocalSystemSid, DACL_SECURITY_INFORMATION,
        OWNER_SECURITY_INFORMATION, PROTECTED_DACL_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR,
        PSID, SECURITY_ATTRIBUTES, SECURITY_MAX_SID_SIZE,
    };
    use windows::Win32::Storage::FileSystem::{
        CreateDirectoryW, FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_REPARSE_POINT,
    };

    use super::PUBLIC_FILES;

    // Security descriptors, in SDDL. `P`: protected (nothing inherited from
    // ProgramData). SY: SYSTEM, BA: Administrators, OW: whoever owns the
    // object (so a host run by hand in a folder of the user's own keeps its
    // files), BU: Users. `OICI`: inherited by files and folders inside.

    /// The data folder: private, and what is created in it is too. Users may
    /// list it and pass through it (to the two public files), no more.
    const FOLDER: PCWSTR =
        w!("D:P(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)(A;OICI;FA;;;OW)(A;;0x1200a9;;;BU)");
    /// The same, owned by Administrators (`O:BA`): the default folder, as the
    /// host creates it.
    const HOST_FOLDER: PCWSTR =
        w!("O:BAD:P(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)(A;OICI;FA;;;OW)(A;;0x1200a9;;;BU)");
    /// A folder inside it (the logs).
    const SUBFOLDER: PCWSTR = w!("D:P(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)(A;OICI;FA;;;OW)");
    /// A file that was there before the folder was private.
    const PRIVATE_FILE: PCWSTR = w!("D:P(A;;FA;;;SY)(A;;FA;;;BA)(A;;FA;;;OW)");
    /// One of the public files.
    const PUBLIC_FILE: PCWSTR = w!("D:P(A;;FA;;;SY)(A;;FA;;;BA)(A;;FA;;;OW)(A;;FR;;;BU)");
    /// The host's tokens.
    const ADMINS_ONLY: PCWSTR = w!("D:P(A;;FA;;;SY)(A;;FA;;;BA)");

    /// Replace `path`'s access list with `sddl`.
    fn set_dacl(path: &Path, sddl: PCWSTR) -> std::io::Result<()> {
        let mut sd = PSECURITY_DESCRIPTOR::default();
        // SAFETY: `sddl` is a NUL-terminated constant; `sd` receives a
        // descriptor the system allocates, used only until it is freed below.
        unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                sddl,
                SDDL_REVISION_1,
                &mut sd,
                None,
            )
            .map_err(std::io::Error::other)?;
            let ok = SetFileSecurityW(
                &HSTRING::from(path.as_os_str()),
                DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
                sd,
            );
            let _ = LocalFree(Some(HLOCAL(sd.0)));
            ok.ok().map_err(std::io::Error::other)
        }
    }

    pub fn elevated() -> bool {
        let mut sid = [0u8; SECURITY_MAX_SID_SIZE as usize];
        let mut size = sid.len() as u32;
        let psid = PSID(sid.as_mut_ptr().cast());
        let mut member = BOOL(0);
        // SAFETY: `sid` holds any SID (SECURITY_MAX_SID_SIZE) and `size` says
        // how much; the token checked is this thread's or process's own.
        unsafe {
            CreateWellKnownSid(WinBuiltinAdministratorsSid, None, Some(psid), &mut size).is_ok()
                && CheckTokenMembership(None, psid, &mut member).is_ok()
                && member.as_bool()
        }
    }

    pub fn secure_data_dir(dir: &Path) -> Result<Option<String>, String> {
        let mut said = None;
        let default = std::env::var_os("HELIO_HOST_DATA_DIR").is_none();
        if default {
            if !elevated() {
                // The host, which is, does it when it starts.
                return Ok(None);
            }
            said = claim(dir).map_err(|e| {
                format!(
                    "{} could not be made the host's own ({e}), so the host does not use it: \
                        sign out the other users of this PC (one may hold a file in it open), \
                        or remove the folder as an administrator, and start the host again",
                    dir.display()
                )
            })?;
        } else if std::fs::create_dir_all(dir).is_err() {
            return Ok(None);
        }
        if set_dacl(dir, FOLDER).is_err() {
            // Not ours to change (not an administrator).
            return Ok(said);
        }
        // What is already there keeps the permissions it was created with
        // (changing a folder's does not reach into it), so set each.
        secure_entries(dir, true, default);
        Ok(said)
    }

    /// Whether `meta` (read without following links) is a reparse point: a
    /// junction or a symbolic link, which Windows follows by path.
    fn is_link(meta: &std::fs::Metadata) -> bool {
        use std::os::windows::fs::MetadataExt;
        meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT.0 != 0
    }

    /// Make sure the default folder `dir` is one SYSTEM or Administrators
    /// made: create it so, or move one someone else made aside first. What
    /// was moved, for the log.
    fn claim(dir: &Path) -> std::io::Result<Option<String>> {
        let mut moved = None;
        let found = match std::fs::symlink_metadata(dir) {
            Ok(meta) => Some(meta),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => return Err(e),
        };
        if let Some(meta) = found {
            // An owner that cannot be read is no proof it is ours.
            if !is_link(&meta) && owned_by_host(dir).unwrap_or(false) {
                return Ok(None);
            }
            let secs = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_secs());
            let name = dir.file_name().unwrap_or(dir.as_os_str()).to_string_lossy();
            let aside = dir.with_file_name(format!("{name}.untrusted-{secs}"));
            std::fs::rename(dir, &aside)?;
            moved = Some(format!(
                "{} was not made by the host (a link elsewhere, or its owner is neither SYSTEM \
                    nor Administrators), so anything in it may have been planted: it is now {}, \
                    and the host starts afresh (devices pair again)",
                dir.display(),
                aside.display()
            ));
        }
        create_private(dir)?;
        Ok(moved)
    }

    /// Whether SYSTEM or Administrators own `path`.
    fn owned_by_host(path: &Path) -> std::io::Result<bool> {
        let mut owner = PSID::default();
        let mut sd = PSECURITY_DESCRIPTOR::default();
        // SAFETY: `owner` points into `sd`, which the system allocates; it
        // is read before `sd` is freed, once.
        unsafe {
            let status = GetNamedSecurityInfoW(
                &HSTRING::from(path.as_os_str()),
                SE_FILE_OBJECT,
                OWNER_SECURITY_INFORMATION,
                Some(&mut owner),
                None,
                None,
                None,
                &mut sd,
            );
            if status.is_err() {
                return Err(std::io::Error::from_raw_os_error(status.0 as i32));
            }
            let ours = IsWellKnownSid(owner, WinLocalSystemSid).as_bool()
                || IsWellKnownSid(owner, WinBuiltinAdministratorsSid).as_bool();
            let _ = LocalFree(Some(HLOCAL(sd.0)));
            Ok(ours)
        }
    }

    /// Create the folder `dir` owned by Administrators and private, in one
    /// step: there is no moment when another user could put a file in it.
    fn create_private(dir: &Path) -> std::io::Result<()> {
        let mut sd = PSECURITY_DESCRIPTOR::default();
        // SAFETY: `HOST_FOLDER` is a NUL-terminated constant; `sd` is the
        // descriptor the system allocates for it, alive until it is freed
        // below, after the call that reads it.
        unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                HOST_FOLDER,
                SDDL_REVISION_1,
                &mut sd,
                None,
            )
            .map_err(std::io::Error::other)?;
            let attributes = SECURITY_ATTRIBUTES {
                nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
                lpSecurityDescriptor: sd.0,
                bInheritHandle: BOOL(0),
            };
            let made = CreateDirectoryW(&HSTRING::from(dir.as_os_str()), Some(&attributes));
            let _ = LocalFree(Some(HLOCAL(sd.0)));
            made.map_err(std::io::Error::other)
        }
    }

    /// Set the permissions of what is in `dir` (the data folder when `top`).
    /// A link is never followed: in the host's own folder (`remove_links`)
    /// it is deleted (see the module's doc), in a chosen one left as it is.
    fn secure_entries(dir: &Path, top: bool, remove_links: bool) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            // A directory entry's metadata is the link's own, not followed.
            if let Some(meta) = entry.metadata().ok().filter(is_link) {
                if remove_links {
                    use std::os::windows::fs::MetadataExt;
                    // The link goes, not what it points to.
                    let removed = if meta.file_attributes() & FILE_ATTRIBUTE_DIRECTORY.0 != 0 {
                        std::fs::remove_dir(&path)
                    } else {
                        std::fs::remove_file(&path)
                    };
                    if let Err(e) = removed {
                        tracing::warn!(path = %path.display(), error = %e,
                            "could not remove a link the host did not make");
                    }
                }
                continue;
            }
            let is_dir = entry.file_type().is_ok_and(|t| t.is_dir());
            let name = entry.file_name();
            let public = top && PUBLIC_FILES.iter().any(|p| name == *p);
            let result = if is_dir {
                set_dacl(&path, SUBFOLDER)
            } else if public {
                set_dacl(&path, PUBLIC_FILE)
            } else if top && is_token(&name) {
                set_dacl(&path, ADMINS_ONLY)
            } else {
                set_dacl(&path, PRIVATE_FILE)
            };
            if let Err(e) = result {
                tracing::warn!(path = %path.display(), error = %e, "could not set permissions");
            }
            if is_dir {
                secure_entries(&path, false, remove_links);
            }
        }
    }

    /// The files `restrict_to_admins` guards (see `web`): they stay closed
    /// to their owner too.
    fn is_token(name: &std::ffi::OsStr) -> bool {
        name == "local-token" || name == "app-tokens.toml"
    }

    pub fn make_public(path: &Path) {
        if let Err(e) = set_dacl(path, PUBLIC_FILE) {
            tracing::warn!(
                path = %path.display(),
                error = %e,
                "could not make the file readable by Helio Host's window"
            );
        }
    }

    pub fn restrict_to_admins(path: &Path) -> std::io::Result<()> {
        set_dacl(path, ADMINS_ONLY)
    }
}

#[cfg(not(windows))]
mod imp {
    use std::path::Path;

    pub fn secure_data_dir(_dir: &Path) -> Result<Option<String>, String> {
        Ok(None)
    }

    pub fn make_public(_path: &Path) {}

    pub fn restrict_to_admins(_path: &Path) -> std::io::Result<()> {
        Ok(())
    }
}
