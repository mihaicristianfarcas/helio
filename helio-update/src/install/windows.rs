//! Installing an update on Windows.
//!
//! The app replaces its files, the host's engine (`helio-host.exe`) and the
//! `helio` command (`bin\helio.exe`) among them, in the folder it runs from
//! (per user, so no administrator is needed). A
//! running program or loaded DLL cannot be written over or deleted, but it
//! can be renamed: the old files are renamed `*.old` and the new ones
//! copied beside them; the next start deletes the old ones. The archive is
//! the release's zip, the installer's files and nothing else
//! (`tools/package-windows.ps1`), and no installer runs: the new app, when
//! it starts, brings the installer's entry in Settings > Apps to its
//! version ([`keep_listed`]).
//!
//! The host itself runs from the engine's own copy in Program Files, which
//! only `helio-host enable`, as an administrator, replaces
//! (`helio-host/src/hosting/windows.rs`): while hosting is on, the update runs it
//! again from the new files, and Windows asks ([`update_host`]).

use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use super::ps_quote;

/// How this copy is updated: the files in `dir`.
pub(crate) struct Method {
    dir: PathBuf,
}

/// No console window for a script or a tool.
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Windows' own programs, by full path: never one that happens to be on
/// PATH or in the current folder.
fn system32(program: &str) -> PathBuf {
    let root = std::env::var_os("SystemRoot")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Windows"));
    root.join("System32").join(program)
}

fn powershell() -> PathBuf {
    system32(r"WindowsPowerShell\v1.0\powershell.exe")
}

pub(crate) fn method() -> Result<Method, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let dir = exe
        .parent()
        .ok_or("Helio's folder is unknown")?
        .to_path_buf();
    let probe = dir.join(".helio-update-probe");
    std::fs::write(&probe, b"")
        .and_then(|()| std::fs::remove_file(&probe))
        .map_err(|_| {
            format!(
                "Helio cannot change its own folder ({}): download the update instead.",
                dir.display()
            )
        })?;
    Ok(Method { dir })
}

pub(crate) fn needs_archive(_method: &Method) -> bool {
    true
}

