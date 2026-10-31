//! How `helio` looks in a terminal: headings, commands and what worked or
//! failed in colour, lists in aligned columns, as cargo's and gh's output
//! does (cargo's colours: headings bold green, what to type cyan).
//!
//! Only in a terminal. Piped or redirected, the output is plain text, lists
//! tab-separated, as programs read it ([`terminal`]). Colour follows the
//! conventions: `NO_COLOR` (no-color.org) turns it off, `CLICOLOR_FORCE`
//! (bixense.com/clicolors) on, through a pager too, and a `dumb` terminal
//! gets none ([`wanted`]). Windows' console shows the escape codes only
//! once asked to (`ENABLE_VIRTUAL_TERMINAL_PROCESSING`); where it refuses,
//! there is no colour.

use std::io::IsTerminal;
use std::sync::OnceLock;

/// Where text goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum To {
    Out,
    Err,
}

/// What a piece of text is, which says how it looks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Look {
    /// A section's heading.
    Heading,
    /// What to type as it is: a command, a flag.
    Literal,
    /// What to type in its place: NAME, [FLAGS].
    Placeholder,
    /// Said in passing: a note, a list's header, what is under way.
    Quiet,
    /// What worked, and what is on.
    Good,
    /// What is under way, or half there.
    Warn,
    /// What failed.
    Bad,
    /// What to look at: a PIN to type.
    Strong,
}

impl Look {
    /// Its SGR parameters.
    fn sgr(self) -> &'static str {
        match self {
            Look::Heading => "1;32",
            Look::Literal => "1;36",
            Look::Placeholder => "36",
            Look::Quiet => "2",
            Look::Good => "32",
            Look::Warn => "33",
            Look::Bad => "1;31",
            Look::Strong => "1",
        }
    }
}

/// Whether `to` is a terminal: lists are laid out for a person, not as
/// tab-separated lines for a program.
pub fn terminal(to: To) -> bool {
    match to {
        To::Out => std::io::stdout().is_terminal(),
        To::Err => std::io::stderr().is_terminal(),
    }
}

/// Whether text to `to` is in colour (asked once each).
pub fn colour(to: To) -> bool {
    static OUT: OnceLock<bool> = OnceLock::new();
    static ERR: OnceLock<bool> = OnceLock::new();
    let cell = match to {
        To::Out => &OUT,
        To::Err => &ERR,
    };
    *cell.get_or_init(|| {
        let var = |name| std::env::var(name).ok();
        wanted(
            var("NO_COLOR").as_deref(),
            var("CLICOLOR_FORCE").as_deref(),
            var("TERM").as_deref(),
            terminal(to),
        ) && escapes_shown(to)
    })
}

/// Colour or not, from `NO_COLOR`, `CLICOLOR_FORCE` and `TERM`, and
/// whether the text goes to a terminal.
fn wanted(no_color: Option<&str>, force: Option<&str>, term: Option<&str>, tty: bool) -> bool {
    if no_color.is_some_and(|v| !v.is_empty()) {
        false
    } else if force.is_some_and(|v| !v.is_empty() && v != "0") {
        true
    } else {
        tty && term != Some("dumb")
    }
}

/// Windows: ask `to`'s console to show escape codes; whether it does.
#[cfg(windows)]
fn escapes_shown(to: To) -> bool {
    use windows::Win32::System::Console::{
        GetConsoleMode, GetStdHandle, SetConsoleMode, CONSOLE_MODE,
        ENABLE_VIRTUAL_TERMINAL_PROCESSING, STD_ERROR_HANDLE, STD_OUTPUT_HANDLE,
    };
    let which = match to {
        To::Out => STD_OUTPUT_HANDLE,
        To::Err => STD_ERROR_HANDLE,
    };
    // SAFETY: the process's own standard handle, and a mode read into a
    // local before it is written back with one flag more.
    unsafe {
        let Ok(handle) = GetStdHandle(which) else {
            return false;
        };
        let mut mode = CONSOLE_MODE::default();
        if GetConsoleMode(handle, &mut mode).is_err() {
            // Not a console (a file, a pipe), unless CLICOLOR_FORCE: then
            // whatever reads it is asked to take the codes.
            return true;
        }
        mode.contains(ENABLE_VIRTUAL_TERMINAL_PROCESSING)
            || SetConsoleMode(handle, mode | ENABLE_VIRTUAL_TERMINAL_PROCESSING).is_ok()
    }
}

