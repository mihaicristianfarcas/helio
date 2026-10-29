//! What Helio Host keeps on disk, for the host and for whoever manages it
//! from outside (`helio host`, helio-cli): where its data folder is, and
//! kept private on Windows (`private`), its identity, its paired clients
//! and what each may do (`clients`).
//!
//! A crate of its own, not a library of `helio-host`: what links it (the
//! `helio` command, the app) must not take in the host's dependencies, some
//! of which cannot share a Linux build with GPUI's (docs/development.md).

use std::path::{Path, PathBuf};

pub mod clients;
pub mod private;

/// `%ProgramData%\Helio Host` on Windows (shared by the service and the host it
/// launches), `~/Library/Application Support/Helio Host` on a Mac,
/// `$XDG_CONFIG_HOME/helio-host` (`~/.config/helio-host`) on Linux. `HELIO_HOST_DATA_DIR`
/// overrides them.
pub fn data_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("HELIO_HOST_DATA_DIR") {
        return PathBuf::from(dir);
    }
    #[cfg(windows)]
    {
        let base = std::env::var_os("ProgramData")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(r"C:\ProgramData"));
        base.join("Helio Host")
    }
    #[cfg(target_os = "macos")]
    {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        home.join("Library/Application Support/Helio Host")
    }
    // Beside Helio's: $XDG_CONFIG_HOME/helio-host.
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        let env = |k: &str| {
            std::env::var_os(k)
                .filter(|v| !v.is_empty())
                .map(PathBuf::from)
        };
        let home = env("HOME").unwrap_or_else(std::env::temp_dir);
        env("XDG_CONFIG_HOME")
            .unwrap_or_else(|| home.join(".config"))
            .join("helio-host")
    }
}

/// The host's identity (its tunnel keys) in the data folder `dir`.
pub fn identity_path(dir: &Path) -> PathBuf {
    dir.join("identity.toml")
}
