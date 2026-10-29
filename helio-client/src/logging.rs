//! Logging for the app: to stderr and to a file the user can send, the
//! previous two launches' kept beside it.
//!
//!   macOS    ~/Library/Logs/Helio/helio.log
//!   Windows  %LOCALAPPDATA%\Helio\Logs\helio.log
//!   Linux    ~/.local/state/helio/helio.log

use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

pub const DEFAULT_FILTER: &str =
    "info,mainline=error,helio_client=info,helio_transport=info,helio_decode=info";

pub fn logs_dir() -> PathBuf {
    let env = |k: &str| {
        std::env::var_os(k)
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
    };
    if cfg!(windows) {
        return env("LOCALAPPDATA")
            .unwrap_or_else(std::env::temp_dir)
            .join("Helio")
            .join("Logs");
    }
    let home = env("HOME").unwrap_or_else(std::env::temp_dir);
    if cfg!(target_os = "macos") {
        home.join("Library/Logs/Helio")
    } else {
        env("XDG_STATE_HOME")
            .unwrap_or_else(|| home.join(".local/state"))
            .join("helio")
    }
}

/// Log to stderr and `logs_dir()/helio.log`. Once per process.
pub fn init() {
    static ONCE: OnceLock<()> = OnceLock::new();
    ONCE.get_or_init(|| {
        let logs = logs_dir();
        let _ = std::fs::create_dir_all(&logs);
        let filter = || {
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| DEFAULT_FILTER.into())
        };
        use tracing_subscriber::prelude::*;
        let _ = std::fs::rename(logs.join("helio.1.log"), logs.join("helio.2.log"));
        let _ = std::fs::rename(logs.join("helio.log"), logs.join("helio.1.log"));
        let file = std::fs::File::create(logs.join("helio.log")).ok();
        let registry = tracing_subscriber::registry().with(
            tracing_subscriber::fmt::layer()
                .with_writer(std::io::stderr)
                .with_filter(filter()),
        );
        match file {
            Some(f) => {
                let _ = registry
                    .with(
                        tracing_subscriber::fmt::layer()
                            .with_ansi(false)
                            .with_writer(Mutex::new(f))
                            .with_filter(filter()),
                    )
                    .try_init();
            }
            None => {
                let _ = registry.try_init();
            }
        }
        tracing::info!(
            version = env!("CARGO_PKG_VERSION"),
            os = std::env::consts::OS,
            "Helio started"
        );
    });
}