/// Windows' own tar (Windows 10 1803 and later) unpacks a zip; failing
/// that, PowerShell's Expand-Archive.
pub(crate) fn unpack(archive: &Path, into: &Path) -> Result<(), String> {
    let tar = system32("tar.exe");
    let out = if tar.is_file() {
        Command::new(tar)
            .arg("-xf")
            .arg(archive)
            .arg("-C")
            .arg(into)
            .creation_flags(CREATE_NO_WINDOW)
            .output()
    } else {
        Command::new(powershell())
            .args(["-NoProfile", "-NonInteractive", "-Command"])
            .arg(format!(
                "Expand-Archive -LiteralPath {} -DestinationPath {} -Force",
                ps_quote(&archive.to_string_lossy()),
                ps_quote(&into.to_string_lossy())
            ))
            .creation_flags(CREATE_NO_WINDOW)
            .output()
    }
    .map_err(|e| format!("the archive could not be unpacked: {e}"))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(format!(
            "the archive did not unpack: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ))
    }
}

pub(crate) fn place(
    method: &Method,
    unpacked: &Path,
    _stage: &Path,
    _dir: &Path,
) -> Result<(), String> {
    let folder = &method.dir;
    replace_files(unpacked, folder)?;
    update_host(folder);
    let new = folder.join("Helio.exe");
    Command::new(&new)
        .args(["--after-update", &std::process::id().to_string()])
        .spawn()
        .map(drop)
        .map_err(|e| format!("the new Helio did not start: {e}"))
}

/// While hosting is on, `helio-host enable` from the new files in `folder`, as an
/// administrator (Windows asks, as when hosting was turned on): it puts the
/// new engine in Program Files and restarts the service on it. Without it
/// the host would go on running the engine of the release before. Not
/// allowed, or failed: the app is updated all the same, the host keeps the
/// engine it had, and turning hosting off and on brings it up to date.
fn update_host(folder: &Path) {
    let engine = folder.join("helio-host.exe");
    let status = Command::new(&engine)
        .args(["status", "--json"])
        .creation_flags(CREATE_NO_WINDOW)
        .output();
    if !status.is_ok_and(|out| hosting_on(&out.stdout)) {
        return;
    }
    tracing::info!("hosting is on: the host's engine is updated as an administrator");
    // Stop: a refused prompt is an error, and the exit code says so.
    let script = format!(
        "$ErrorActionPreference = 'Stop'; \
         $p = Start-Process -FilePath {} -ArgumentList enable -Verb RunAs -WindowStyle Hidden \
         -Wait -PassThru; exit $p.ExitCode",
        ps_quote(&engine.to_string_lossy())
    );
    match Command::new(powershell())
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
    {
        Ok(out) if out.status.success() => tracing::info!("the host runs the new engine"),
        Ok(out) => tracing::warn!(
            code = ?out.status.code(),
            error = %String::from_utf8_lossy(&out.stderr).trim(),
            "the host's engine was not updated: it runs the one it had until hosting is \
             turned off and on"
        ),
        Err(e) => tracing::warn!(error = %e, "the host's engine was not updated"),
    }
}

/// Whether what `helio-host status --json` printed says hosting is on (starting,
/// on, or on but failing: the service runs an engine either way).
fn hosting_on(status: &[u8]) -> bool {
    serde_json::from_slice::<serde_json::Value>(status)
        .ok()
        .and_then(|v| v.get("state")?.as_str().map(|s| s != "off"))
        .unwrap_or(false)
}

/// Whether `name` is one of the app's own files: only those are renamed
/// aside or deleted, in a folder that may hold anything (Downloads).
fn ours(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    let dll = |prefix: &str| {
        lower
            .strip_prefix(prefix)
            .and_then(|rest| rest.strip_suffix(".dll"))
            .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
    };
    lower == "helio.exe"
        || lower == "helio-host.exe"
        || lower == "ffmpeg license.txt"
        || dll("avcodec-")
        || dll("avutil-")
        || dll("swresample-")
}

/// The `helio` command, in a folder of its own beside the app's files, which
/// Helio's Settings > General puts on the PATH (helio-app's command.rs):
/// `helio.exe` cannot sit beside `Helio.exe`.
const COMMAND: &str = r"bin\helio.exe";

/// The name a file renamed aside gets: `Helio.exe.old`, or
/// `Helio.exe.2.old` while an older one is still held.
fn aside(path: &Path) -> PathBuf {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let first = path.with_file_name(format!("{name}.old"));
    if std::fs::remove_file(&first).is_ok() || !first.exists() {
        return first;
    }
    (2..100)
        .map(|n| path.with_file_name(format!("{name}.{n}.old")))
        .find(|p| std::fs::remove_file(p).is_ok() || !p.exists())
        .unwrap_or(first)
}

/// The archive's program files over Helio's in `dir`, all or none: those
/// beside Helio.exe, and the command in its folder ([`COMMAND`]). FFmpeg's
/// DLLs of an older version (another number) are renamed aside too: the
/// uninstaller deletes what an update leaves (`tools/windows/helio.iss`).
fn replace_files(unpacked: &Path, dir: &Path) -> Result<(), String> {
    let mut new: Vec<(PathBuf, PathBuf)> = std::fs::read_dir(unpacked)
        .map_err(|e| e.to_string())?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.is_file() && p.file_name().is_some_and(|n| ours(&n.to_string_lossy())))
        .filter_map(|p| {
            let to = dir.join(p.file_name()?);
            Some((p, to))
        })
        .collect();
    let names: Vec<String> = new
        .iter()
        .filter_map(|(p, _)| {
            p.file_name()
                .map(|n| n.to_string_lossy().to_ascii_lowercase())
        })
        .collect();
    if !names.iter().any(|n| n == "helio.exe") {
        return Err("the archive has no Helio.exe".into());
    }
    let command = unpacked.join(COMMAND);
    if command.is_file() {
        let to = dir.join(COMMAND);
        if let Some(bin) = to.parent() {
            std::fs::create_dir_all(bin).map_err(|e| format!("{}: {e}", bin.display()))?;
        }
        new.push((command, to));
    }
    let stale: Vec<PathBuf> = std::fs::read_dir(dir)
        .map_err(|e| e.to_string())?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .is_some_and(|n| ours(&n) && !names.contains(&n.to_ascii_lowercase()))
        })
        .collect();

    let mut moved: Vec<(PathBuf, PathBuf)> = Vec::new();
    let mut written: Vec<PathBuf> = Vec::new();
    let result = (|| -> Result<(), String> {
        for old in &stale {
            let to = aside(old);
            std::fs::rename(old, &to).map_err(|e| format!("{}: {e}", old.display()))?;
            moved.push((old.clone(), to));
        }
        for (file, target) in &new {
            if target.exists() {
                let to = aside(target);
                std::fs::rename(target, &to).map_err(|e| format!("{}: {e}", target.display()))?;
                moved.push((target.clone(), to));
            }
            std::fs::copy(file, target).map_err(|e| format!("{}: {e}", target.display()))?;
            written.push(target.clone());
        }
        Ok(())
    })();
    if result.is_err() {
        for path in &written {
            let _ = std::fs::remove_file(path);
        }
        for (from, to) in moved.iter().rev() {
            let _ = std::fs::rename(to, from);
        }
    }
    result
}

