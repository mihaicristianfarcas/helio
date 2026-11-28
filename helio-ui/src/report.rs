//! A bug report from inside the app: GitHub's bug form
//! (`.github/ISSUE_TEMPLATE/bug.yml`) opened in the browser with what the
//! app knows already written in, its version and this computer's system.
//! GitHub's issue forms take a field's `id` as a query parameter for that
//! (docs.github.com, "Syntax for GitHub's form schema", the `id` key), so
//! nothing is sent from here: the person reads the form, fills in the rest
//! and submits it, or closes the tab. **Help → Report a Bug…** and
//! **Settings → General** open it.
//!
//! The system is written as the form's own examples write it ("macOS 26.1",
//! "Windows 11 25H2"), with Windows' build after it; on Linux, the
//! distribution and the desktop, which the form asks for. What cannot be
//! read is left for the person to fill in.

use std::fmt::Write as _;

/// The bug form of the project at `repository` (its GitHub address), with
/// `version` in its version field and `system`, when known, in the field for
/// the computer you sit at.
pub fn bug_form(repository: &str, version: &str, system: Option<&str>) -> String {
    let mut url = format!(
        "{repository}/issues/new?template=bug.yml&helio-version={}",
        query(version)
    );
    if let Some(system) = system {
        url.push_str("&client-os=");
        url.push_str(&query(system));
    }
    url
}

/// `text` as a query value: RFC 3986's unreserved characters as they are,
/// every other byte of its UTF-8 as `%XX`.
fn query(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for b in text.bytes() {
        if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
            out.push(b as char);
        } else {
            let _ = write!(out, "%{b:02X}");
        }
    }
    out
}

/// This computer's system, as [`bug_form`] writes it; `None` when it cannot
/// be read.
pub fn this_system() -> Option<String> {
    #[cfg(target_os = "macos")]
    {
        // NSProcessInfo says the version macOS itself shows, whatever the
        // SDK the app was built with (`sw_vers` asks the same).
        let v = objc2_foundation::NSProcessInfo::processInfo().operatingSystemVersion();
        Some(macos_name(v.majorVersion, v.minorVersion, v.patchVersion))
    }
    #[cfg(windows)]
    {
        let build = crate::glass::windows_build();
        (build != 0).then(|| windows_name(build, windows_display_version().as_deref()))
    }
    #[cfg(target_os = "linux")]
    {
        let release = std::fs::read_to_string("/etc/os-release")
            .or_else(|_| std::fs::read_to_string("/usr/lib/os-release"))
            .ok()?;
        let desktop = std::env::var("XDG_CURRENT_DESKTOP").ok();
        let session = std::env::var("XDG_SESSION_TYPE").ok();
        Some(linux_name(
            &os_release_name(&release)?,
            desktop.as_deref(),
            session.as_deref(),
        ))
    }
    #[cfg(not(any(target_os = "macos", windows, target_os = "linux")))]
    None
}

/// "macOS 26.1", or "macOS 26.1.2" for a point release.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn macos_name(major: isize, minor: isize, patch: isize) -> String {
    match patch {
        0 => format!("macOS {major}.{minor}"),
        _ => format!("macOS {major}.{minor}.{patch}"),
    }
}

/// "Windows 11 25H2 (build 26200)": build 22000 is the first of Windows 11.
/// The release ("25H2") is what Settings → System → About shows, when the
/// registry has it.
#[cfg_attr(not(windows), allow(dead_code))]
fn windows_name(build: u32, release: Option<&str>) -> String {
    let name = if build >= 22000 {
        "Windows 11"
    } else {
        "Windows 10"
    };
    match release {
        Some(release) => format!("{name} {release} (build {build})"),
        None => format!("{name} (build {build})"),
    }
}

/// `DisplayVersion` under `HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion`
/// ("25H2"), which Windows 10 20H2 and later keep.
#[cfg(windows)]
fn windows_display_version() -> Option<String> {
    use windows::core::w;
    use windows::Win32::System::Registry::{RegGetValueW, HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ};

    // "25H2" and its NUL fit many times over; a longer value fails the call
    // with ERROR_MORE_DATA, and the field is left to the person.
    let mut text = [0u16; 32];
    let mut bytes = std::mem::size_of_val(&text) as u32;
    // SAFETY: `text` is writable for the `bytes` said; RegGetValueW writes
    // at most that, ends the string with a NUL, and says how many bytes it
    // wrote.
    let status = unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            w!("SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion"),
            w!("DisplayVersion"),
            RRF_RT_REG_SZ,
            None,
            Some(text.as_mut_ptr().cast()),
            Some(&mut bytes),
        )
    };
    status.ok().ok()?;
    let len = (bytes as usize / 2).saturating_sub(1).min(text.len());
    let release = String::from_utf16_lossy(&text[..len]);
    (!release.is_empty()).then_some(release)
}

