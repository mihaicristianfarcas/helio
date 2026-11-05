//! `helio` in terminals: Settings > General > Command line puts the command
//! on the PATH, and takes it off again.
//!
//! The command comes inside the app (its program is `helio` when started
//! under that name: main.rs's `command_line`), and no installer puts it on
//! the PATH: whoever wants it does, here. Each system's way is the one that
//! takes the command away when the app goes, however it is removed:
//!
//! - macOS: the app's `Contents/Resources/bin` (where `helio` is a link to
//!   the app's program: tools/build-helio-app) named in `/etc/paths.d/helio`,
//!   which `path_helper` adds to every login shell's PATH, as Postgres.app
//!   does. A link in `/usr/local/bin` would outlive the app: one dragged to
//!   the Trash runs nothing, and Homebrew's uninstall steps run at every
//!   upgrade too (tools/update-casks). This way `helio` goes with the app,
//!   and what stays is a line naming a folder that is no more, which a
//!   Helio installed again names again. `/etc/paths.d` is root's, so macOS
//!   asks for an administrator (`do shell script … with administrator
//!   privileges`, the system's own dialog). The app must be where it stays:
//!   not on a disk image, nor run from where it was downloaded (App
//!   Translocation runs that from a folder of its own).
//! - Windows: the app's `bin` folder (`bin\helio.exe`, helio-app's
//!   `helio-console`) on the user's PATH: `HKCU\Environment`, written as
//!   the `REG_EXPAND_SZ` Windows keeps it as, and the change announced, so
//!   that what Explorer starts from then on has it. No administrator.
//!   Helio's uninstaller takes it off (tools/windows/helio.iss), and the
//!   folder goes with Helio's.
//! - Linux: a link, `~/.local/bin/helio`, to the app's program, in the
//!   user's own folder for programs, which most distributions put on a
//!   terminal's PATH. install.sh's uninstall removes it
//!   (tools/linux/install-release); a `helio` there that is not Helio's is
//!   left as it is, here and there.
//!
//! Terminals already open keep the PATH they started with, everywhere.

use std::path::{Path, PathBuf};

pub use platform::{install, remove, state, HOW};

/// Where `helio` stands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum State {
    /// On the PATH, as this copy of Helio's.
    On,
    /// Not on the PATH.
    Off,
    /// On the PATH as another copy of Helio's (moved, or gone), there:
    /// installing makes it this copy's.
    Elsewhere(PathBuf),
    /// Linux: a program of someone else's has the link's place, there.
    Taken(PathBuf),
    /// This copy cannot be the command from where it runs: why.
    Unavailable(String),
}

impl State {
    /// HELIO_UI_DEMO's `command=on|off|elsewhere|taken|unavailable`: a state
    /// made up, to see the row in it.
    pub fn demo(name: &str) -> Option<State> {
        let elsewhere = if cfg!(windows) {
            r"C:\path\to\Helio\bin"
        } else {
            "/Volumes/Old Mac/Applications/Helio.app/Contents/Resources/bin"
        };
        Some(match name {
            "on" => State::On,
            "off" => State::Off,
            "elsewhere" => State::Elsewhere(elsewhere.into()),
            "taken" => State::Taken("/home/you/.local/bin/helio".into()),
            "unavailable" => State::Unavailable(
                "Helio runs from where it was downloaded (macOS runs it from a folder of its \
                    own): move Helio to Applications and open it from there first."
                    .into(),
            ),
            _ => return None,
        })
    }
}

/// What a change came to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Done {
    Changed,
    /// macOS: no administrator's password was given; nothing changed.
    /// Elsewhere no one is asked.
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    Declined,
}

