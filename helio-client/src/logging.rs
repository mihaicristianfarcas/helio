//! Logging for the app: to stderr and to a file the user can send, with the
//! logs of the launches before it kept beside it.
//!
//!   macOS    ~/Library/Logs/Helio/helio.log
//!   Windows  %LOCALAPPDATA%\Helio\Logs\helio.log
//!   Linux    ~/.local/state/helio/helio.log
//!
//! `helio.log` is always this launch's. At each start the one before it is
//! set aside under the time that launch started (`helio-2026-11-01-144906.log`,
//! UTC, read from its first line, not from the file: Windows can hand a new
//! file the creation time of the one just renamed away from its name), so a
//! stream's log is not lost to the launches after it: the app opened again,
//! started at login, restarted by an update, or a check. Moonlight keeps its
//! last 10 launches' logs (moonlight-qt `app/main.cpp`); Helio keeps as many,
//! and every log from the last two weeks however many launches came after
//! ([`KEEP_LAUNCHES`], [`KEEP_FOR`], [`KEEP_AT_MOST`]).
//!
//! With `HELIO_DATA_DIR` set (a check, or a second copy of the app), the
//! logs go in its `logs` folder, never among the user's own.
//!
//! They name hosts and addresses, and are kept for weeks, so on macOS and
//! Linux only the user can enter their folder: a home folder others may
//! enter is common on Linux.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub const DEFAULT_FILTER: &str =
    "info,mainline=error,helio_client=info,helio_transport=info,helio_decode=info";

/// The logs of this many launches before this one are kept whatever their
/// age, as Moonlight keeps 10 (moonlight-qt `app/main.cpp`).
const KEEP_LAUNCHES: usize = 10;

/// Beyond those, a log is kept until it is this old (since its launch last
/// wrote to it): two weeks of streams, however often Helio started since.
const KEEP_FOR: Duration = Duration::from_secs(14 * 24 * 60 * 60);

/// Past this, all the logs set aside together, the oldest beyond the newest
/// [`KEEP_LAUNCHES`] go first, even if younger than [`KEEP_FOR`]: a bound on
/// the folder, should something start Helio over and over. A five-hour
/// session with debug logging wrote 9.4 MB (macOS), so this is about a
/// hundred such launches.
const KEEP_AT_MOST: u64 = 1 << 30;

/// This launch's log, in [`logs_dir`].
const CURRENT: &str = "helio.log";

pub fn logs_dir() -> PathBuf {
    let env = |k: &str| {
        std::env::var_os(k)
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
    };
    // A check's run (docs/ui.md) or another copy of the app: its logs
    // with its data, so starting it moves none of the user's own aside.
    if let Some(dir) = env("HELIO_DATA_DIR") {
        return dir.join("logs");
    }
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
        let private = helio_transport::identity::private_dir(&logs);
        let filter = || {
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| DEFAULT_FILTER.into())
        };
        use tracing_subscriber::prelude::*;
        set_aside(&logs);
        let removed = prune(&logs, SystemTime::now());
        let file = std::fs::File::create(logs.join(CURRENT)).ok();
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
        // The first line: the next launch names this log after its time.
        tracing::info!(
            version = env!("CARGO_PKG_VERSION"),
            os = std::env::consts::OS,
            "Helio started"
        );
        if removed > 0 {
            tracing::debug!(removed, "old launches' logs removed");
        }
        if let Err(e) = private {
            tracing::warn!(error = %e, "the logs' folder is not private");
        }
    });
}

/// Move the last launch's `helio.log` aside, under the time it started.
/// Another process may still be writing it (on Linux the app, while a
/// stream's process starts): it goes on writing to the same file under its
/// new name.
fn set_aside(logs: &Path) {
    let current = logs.join(CURRENT);
    let Ok(meta) = std::fs::metadata(&current) else {
        return;
    };
    let stamp = first_line(&current)
        .as_deref()
        .and_then(started_at)
        .or_else(|| meta.modified().ok().map(stamp_of))
        .unwrap_or_else(|| "unknown".into());
    // Two launches in one second (a check, a second copy refused): the
    // later one gets a number rather than replace the first.
    let mut name = format!("helio-{stamp}.log");
    let mut n = 1;
    while logs.join(&name).exists() {
        n += 1;
        name = format!("helio-{stamp}-{n}.log");
    }
    let _ = std::fs::rename(&current, logs.join(name));
}