/// The distribution's name from `os-release` (freedesktop.org's
/// specification): `PRETTY_NAME`, else `NAME` and `VERSION_ID`.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn os_release_name(text: &str) -> Option<String> {
    let field = |key: &str| {
        text.lines().find_map(|line| {
            let value = line.trim().strip_prefix(key)?.strip_prefix('=')?;
            let value = value
                .strip_prefix('"')
                .and_then(|v| v.strip_suffix('"'))
                .or_else(|| value.strip_prefix('\'').and_then(|v| v.strip_suffix('\'')))
                .unwrap_or(value);
            (!value.is_empty()).then(|| value.to_string())
        })
    };
    field("PRETTY_NAME").or_else(|| {
        let name = field("NAME")?;
        Some(match field("VERSION_ID") {
            Some(version) => format!("{name} {version}"),
            None => name,
        })
    })
}

/// "Fedora Linux 43 (KDE Plasma Desktop Edition), KDE on Wayland": the
/// desktop is `XDG_CURRENT_DESKTOP`'s last name ("ubuntu:GNOME" is GNOME's),
/// the session `XDG_SESSION_TYPE`.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn linux_name(distribution: &str, desktop: Option<&str>, session: Option<&str>) -> String {
    let desktop = desktop
        .and_then(|d| d.rsplit(':').next())
        .filter(|d| !d.is_empty());
    let session = session.map(|s| match s {
        "wayland" => "Wayland",
        "x11" => "X11",
        other => other,
    });
    match (desktop, session) {
        (Some(d), Some(s)) => format!("{distribution}, {d} on {s}"),
        (Some(d), None) => format!("{distribution}, {d}"),
        (None, Some(s)) => format!("{distribution}, {s}"),
        (None, None) => distribution.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const REPO: &str = "https://github.com/mihaicristianfarcas/helio";

    #[test]
    fn the_form_opens_with_the_version_and_the_system_in_their_fields() {
        assert_eq!(
            bug_form(REPO, "0.10.0", Some("macOS 26.1")),
            "https://github.com/mihaicristianfarcas/helio/issues/new?template=bug.yml\
                &helio-version=0.10.0&client-os=macOS%2026.1"
        );
    }

    #[test]
    fn a_system_that_cannot_be_read_leaves_its_field_to_the_person() {
        assert_eq!(
            bug_form(REPO, "0.10.0", None),
            "https://github.com/mihaicristianfarcas/helio/issues/new?template=bug.yml\
                &helio-version=0.10.0"
        );
    }

    #[test]
    fn what_would_end_a_query_value_is_escaped() {
        let url = bug_form(
            REPO,
            "0.10.0 (abc1234)",
            Some("Fedora & co, KDE on Wayland #1"),
        );
        assert!(url.ends_with(
            "&helio-version=0.10.0%20%28abc1234%29\
                &client-os=Fedora%20%26%20co%2C%20KDE%20on%20Wayland%20%231"
        ));
        assert_eq!(query("Ünïcode"), "%C3%9Cn%C3%AFcode");
    }

    /// The platform half, on whichever system runs the tests (CI runs all
    /// three): a Mac and Windows name themselves, Linux reads os-release.
    #[test]
    fn this_computer_names_its_system() {
        let system = this_system().expect("this system's name and version are readable");
        if cfg!(target_os = "macos") {
            assert!(system.starts_with("macOS "), "{system}");
        }
        if cfg!(windows) {
            assert!(system.starts_with("Windows 1"), "{system}");
        }
    }

    #[test]
    fn macos_names_a_point_release_only_when_there_is_one() {
        assert_eq!(macos_name(26, 1, 0), "macOS 26.1");
        assert_eq!(macos_name(26, 1, 2), "macOS 26.1.2");
    }

    #[test]
    fn windows_11_starts_at_build_22000() {
        assert_eq!(
            windows_name(26200, Some("25H2")),
            "Windows 11 25H2 (build 26200)"
        );
        assert_eq!(windows_name(19045, None), "Windows 10 (build 19045)");
    }

    #[test]
    fn os_release_gives_its_pretty_name_without_quotes() {
        let fedora = "NAME=\"Fedora Linux\"\nVERSION_ID=43\n\
            PRETTY_NAME=\"Fedora Linux 43 (KDE Plasma Desktop Edition)\"\n";
        assert_eq!(
            os_release_name(fedora).as_deref(),
            Some("Fedora Linux 43 (KDE Plasma Desktop Edition)")
        );
        let bare = "NAME='Arch Linux'\nID=arch\n";
        assert_eq!(os_release_name(bare).as_deref(), Some("Arch Linux"));
        let versioned = "NAME=Debian\nVERSION_ID=\"13\"\n";
        assert_eq!(os_release_name(versioned).as_deref(), Some("Debian 13"));
        assert_eq!(os_release_name("ID=unknown\n"), None);
    }

    #[test]
    fn linux_names_the_desktop_and_the_session_the_form_asks_for() {
        assert_eq!(
            linux_name("Ubuntu 24.04.3 LTS", Some("ubuntu:GNOME"), Some("wayland")),
            "Ubuntu 24.04.3 LTS, GNOME on Wayland"
        );
        assert_eq!(
            linux_name("Arch Linux", Some("Hyprland"), None),
            "Arch Linux, Hyprland"
        );
        assert_eq!(linux_name("Debian 13", None, Some("x11")), "Debian 13, X11");
        assert_eq!(linux_name("Debian 13", Some(""), None), "Debian 13");
    }
}