/// macOS: why the program at `exe` is not somewhere a command can lead to
/// for good, if it is not: a disk image or another disk (`/Volumes`), or
/// App Translocation's folder (a downloaded app run where it was
/// downloaded).
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn unsettled(exe: &Path) -> Option<&'static str> {
    if exe.to_string_lossy().contains("/AppTranslocation/") {
        Some(
            "Helio runs from where it was downloaded (macOS runs it from a folder of its own): \
                move Helio to Applications and open it from there first.",
        )
    } else if exe.starts_with("/Volumes") {
        Some(
            "Helio runs from a disk image or another disk: copy it to Applications and open it \
                from there first.",
        )
    } else {
        None
    }
}

/// macOS: what `/etc/paths.d/helio` holding `text` (`None`: there is none)
/// says of `bin`, this copy's folder for the command.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn named_in(text: Option<&str>, bin: &Path) -> State {
    match text.and_then(|t| t.lines().map(str::trim).find(|l| !l.is_empty())) {
        None => State::Off,
        Some(named) if Path::new(named) == bin => State::On,
        Some(named) => State::Elsewhere(named.into()),
    }
}

/// Windows: `%NAME%` in `entry` replaced by `var(NAME)`, as Windows
/// expands a `REG_EXPAND_SZ`: a name it does not know stays as written.
#[cfg_attr(not(windows), allow(dead_code))]
fn expand(entry: &str, var: impl Fn(&str) -> Option<String>) -> String {
    let mut out = String::new();
    let mut rest = entry;
    while let Some(start) = rest.find('%') {
        let after = &rest[start + 1..];
        let Some(end) = after.find('%') else {
            break;
        };
        out.push_str(&rest[..start]);
        match Some(&after[..end]).filter(|n| !n.is_empty()).and_then(&var) {
            Some(value) => {
                out.push_str(&value);
                rest = &after[end + 1..];
            }
            // Its closing `%` may open the next name.
            None => {
                out.push('%');
                out.push_str(&after[..end]);
                rest = &after[end..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// Windows: two folders are one when they differ only in case or a
/// trailing separator, as Windows compares them.
#[cfg_attr(not(windows), allow(dead_code))]
fn same_folder(a: &str, b: &str) -> bool {
    a.trim_end_matches('\\').to_lowercase() == b.trim_end_matches('\\').to_lowercase()
}

/// Windows: whether the user's PATH, as kept (`path`), has `bin` on it,
/// its entries expanded by `expand`.
#[cfg_attr(not(windows), allow(dead_code))]
fn has(path: &str, bin: &str, expand: impl Fn(&str) -> String) -> bool {
    path.split(';').any(|e| same_folder(&expand(e), bin))
}

/// Windows: the user's PATH, as kept (`path`), with `bin` at its end
/// (`on`) or nowhere on it; empty entries dropped. Empty: none left.
#[cfg_attr(not(windows), allow(dead_code))]
fn path_with(path: &str, bin: &str, on: bool, expand: impl Fn(&str) -> String) -> String {
    let mut entries: Vec<&str> = path
        .split(';')
        .filter(|e| !e.is_empty() && !same_folder(&expand(e), bin))
        .collect();
    if on {
        entries.push(bin);
    }
    entries.join(";")
}

/// Linux: what is at the link's place.
#[cfg_attr(any(target_os = "macos", windows), allow(dead_code))]
#[derive(Debug)]
enum Found {
    Nothing,
    /// A link, to `to` (followed to the end when it leads somewhere).
    Link {
        to: PathBuf,
        exists: bool,
    },
    /// Not a link: a program, or anything else.
    Other,
}

/// Linux: what `found` at `at` is, for this copy's program at `program`. A
/// link to a program of the same name (another copy of Helio), or to
/// nothing, is Helio's to point here; anything else is someone else's.
#[cfg_attr(any(target_os = "macos", windows), allow(dead_code))]
fn judge(found: Found, at: &Path, program: &Path) -> State {
    match found {
        Found::Nothing => State::Off,
        Found::Link { to, .. } if to == program => State::On,
        Found::Link { to, exists } if !exists || to.file_name() == program.file_name() => {
            State::Elsewhere(to)
        }
        Found::Link { .. } | Found::Other => State::Taken(at.to_path_buf()),
    }
}

#[cfg(target_os = "macos")]
mod platform {
    use std::path::{Path, PathBuf};
    use std::process::Command;

    use super::{Done, State};

    /// How Install does it, for the row's line.
    pub const HOW: &str = "macOS asks for an administrator.";

    const PATHS_D: &str = "/etc/paths.d/helio";

    /// The folder `/etc/paths.d/helio` names: `$1` (`quoted form of`), never
    /// spliced into the script's text.
    const INSTALL: [&str; 3] = [
        "on run argv",
        "do shell script \"/bin/mkdir -p /etc/paths.d && /bin/echo \" & quoted form of (item 2 \
            of argv) & \" > /etc/paths.d/helio\" with prompt (item 1 of argv) with administrator \
            privileges",
        "end run",
    ];
    const REMOVE: [&str; 3] = [
        "on run argv",
        "do shell script \"/bin/rm -f /etc/paths.d/helio\" with prompt (item 1 of argv) with \
            administrator privileges",
        "end run",
    ];

    /// This copy's folder for the command, `Contents/Resources/bin`, if it
    /// can be the command from where it runs.
    fn bin() -> Result<PathBuf, String> {
        let exe = std::env::current_exe()
            .and_then(|e| e.canonicalize())
            .map_err(|e| e.to_string())?;
        if let Some(why) = super::unsettled(&exe) {
            return Err(why.into());
        }
        let bin = exe
            .parent()
            .and_then(Path::parent)
            .map(|contents| contents.join("Resources/bin"))
            .filter(|bin| bin.join("helio").exists());
        bin.ok_or_else(|| {
            "This copy of Helio has no command inside it (Contents/Resources/bin/helio): it is \
                a build of its own, and tools/build-helio-app makes one with it."
                .into()
        })
    }

    pub fn state() -> State {
        match bin() {
            Ok(bin) => super::named_in(std::fs::read_to_string(PATHS_D).ok().as_deref(), &bin),
            Err(why) => State::Unavailable(why),
        }
    }

    pub fn install() -> Result<Done, String> {
        let bin = bin()?;
        as_administrator(
            &INSTALL,
            &[
                "Helio is putting the helio command on the PATH.",
                &bin.to_string_lossy(),
            ],
        )
    }

    pub fn remove() -> Result<Done, String> {
        as_administrator(
            &REMOVE,
            &["Helio is taking the helio command off the PATH."],
        )
    }

    /// Run `script` (AppleScript, with `args` as its `argv`), which asks
    /// macOS for an administrator. Waits for the person at the dialog.
    fn as_administrator(script: &[&str], args: &[&str]) -> Result<Done, String> {
        let mut command = Command::new("/usr/bin/osascript");
        for line in script {
            command.arg("-e").arg(line);
        }
        let out = command
            .args(args)
            .output()
            .map_err(|e| format!("osascript: {e}"))?;
        let said = String::from_utf8_lossy(&out.stderr);
        if out.status.success() {
            Ok(Done::Changed)
        } else if said.contains("(-128)") {
            // userCanceledErr: Cancel in the dialog.
            Ok(Done::Declined)
        } else {
            Err(said.trim().to_string())
        }
    }
}

#[cfg(windows)]
mod platform {
    use windows::core::w;
    use windows::Win32::Foundation::{ERROR_FILE_NOT_FOUND, LPARAM, WPARAM};
    use windows::Win32::System::Registry::{
        RegDeleteKeyValueW, RegGetValueW, RegSetKeyValueW, HKEY_CURRENT_USER, REG_EXPAND_SZ,
        RRF_NOEXPAND, RRF_RT_REG_EXPAND_SZ, RRF_RT_REG_SZ,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        SendMessageTimeoutW, HWND_BROADCAST, SMTO_ABORTIFHUNG, WM_SETTINGCHANGE,
    };

    use super::{Done, State};

    /// How Install does it, for the row's line.
    pub const HOW: &str = "It adds Helio's bin folder to your PATH.";

    /// This copy's folder for the command, `bin` beside Helio.exe, if it
    /// has the command.
    fn bin() -> Result<String, String> {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let bin = exe.parent().map(|dir| dir.join("bin"));
        bin.filter(|bin| bin.join("helio.exe").is_file())
            .map(|bin| bin.to_string_lossy().into_owned())
            .ok_or_else(|| {
                "This copy of Helio has no bin\\helio.exe beside it: it is a build of its own, \
                    and tools\\build-helio-win.ps1 makes one with it."
                    .into()
            })
    }

    fn expanded(entry: &str) -> String {
        super::expand(entry, |name| std::env::var(name).ok())
    }

    pub fn state() -> State {
        match bin().and_then(|bin| Ok((user_path()?, bin))) {
            Ok((path, bin)) if super::has(&path, &bin, expanded) => State::On,
            Ok(_) => State::Off,
            Err(why) => State::Unavailable(why),
        }
    }

    pub fn install() -> Result<Done, String> {
        let bin = bin()?;
        set_user_path(&super::path_with(&user_path()?, &bin, true, expanded))?;
        Ok(Done::Changed)
    }

    pub fn remove() -> Result<Done, String> {
        let bin = bin()?;
        set_user_path(&super::path_with(&user_path()?, &bin, false, expanded))?;
        Ok(Done::Changed)
    }

    /// The user's PATH as kept, its `%NAME%`s unexpanded; empty when there
    /// is none.
    fn user_path() -> Result<String, String> {
        let flags = RRF_RT_REG_SZ | RRF_RT_REG_EXPAND_SZ | RRF_NOEXPAND;
        let mut bytes = 0u32;
        // SAFETY: a size query: no buffer, only `bytes` written.
        let status = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                w!("Environment"),
                w!("Path"),
                flags,
                None,
                None,
                Some(&mut bytes),
            )
        };
        if status == ERROR_FILE_NOT_FOUND {
            return Ok(String::new());
        }
        status.ok().map_err(|e| format!("your PATH: {e}"))?;
        let mut text = vec![0u16; bytes as usize / 2 + 1];
        let mut bytes = (text.len() * 2) as u32;
        // SAFETY: `text` is writable for the `bytes` said; RegGetValueW
        // writes at most that, ends the string with a NUL, and says how many
        // bytes it wrote.
        let status = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                w!("Environment"),
                w!("Path"),
                flags,
                None,
                Some(text.as_mut_ptr().cast()),
                Some(&mut bytes),
            )
        };
        status.ok().map_err(|e| format!("your PATH: {e}"))?;
        let len = (bytes as usize / 2).saturating_sub(1).min(text.len());
        Ok(String::from_utf16_lossy(&text[..len]))
    }

    /// Write the user's PATH (none, when `path` is empty), and say so to
    /// the windows that keep an environment for what they start (Explorer).
    fn set_user_path(path: &str) -> Result<(), String> {
        let status = if path.is_empty() {
            // SAFETY: constant, NUL-ended names.
            unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, w!("Environment"), w!("Path")) }
        } else {
            let wide: Vec<u16> = path.encode_utf16().chain([0]).collect();
            // SAFETY: `wide` is NUL-ended, lives across the call, and is
            // `cbdata` bytes long, its NUL included, as REG_EXPAND_SZ wants.
            unsafe {
                RegSetKeyValueW(
                    HKEY_CURRENT_USER,
                    w!("Environment"),
                    w!("Path"),
                    REG_EXPAND_SZ.0,
                    Some(wide.as_ptr().cast()),
                    (wide.len() * 2) as u32,
                )
            }
        };
        status.ok().map_err(|e| format!("your PATH: {e}"))?;
        // SAFETY: the message's string is a constant; a window that does not
        // answer is skipped (SMTO_ABORTIFHUNG), each given 5 s at most.
        unsafe {
            SendMessageTimeoutW(
                HWND_BROADCAST,
                WM_SETTINGCHANGE,
                WPARAM(0),
                LPARAM(w!("Environment").as_ptr() as isize),
                SMTO_ABORTIFHUNG,
                5000,
                None,
            )
        };
        Ok(())
    }
}