/// Where the installer (`tools/windows/helio.iss`) installs Helio.
fn helio_install_folder() -> Option<PathBuf> {
    std::env::var_os("LOCALAPPDATA").map(|dir| PathBuf::from(dir).join(r"Programs\Helio"))
}

/// Helio's entry in Settings > Apps, under HKEY_CURRENT_USER, as the
/// installer names it: its `AppId`, to which Inno Setup adds `_is1`.
const HELIO_ENTRY: &str =
    r"Software\Microsoft\Windows\CurrentVersion\Uninstall\dev.mihaicristian.helio_is1";

/// Whether two paths name the same folder, as Windows compares them:
/// without regard to case or a trailing separator.
fn same_folder(a: &Path, b: &Path) -> bool {
    let norm = |p: &Path| p.to_string_lossy().trim_end_matches('\\').to_lowercase();
    norm(a) == norm(b)
}

/// The version Helio's entry in Settings > Apps names, if there is one.
fn listed_version() -> Option<String> {
    use windows::core::HSTRING;
    use windows::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_SZ};
    let mut text = [0u16; 64];
    let mut bytes = std::mem::size_of_val(&text) as u32;
    // SAFETY: `text` is writable for the `bytes` said; RegGetValueW writes at
    // most that, ends a REG_SZ with a NUL, and says how many bytes it wrote.
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            &HSTRING::from(HELIO_ENTRY),
            &HSTRING::from("DisplayVersion"),
            RRF_RT_REG_SZ,
            None,
            Some(text.as_mut_ptr().cast()),
            Some(&mut bytes),
        )
    };
    if !status.is_ok() {
        return None;
    }
    let len = (bytes as usize / 2).saturating_sub(1).min(text.len());
    Some(String::from_utf16_lossy(&text[..len]))
}

/// At each start: a Helio that runs from the folder the installer put it
/// in keeps the installer's entry in Settings > Apps naming its `version`.
/// Helio's own update replaces the files and runs no installer, so the
/// entry would go on naming the release that was installed. A folder with
/// no entry (files copied there by hand) is given none: the entry is the
/// uninstaller's, and only the installer puts one there. Run on the update
/// check's thread, not the window's.
pub(crate) fn keep_listed(version: &str) {
    use windows::core::{w, HSTRING};
    use windows::Win32::System::Registry::{RegSetKeyValueW, HKEY_CURRENT_USER, REG_SZ};
    let Some(dir) = std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(Path::to_path_buf))
    else {
        return;
    };
    if !helio_install_folder().is_some_and(|f| same_folder(&dir, &f)) {
        return;
    }
    match listed_version() {
        Some(listed) if listed != version => {}
        _ => return,
    }
    tracing::info!(version, "Settings > Apps: Helio's entry names this version");
    let wide: Vec<u16> = version.encode_utf16().chain([0]).collect();
    // SAFETY: `wide` is NUL-ended, lives across the call, and is `cbdata`
    // bytes long, its NUL included, as REG_SZ wants.
    let status = unsafe {
        RegSetKeyValueW(
            HKEY_CURRENT_USER,
            &HSTRING::from(HELIO_ENTRY),
            w!("DisplayVersion"),
            REG_SZ.0,
            Some(wide.as_ptr().cast()),
            (wide.len() * 2) as u32,
        )
    };
    if let Err(e) = status.ok() {
        tracing::warn!(error = %e, "Helio's entry in Settings > Apps was not updated");
    }
}

/// `install::rehearse`: the app's files replaced in `folder` as in its
/// own. The host and the entry in Settings > Apps are left alone.
pub(crate) fn rehearse(unpacked: &Path, folder: &Path) -> Result<(), String> {
    replace_files(unpacked, folder)
}