/// The first line of `path`, from at most its first 256 bytes.
fn first_line(path: &Path) -> Option<String> {
    use std::io::Read;
    let mut head = [0u8; 256];
    let n = std::fs::File::open(path).ok()?.read(&mut head).ok()?;
    let text = String::from_utf8_lossy(&head[..n]);
    text.lines().next().map(str::to_owned)
}

/// `2026-11-01-144906` from a log line that starts with tracing's UTC time,
/// `2026-11-01T14:49:06.683558Z`; None for anything else.
fn started_at(line: &str) -> Option<String> {
    let b = line.as_bytes().get(..19)?;
    let digits = [0, 1, 2, 3, 5, 6, 8, 9, 11, 12, 14, 15, 17, 18];
    let shape = b[4] == b'-' && b[7] == b'-' && b[10] == b'T' && b[13] == b':' && b[16] == b':';
    if !shape || !digits.iter().all(|&i| b[i].is_ascii_digit()) {
        return None;
    }
    let s = &line[..19];
    Some(format!(
        "{}-{}{}{}",
        &s[..10],
        &s[11..13],
        &s[14..16],
        &s[17..19]
    ))
}

/// A time as [`started_at`] writes it, in UTC: for a log without a first
/// line to read it from.
fn stamp_of(t: SystemTime) -> String {
    let secs = t.duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let (days, rest) = (secs / 86_400, secs % 86_400);
    // Days since 1970-01-01 to a civil date (Howard Hinnant's
    // `civil_from_days`, for days on or after the epoch).
    let z = days + 719_468;
    let era = z / 146_097;
    let doe = z % 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + u64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}-{:02}{:02}{:02}",
        rest / 3600,
        rest / 60 % 60,
        rest % 60
    )
}

/// One launch's log, set aside.
struct SetAside {
    path: PathBuf,
    /// When its launch last wrote to it.
    modified: SystemTime,
    len: u64,
}

/// Remove the logs set aside that are no longer kept ([`to_remove`]).
/// Returns how many were removed.
fn prune(logs: &Path, now: SystemTime) -> usize {
    let Ok(entries) = std::fs::read_dir(logs) else {
        return 0;
    };
    let set_aside = entries
        .flatten()
        .filter(|e| {
            let name = e.file_name();
            let name = name.to_string_lossy();
            name != CURRENT && name.starts_with("helio") && name.ends_with(".log")
        })
        .filter_map(|e| {
            let meta = e.metadata().ok()?;
            meta.is_file().then(|| SetAside {
                path: e.path(),
                modified: meta.modified().unwrap_or(UNIX_EPOCH),
                len: meta.len(),
            })
        })
        .collect();
    to_remove(set_aside, now)
        .iter()
        .filter(|p| std::fs::remove_file(p).is_ok())
        .count()
}