#[cfg(not(any(target_os = "macos", windows)))]
mod platform {
    use std::path::PathBuf;

    use super::{Done, Found, State};

    /// How Install does it, for the row's line.
    pub const HOW: &str = "It links it in ~/.local/bin, which most distributions put on the PATH.";

    /// Where the link goes: `~/.local/bin/helio`.
    fn link() -> Result<PathBuf, String> {
        std::env::var_os("HOME")
            .map(|home| PathBuf::from(home).join(".local/bin/helio"))
            .ok_or_else(|| "HOME is not set".into())
    }

    /// This copy's program, where the link leads.
    fn program() -> Result<PathBuf, String> {
        std::env::current_exe()
            .and_then(|e| e.canonicalize())
            .map_err(|e| e.to_string())
    }

    fn found(link: &std::path::Path) -> Found {
        match std::fs::symlink_metadata(link) {
            Err(_) => Found::Nothing,
            Ok(meta) if meta.file_type().is_symlink() => {
                let to = std::fs::read_link(link).unwrap_or_default();
                let to = link.parent().map(|dir| dir.join(&to)).unwrap_or(to);
                match to.canonicalize() {
                    Ok(to) => Found::Link { to, exists: true },
                    Err(_) => Found::Link { to, exists: false },
                }
            }
            Ok(_) => Found::Other,
        }
    }

