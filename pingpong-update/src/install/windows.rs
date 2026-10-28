//! Installing an update on Windows.
//!
//! The app replaces its files, the host's engine (`pong.exe`) among them,
//! in the folder it runs from (per user, so no administrator is needed). A
//! running program or loaded DLL cannot be written over or deleted, but it
//! can be renamed: the old files are renamed `*.old` and the new ones
//! copied beside them; the next start deletes the old ones. Then the
//! archive's `install.ps1 -Register FOLDER` brings the app's entry in
//! Settings > Apps up to date, if the folder is the one it installs the app
//! into. An update by an older release runs no installer: the new app lists
//! itself when it starts ([`keep_listed`]).
//!
//! The host itself runs from the engine's own copy in Program Files, which
//! only `pong enable`, as an administrator, replaces
//! (`pong/src/hosting/windows.rs`): while hosting is on, the update runs it
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
        .ok_or("Ping's folder is unknown")?
        .to_path_buf();
    let probe = dir.join(".ping-update-probe");
    std::fs::write(&probe, b"")
        .and_then(|()| std::fs::remove_file(&probe))
        .map_err(|_| {
            format!(
                "Ping cannot change its own folder ({}): download the update instead.",
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
    register(unpacked, folder);
    update_host(folder);
    let new = folder.join("Ping.exe");
    Command::new(&new)
        .args(["--after-update", &std::process::id().to_string()])
        .spawn()
        .map(drop)
        .map_err(|e| format!("the new Ping did not start: {e}"))
}

/// While hosting is on, `pong enable` from the new files in `folder`, as an
/// administrator (Windows asks, as when hosting was turned on): it puts the
/// new engine in Program Files and restarts the service on it. Without it
/// the host would go on running the engine of the release before. Not
/// allowed, or failed: the app is updated all the same, the host keeps the
/// engine it had, and turning hosting off and on brings it up to date.
fn update_host(folder: &Path) {
    let pong = folder.join("pong.exe");
    let status = Command::new(&pong)
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
        ps_quote(&pong.to_string_lossy())
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

/// Whether what `pong status --json` printed says hosting is on (starting,
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
    lower == "ping.exe"
        || lower == "pong.exe"
        || lower == "ffmpeg license.txt"
        || dll("avcodec-")
        || dll("avutil-")
        || dll("swresample-")
}

/// The name a file renamed aside gets: `Ping.exe.old`, or
/// `Ping.exe.2.old` while an older one is still held.
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

/// The archive's program files over Ping's in `dir`, all or none. FFmpeg's
/// DLLs of an older version (another number) are renamed aside too. The
/// archive's installer is not copied: [`register`] puts it where Ping is
/// installed, and a copy elsewhere (Downloads) needs none.
fn replace_files(unpacked: &Path, dir: &Path) -> Result<(), String> {
    let new: Vec<PathBuf> = std::fs::read_dir(unpacked)
        .map_err(|e| e.to_string())?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.is_file() && p.file_name().is_some_and(|n| ours(&n.to_string_lossy())))
        .collect();
    let names: Vec<String> = new
        .iter()
        .filter_map(|p| {
            p.file_name()
                .map(|n| n.to_string_lossy().to_ascii_lowercase())
        })
        .collect();
    if !names.iter().any(|n| n == "ping.exe") {
        return Err("the archive has no Ping.exe".into());
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
        for file in &new {
            let name = file.file_name().ok_or("a file without a name")?;
            let target = dir.join(name);
            if target.exists() {
                let to = aside(&target);
                std::fs::rename(&target, &to).map_err(|e| format!("{}: {e}", target.display()))?;
                moved.push((target.clone(), to));
            }
            std::fs::copy(file, &target).map_err(|e| format!("{}: {e}", target.display()))?;
            written.push(target);
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

/// Bring Ping's entry in Windows' list of installed apps (Settings > Apps)
/// up to date with the `install.ps1` in `scripts` (the archive's, or the
/// installed one), run with `-Register FOLDER`: it does so only when
/// `folder` is where it installs Ping. The files are in place already: an
/// entry that stays behind is no reason to undo them.
fn register(scripts: &Path, folder: &Path) {
    let script = scripts.join("install.ps1");
    if !script.is_file() {
        return;
    }
    let out = Command::new(powershell())
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-File",
        ])
        .arg(&script)
        .arg("-Register")
        .arg(folder)
        .creation_flags(CREATE_NO_WINDOW)
        .output();
    match out {
        Ok(out) if out.status.success() => {}
        Ok(out) => tracing::warn!(
            error = %String::from_utf8_lossy(&out.stderr).trim(),
            "Ping's entry in Settings > Apps was not updated"
        ),
        Err(e) => tracing::warn!(error = %e, "Ping's entry in Settings > Apps was not updated"),
    }
}

/// Where `install.ps1` (tools/install-ping.ps1) installs Ping.
fn ping_install_folder() -> Option<PathBuf> {
    std::env::var_os("LOCALAPPDATA").map(|dir| PathBuf::from(dir).join(r"Programs\Ping"))
}

/// Ping's entry in Settings > Apps, under HKEY_CURRENT_USER, as
/// `install.ps1` names it.
const PING_ENTRY: &str = r"Software\Microsoft\Windows\CurrentVersion\Uninstall\dev.pingpong.Ping";

/// Whether two paths name the same folder, as Windows compares them:
/// without regard to case or a trailing separator.
fn same_folder(a: &Path, b: &Path) -> bool {
    let norm = |p: &Path| p.to_string_lossy().trim_end_matches('\\').to_lowercase();
    norm(a) == norm(b)
}

/// The version Ping's entry in Settings > Apps names, if there is one.
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
            &HSTRING::from(PING_ENTRY),
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

/// At each start: a Ping that runs from the folder `install.ps1` installs
/// it into, with that script beside it, keeps its entry in Settings > Apps
/// naming its `version`. An update by a release older than the entry (0.9.4
/// and before) copies the new files and runs no installer, and someone may
/// have copied the files by hand: the script beside Ping writes the entry
/// then. Run on the update check's thread, not the window's.
pub(crate) fn keep_listed(version: &str) {
    let Some(dir) = std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(Path::to_path_buf))
    else {
        return;
    };
    let installed = ping_install_folder().is_some_and(|f| same_folder(&dir, &f));
    if !installed || !dir.join("install.ps1").is_file() {
        return;
    }
    if listed_version().as_deref() == Some(version) {
        return;
    }
    tracing::info!(version, "listing Ping in Settings > Apps");
    register(&dir, &dir);
}

/// `install::rehearse`: the app's files replaced in `folder` as in its
/// own, and its entry in Settings > Apps written if `folder` is where the
/// app is installed. The host is left alone.
pub(crate) fn rehearse(unpacked: &Path, folder: &Path) -> Result<(), String> {
    replace_files(unpacked, folder)?;
    register(unpacked, folder);
    Ok(())
}

/// Delete what an update renamed aside in Ping's folder, now that nothing
/// holds it.
pub(crate) fn tidy() {
    let Some(dir) = std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(Path::to_path_buf))
    else {
        return;
    };
    let Ok(entries) = std::fs::read_dir(&dir) else {
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

    /// PowerShell's own parser's complaints about the script at `path`.
    fn parse(path: &Path) -> Result<(), String> {
        let check = format!(
            "$errors = $null; [void][System.Management.Automation.Language.Parser]::ParseFile({}, [ref]$null, [ref]$errors); \
                if ($errors.Count) {{ $errors | ForEach-Object {{ $_.Message }}; exit 1 }}",
            ps_quote(&path.to_string_lossy())
        );
        let out = Command::new(powershell())
            .args(["-NoProfile", "-NonInteractive", "-Command", &check])
            .output()
            .map_err(|e| e.to_string())?;
        if out.status.success() {
            Ok(())
        } else {
            Err(String::from_utf8_lossy(&out.stdout).into_owned())
        }
    }

    #[test]
    fn the_release_installer_is_powershell_that_parses() {
        // What the archive carries as install.ps1 (tools/package-windows.ps1),
        // and an update runs.
        let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tools/install-ping.ps1");
        if let Err(errors) = parse(&script) {
            panic!("{errors}");
        }
    }

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
    fn the_archives_installer_hears_which_folder_was_updated() {
        let dir = tempfile::tempdir().unwrap();
        let unpacked = dir.path().join("unpacked");
        // A space and an apostrophe, as a user's name can have.
        let folder = dir.path().join(r"Programs\o'brien's Ping");
        std::fs::create_dir_all(&unpacked).unwrap();
        std::fs::create_dir_all(&folder).unwrap();
        // A stand-in for install.ps1 that writes down what it was asked.
        std::fs::write(
            unpacked.join("install.ps1"),
            "param([string]$Register)\r\n\
             Set-Content -LiteralPath (Join-Path $PSScriptRoot 'asked.txt') -Value $Register\r\n",
        )
        .unwrap();
        register(&unpacked, &folder);
        let asked = std::fs::read_to_string(unpacked.join("asked.txt")).unwrap();
        assert_eq!(asked.trim(), folder.to_string_lossy());
    }

    #[test]
    fn a_folder_is_the_same_whatever_its_case_or_a_trailing_separator() {
        let installed = Path::new(r"C:\Users\o'brien\AppData\Local\Programs\Ping");
        assert!(same_folder(
            Path::new(r"c:\users\O'BRIEN\appdata\local\programs\ping\"),
            installed
        ));
        assert!(!same_folder(
            Path::new(r"C:\Users\o'brien\Downloads\Ping-0.9.5-windows-x86_64"),
            installed
        ));
    }

    #[test]
    fn only_the_apps_own_files_are_renamed_or_deleted() {
        for name in [
            "Ping.exe",
            "pong.exe",
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
            "pong.exe.lnk",
            "ping.exe.lnk",
        ] {
            assert!(!ours(name), "{name}");
        }
    }
}