/// Delete what an update renamed aside in Helio's folder and the command's
/// ([`COMMAND`]), now that nothing holds it.
pub(crate) fn tidy() {
    let Some(dir) = std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(Path::to_path_buf))
    else {
        return;
    };
    tidy_in(&dir);
    if let Some(bin) = dir.join(COMMAND).parent() {
        tidy_in(bin);
    }
}

fn tidy_in(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for path in entries.filter_map(Result::ok).map(|e| e.path()) {
        let Some(name) = path.file_name().map(|n| n.to_string_lossy().into_owned()) else {
            continue;
        };
        // name.old, name.N.old
        let Some(rest) = name.strip_suffix(".old") else {
            continue;
        };
        let original = match rest.rsplit_once('.') {
            Some((stem, n)) if n.bytes().all(|b| b.is_ascii_digit()) && !n.is_empty() => stem,
            _ => rest,
        };
        if ours(original) {
            let _ = std::fs::remove_file(&path);
        }
    }
}

/// Wait until process `pid` has exited, or `timeout`.
pub(crate) fn wait_for_exit(pid: u32, timeout: Duration) {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{
        OpenProcess, WaitForSingleObject, PROCESS_SYNCHRONIZE,
    };
    // SAFETY: the handle is the process's, opened just for waiting, and
    // closed after.
    unsafe {
        let Ok(process) = OpenProcess(PROCESS_SYNCHRONIZE, false, pid) else {
            // Gone already (or never ours to see).
            return;
        };
        WaitForSingleObject(process, timeout.as_millis().min(u32::MAX as u128) as u32);
        let _ = CloseHandle(process);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_host_is_updated_only_while_hosting_is_on() {
        assert!(!hosting_on(br#"{"state":"off"}"#));
        assert!(hosting_on(br#"{"state":"on","api":true}"#));
        assert!(hosting_on(br#"{"state":"starting"}"#));
        assert!(hosting_on(
            br#"{"state":"failed","reason":"the host exited with code 3"}"#
        ));
        // An engine that could not say: nothing is asked of the user.
        assert!(!hosting_on(b""));
        assert!(!hosting_on(b"not json"));
    }

    #[test]
    fn an_update_replaces_the_command_in_its_folder_too() {
        let dir = tempfile::tempdir().unwrap();
        let (unpacked, folder) = (dir.path().join("unpacked"), dir.path().join("Helio"));
        for (path, text) in [
            (unpacked.join("Helio.exe"), "new app"),
            (unpacked.join(COMMAND), "new command"),
            (folder.join("Helio.exe"), "old app"),
        ] {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        }
        // A copy from before the command: its folder is made.
        replace_files(&unpacked, &folder).unwrap();
        assert_eq!(
            std::fs::read_to_string(folder.join(COMMAND)).unwrap(),
            "new command"
        );
        // And the next one renames the old command aside, as it does the
        // app (a terminal may be running it).
        std::fs::write(unpacked.join(COMMAND), "newer command").unwrap();
        replace_files(&unpacked, &folder).unwrap();
        assert_eq!(
            std::fs::read_to_string(folder.join(COMMAND)).unwrap(),
            "newer command"
        );
        assert!(folder.join(r"bin\helio.exe.old").is_file());
        assert_eq!(
            std::fs::read_to_string(folder.join("Helio.exe")).unwrap(),
            "new app"
        );
    }

    #[test]
    fn a_folder_is_the_same_whatever_its_case_or_a_trailing_separator() {
        let installed = Path::new(r"C:\Users\o'brien\AppData\Local\Programs\Helio");
        assert!(same_folder(
            Path::new(r"c:\users\O'BRIEN\appdata\local\programs\helio\"),
            installed
        ));
        assert!(!same_folder(
            Path::new(r"C:\Users\o'brien\Downloads\Helio-0.9.5-windows-x86_64"),
            installed
        ));
    }

    #[test]
    fn only_the_apps_own_files_are_renamed_or_deleted() {
        for name in [
            "Helio.exe",
            "helio-host.exe",
            "avcodec-62.dll",
            "AVUTIL-60.dll",
            "swresample-6.dll",
            "FFmpeg LICENSE.txt",
        ] {
            assert!(ours(name), "{name}");
        }
        for name in [
            "thesis.docx",
            "avcodec-.dll",
            "avcodec-x.dll",
            "helio-host.exe.lnk",
            "helio.exe.lnk",
        ] {
            assert!(!ours(name), "{name}");
        }
    }
}