    pub fn state() -> State {
        match link().and_then(|link| Ok((program()?, link))) {
            Ok((program, link)) => super::judge(found(&link), &link, &program),
            Err(why) => State::Unavailable(why),
        }
    }

    pub fn install() -> Result<Done, String> {
        let (link, program) = (link()?, program()?);
        if let State::Taken(at) = super::judge(found(&link), &link, &program) {
            return Err(format!(
                "{} is another program: it was left as it is",
                at.display()
            ));
        }
        let dir = link.parent().ok_or("no folder for the link")?;
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        // Made aside, then renamed over the old link: never a moment
        // without one.
        let new = dir.join(format!(".helio.{}", std::process::id()));
        let _ = std::fs::remove_file(&new);
        std::os::unix::fs::symlink(&program, &new)
            .and_then(|()| std::fs::rename(&new, &link))
            .map_err(|e| format!("{}: {e}", link.display()))?;
        Ok(Done::Changed)
    }

    pub fn remove() -> Result<Done, String> {
        let (link, program) = (link()?, program()?);
        match super::judge(found(&link), &link, &program) {
            State::On | State::Elsewhere(_) => {
                std::fs::remove_file(&link).map_err(|e| format!("{}: {e}", link.display()))?;
            }
            _ => {}
        }
        Ok(Done::Changed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env(name: &str) -> Option<String> {
        match name {
            "USERPROFILE" => Some(r"C:\Users\o".into()),
            "LOCALAPPDATA" => Some(r"C:\Users\o\AppData\Local".into()),
            _ => None,
        }
    }

    #[test]
    fn a_windows_path_entry_is_expanded_as_windows_does() {
        assert_eq!(
            expand(r"%USERPROFILE%\AppData\Local\Microsoft\WindowsApps", env),
            r"C:\Users\o\AppData\Local\Microsoft\WindowsApps"
        );
        assert_eq!(expand(r"%NOPE%\bin", env), r"%NOPE%\bin");
        assert_eq!(expand(r"%NOPE%%USERPROFILE%", env), r"%NOPE%C:\Users\o");
        assert_eq!(expand(r"50%\bin", env), r"50%\bin");
        assert_eq!(expand(r"C:\Tools", env), r"C:\Tools");
    }

    #[test]
    fn the_command_goes_on_the_end_of_the_path_once_and_comes_off_whatever_its_spelling() {
        let bin = r"C:\Users\o\AppData\Local\Programs\Helio\bin";
        let ex = |e: &str| expand(e, env);
        let kept = r"%USERPROFILE%\AppData\Local\Microsoft\WindowsApps;C:\Tools";
        let on = path_with(kept, bin, true, ex);
        assert_eq!(on, format!("{kept};{bin}"));
        assert!(has(&on, bin, ex));
        // Once, however often it is asked for.
        assert_eq!(path_with(&on, bin, true, ex), on);
        // Off, as written by hand: another case, a trailing separator, a
        // variable.
        for written in [
            r"c:\users\o\appdata\local\programs\helio\bin\",
            r"%LOCALAPPDATA%\Programs\Helio\bin",
        ] {
            let path = format!("{kept};{written};;");
            assert!(has(&path, bin, ex));
            assert_eq!(path_with(&path, bin, false, ex), kept);
        }
        // The only entry: none left.
        assert_eq!(path_with(bin, bin, false, ex), "");
        assert!(!has(kept, bin, ex));
    }

    #[test]
    fn paths_d_names_this_copy_another_or_none() {
        let bin = Path::new("/Applications/Helio.app/Contents/Resources/bin");
        assert_eq!(named_in(None, bin), State::Off);
        assert_eq!(named_in(Some("\n"), bin), State::Off);
        assert_eq!(
            named_in(
                Some("/Applications/Helio.app/Contents/Resources/bin\n"),
                bin
            ),
            State::On
        );
        assert_eq!(
            named_in(
                Some("/Users/o/Desktop/Helio.app/Contents/Resources/bin\n"),
                bin
            ),
            State::Elsewhere("/Users/o/Desktop/Helio.app/Contents/Resources/bin".into())
        );
    }

    #[test]
    fn a_mac_app_on_a_disk_image_or_translocated_cannot_be_the_command() {
        for exe in [
            "/Volumes/Helio/Helio.app/Contents/MacOS/Helio",
            "/private/var/folders/x/T/AppTranslocation/0A1B/d/Helio.app/Contents/MacOS/Helio",
        ] {
            assert!(unsettled(Path::new(exe)).is_some(), "{exe}");
        }
        for exe in [
            "/Applications/Helio.app/Contents/MacOS/Helio",
            "/Users/o/Applications/Helio.app/Contents/MacOS/Helio",
        ] {
            assert_eq!(unsettled(Path::new(exe)), None, "{exe}");
        }
    }

    #[test]
    fn a_link_is_helios_to_point_here_unless_it_leads_to_another_program() {
        let at = Path::new("/home/o/.local/bin/helio");
        let program = Path::new("/home/o/.local/bin/helio-app");
        assert_eq!(judge(Found::Nothing, at, program), State::Off);
        let link = |to: &str, exists| Found::Link {
            to: to.into(),
            exists,
        };
        assert_eq!(
            judge(link("/home/o/.local/bin/helio-app", true), at, program),
            State::On
        );
        // Another copy of Helio, or a link to nothing: Helio's to point here.
        assert_eq!(
            judge(link("/opt/old/helio-app", true), at, program),
            State::Elsewhere("/opt/old/helio-app".into())
        );
        assert_eq!(
            judge(link("/home/o/src/gone/helio", false), at, program),
            State::Elsewhere("/home/o/src/gone/helio".into())
        );
        // Someone else's.
        assert_eq!(
            judge(link("/usr/lib/other/helio", true), at, program),
            State::Taken(at.into())
        );
        assert_eq!(judge(Found::Other, at, program), State::Taken(at.into()));
    }

    #[test]
    fn every_demo_state_has_a_name() {
        for name in ["on", "off", "elsewhere", "taken", "unavailable"] {
            assert!(State::demo(name).is_some(), "{name}");
        }
        assert_eq!(State::demo("nope"), None);
    }
}