/// Elsewhere terminals show them.
#[cfg(not(windows))]
fn escapes_shown(_: To) -> bool {
    true
}

/// `text` as `look` makes it, in colour or not.
pub fn paint(colour: bool, look: Look, text: &str) -> String {
    if colour && !text.is_empty() {
        format!("\x1b[{}m{text}\x1b[0m", look.sgr())
    } else {
        text.to_string()
    }
}

/// `text` as `look` makes it, for `to`.
pub fn to(to: To, look: Look, text: &str) -> String {
    paint(colour(to), look, text)
}

/// Print a failure on stderr: `error:`, then what happened.
pub fn error(what: impl std::fmt::Display) {
    eprintln!("{} {what}", to(To::Err, Look::Bad, "error:"));
}

/// Print on stderr what went wrong beside a command that worked:
/// `warning:`, then what.
pub fn warn(what: impl std::fmt::Display) {
    eprintln!("{} {what}", to(To::Err, Look::Warn, "warning:"));
}

/// `rows` in columns two spaces apart, as wide as their widest cell, with
/// `header` above them (quiet); each cell painted by `look`, given its
/// column and text. The last column is not padded.
pub fn table(
    header: &[&str],
    rows: &[Vec<String>],
    colour: bool,
    look: impl Fn(usize, &str) -> Option<Look>,
) -> String {
    let columns = header.len();
    let widths: Vec<usize> = (0..columns)
        .map(|c| {
            rows.iter()
                .filter_map(|r| r.get(c))
                .map(|cell| cell.chars().count())
                .chain([header[c].chars().count()])
                .max()
                .unwrap_or(0)
        })
        .collect();
    let line = |cells: Vec<(String, usize)>| {
        let last = cells.len().saturating_sub(1);
        let mut out = String::new();
        for (c, (painted, plain)) in cells.into_iter().enumerate() {
            out.push_str(&painted);
            if c < last {
                out.push_str(&" ".repeat(widths[c] - plain + 2));
            }
        }
        out.trim_end().to_string() + "\n"
    };
    let mut out = line(
        header
            .iter()
            .map(|h| (paint(colour, Look::Quiet, h), h.chars().count()))
            .collect(),
    );
    for row in rows {
        out.push_str(&line(
            row.iter()
                .enumerate()
                .map(|(c, cell)| {
                    let painted = match look(c, cell) {
                        Some(l) => paint(colour, l, cell),
                        None => cell.clone(),
                    };
                    (painted, cell.chars().count())
                })
                .collect(),
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colour_is_for_a_terminal_unless_no_color_says_otherwise_or_it_is_forced() {
        assert!(wanted(None, None, Some("xterm-256color"), true));
        assert!(!wanted(None, None, Some("xterm-256color"), false));
        assert!(!wanted(Some("1"), None, None, true));
        // An empty NO_COLOR is not set (no-color.org).
        assert!(wanted(Some(""), None, None, true));
        assert!(!wanted(None, None, Some("dumb"), true));
        assert!(wanted(None, Some("1"), None, false));
        assert!(!wanted(None, Some("0"), None, false));
        assert!(!wanted(Some("1"), Some("1"), None, true));
    }

    #[test]
    fn painted_text_is_the_text_between_escape_codes_and_plain_text_is_itself() {
        assert_eq!(paint(true, Look::Bad, "error:"), "\x1b[1;31merror:\x1b[0m");
        assert_eq!(paint(false, Look::Bad, "error:"), "error:");
        assert_eq!(paint(true, Look::Bad, ""), "");
    }

    #[test]
    fn a_table_lines_up_its_columns_by_the_text_not_the_colour() {
        let rows = vec![
            vec!["gaming-pc".to_string(), "192.168.1.20:47800".to_string()],
            vec!["mac".to_string(), "203.0.113.7:47800".to_string()],
        ];
        let plain = table(&["NAME", "ADDRESS"], &rows, false, |_, _| None);
        assert_eq!(
            plain,
            "NAME       ADDRESS\n\
             gaming-pc  192.168.1.20:47800\n\
             mac        203.0.113.7:47800\n"
        );
        let painted = table(&["NAME", "ADDRESS"], &rows, true, |c, _| {
            (c == 0).then_some(Look::Strong)
        });
        assert!(painted.contains("\x1b[1mmac\x1b[0m        203.0.113.7:47800\n"));
    }
}