/// Which logs set aside go: none of the newest [`KEEP_LAUNCHES`]; of the
/// rest, those older than [`KEEP_FOR`], then the oldest while all of them
/// together are more than [`KEEP_AT_MOST`].
fn to_remove(mut logs: Vec<SetAside>, now: SystemTime) -> Vec<PathBuf> {
    logs.sort_by_key(|l| std::cmp::Reverse(l.modified));
    let mut total: u64 = logs.iter().map(|l| l.len).sum();
    let mut out = Vec::new();
    // Oldest first, so the size bound takes the oldest.
    for log in logs.iter().skip(KEEP_LAUNCHES).rev() {
        let old = now
            .duration_since(log.modified)
            .is_ok_and(|age| age > KEEP_FOR);
        if old || total > KEEP_AT_MOST {
            total -= log.len;
            out.push(log.path.clone());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const DAY: Duration = Duration::from_secs(24 * 60 * 60);

    fn log(name: &str, age: Duration, len: u64, now: SystemTime) -> SetAside {
        SetAside {
            path: PathBuf::from(name),
            modified: now - age,
            len,
        }
    }

    fn names(paths: &[PathBuf]) -> Vec<String> {
        let mut names: Vec<String> = paths.iter().map(|p| p.display().to_string()).collect();
        names.sort();
        names
    }

    #[test]
    fn a_launchs_log_is_named_after_the_time_on_its_first_line() {
        let line = "2026-11-01T14:49:06.683558Z  INFO helio_client::logging: Helio started";
        assert_eq!(started_at(line).as_deref(), Some("2026-11-01-144906"));
        assert_eq!(started_at(""), None);
        assert_eq!(started_at("Helio started at 2026-11-01T14:49:06Z"), None);
        assert_eq!(started_at("2026-11-01 14:49:06.683558Z"), None);
    }

    #[test]
    fn a_log_without_a_first_line_is_named_after_when_it_was_written() {
        let t = UNIX_EPOCH + Duration::from_secs(1_793_544_546);
        assert_eq!(stamp_of(t), "2026-11-01-144906");
        assert_eq!(stamp_of(UNIX_EPOCH), "1970-01-01-000000");
        // A leap day.
        let t = UNIX_EPOCH + Duration::from_secs(1_709_164_800);
        assert_eq!(stamp_of(t), "2024-02-29-000000");
    }

    #[test]
    fn the_last_ten_launches_logs_are_kept_however_old() {
        let now = SystemTime::now();
        let logs = (0..10)
            .map(|i| log(&format!("old-{i}"), DAY * (100 + i), 1, now))
            .collect();
        assert!(to_remove(logs, now).is_empty());
    }

    #[test]
    fn a_log_from_the_last_two_weeks_outlives_any_number_of_launches_after_it() {
        let now = SystemTime::now();
        let mut logs: Vec<SetAside> = (0..50)
            .map(|i| {
                log(
                    &format!("launch-{i}"),
                    Duration::from_secs(60 * (i + 1)),
                    1,
                    now,
                )
            })
            .collect();
        logs.push(log("stream", DAY * 13, 1, now));
        assert!(to_remove(logs, now).is_empty());
    }

    #[test]
    fn beyond_the_last_ten_a_log_older_than_two_weeks_goes() {
        let now = SystemTime::now();
        let mut logs: Vec<SetAside> = (0..10)
            .map(|i| {
                log(
                    &format!("recent-{i}"),
                    DAY * 15 + Duration::from_secs(i),
                    1,
                    now,
                )
            })
            .collect();
        logs.push(log("old", DAY * 20, 1, now));
        logs.push(log("older", DAY * 30, 1, now));
        assert_eq!(names(&to_remove(logs, now)), ["old", "older"]);
    }

    #[test]
    fn past_the_size_bound_the_oldest_go_first_but_never_the_last_ten() {
        let now = SystemTime::now();
        // Ten logs of a quarter of the bound each are over it already: the
        // two beyond them go, and the ten stay.
        let big = KEEP_AT_MOST / 4;
        let mut logs: Vec<SetAside> = (0..10)
            .map(|i| log(&format!("last-{i}"), Duration::from_secs(i + 1), big, now))
            .collect();
        logs.push(log("newer", DAY, 1, now));
        logs.push(log("oldest", DAY * 2, 1, now));
        assert_eq!(names(&to_remove(logs, now)), ["newer", "oldest"]);

        // Under the bound once the oldest is gone: the newer one stays.
        let mut logs: Vec<SetAside> = (0..10)
            .map(|i| log(&format!("last-{i}"), Duration::from_secs(i + 1), 1, now))
            .collect();
        logs.push(log("newer", DAY, KEEP_AT_MOST / 2, now));
        logs.push(log("oldest", DAY * 2, KEEP_AT_MOST, now));
        assert_eq!(names(&to_remove(logs, now)), ["oldest"]);
    }

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("helio-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn each_start_sets_the_last_launchs_log_aside_and_none_is_overwritten() {
        let dir = scratch("logs-set-aside");
        let line = "2026-11-01T14:49:06.683558Z  INFO helio_client::logging: Helio started\n";
        for text in ["first", "second"] {
            std::fs::write(dir.join(CURRENT), format!("{line}{text}\n")).unwrap();
            set_aside(&dir);
        }
        assert!(!dir.join(CURRENT).exists());
        let read = |name: &str| std::fs::read_to_string(dir.join(name)).unwrap();
        assert!(read("helio-2026-11-01-144906.log").ends_with("first\n"));
        assert!(read("helio-2026-11-01-144906-2.log").ends_with("second\n"));
        // Nothing to set aside: nothing happens.
        set_aside(&dir);
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 2);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn pruning_takes_only_logs_set_aside() {
        let dir = scratch("logs-prune");
        for i in 0..11 {
            std::fs::write(dir.join(format!("helio-2026-01-01-0000{i:02}.log")), "x").unwrap();
        }
        std::fs::write(dir.join(CURRENT), "x").unwrap();
        std::fs::write(dir.join("notes.txt"), "x").unwrap();
        // A year on, every file is older than KEEP_FOR: of the eleven logs
        // set aside, the ten newest stay, and the current log and a file
        // that is not a log are not counted at all.
        assert_eq!(prune(&dir, SystemTime::now() + DAY * 365), 1);
        assert!(dir.join(CURRENT).exists());
        assert!(dir.join("notes.txt").exists());
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 12);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
