//! `helio --help` and `helio host --help`: every command and flag, in
//! sections, what to type in one column and what it does in the next. Kept
//! as data and laid out here, so that a terminal gets it in colour
//! (`style`) and a pipe the same text without.

use crate::style::{paint, Look};

/// One line of a section: what to type, and what it does (`\n` continues
/// it on the next line, under the first).
type Entry = (&'static str, &'static str);

struct Section {
    heading: &'static str,
    entries: &'static [Entry],
}

struct Page {
    /// Who, then what it is.
    title: (&'static str, &'static str),
    usage: &'static str,
    sections: &'static [Section],
    /// Paragraphs after the sections.
    notes: &'static [&'static str],
}

/// Where what an entry does starts, and how wide what to type may be to
/// share its line (two spaces before it, two after).
const COLUMN: usize = 26;
const BESIDE: usize = COLUMN - 4;

const HELIO: Page = Page {
    title: (
        "helio",
        "play your other computers from this one, and let them play this one",
    ),
    usage: "helio COMMAND [ARGS]",
    sections: &[
        Section {
            heading: "Streaming",
            entries: &[
                ("stream NAME [FLAGS]", "stream a paired host (FLAGS below)"),
                (
                    "watch NAME [FLAGS]",
                    "watch the AI agent working on a paired host",
                ),
                ("wake NAME", "send Wake-on-LAN packets to a paired host"),
            ],
        },
        Section {
            heading: "Hosts",
            entries: &[
                ("discover", "hosts on the local network, paired or not"),
                (
                    "pair HOST[:PORT]",
                    "pair with a host: shows a PIN to type on it",
                ),
                (
                    "pair-agent HOST[:PORT]",
                    "pair this device's AI agent (a key of its own)",
                ),
                ("hosts", "paired hosts"),
                (
                    "add-host NAME ADDR X25519 MLKEM",
                    "pair by hand, with the keys `helio host identity`\nprints on the host",
                ),
                ("remove-host NAME", "forget a host"),
                ("identity", "this device's public keys (X25519, ML-KEM-768)"),
            ],
        },
        Section {
            heading: "This computer as a host (more: helio host --help)",
            entries: &[
                (
                    "host enable",
                    "hosting on: the host starts now and at every start",
                ),
                (
                    "host disable",
                    "hosting off: the host stops and no longer starts",
                ),
                (
                    "host status [--json]",
                    "off, starting, on or failed (with why)",
                ),
                ("host identity", "the host's public keys"),
                (
                    "host clients",
                    "paired devices and agents, and what each may do",
                ),
                (
                    "host permissions X25519 [LIST]",
                    "show or set what a paired device may do",
                ),
                (
                    "host add-client NAME X25519 MLKEM [--agent] [--permissions LIST]",
                    "pair a device or an agent by hand",
                ),
                ("host remove-client X25519", "unpair a device or an agent"),
            ],
        },
        Section {
            heading: "Stream flags (default: this display's size at 60 fps, Moonlight's \
                bitrate)",
            entries: &[
                ("--size WxH", "resolution, exactly as given"),
                ("--fps N", "frame rate"),
                ("--mbps N", "bitrate"),
                ("--codec h264|hevc|av1", "only this codec"),
                ("--hdr", "HDR, when this display shows it and the host can"),
                (
                    "--yuv444",
                    "4:4:4 colour, when this computer decodes it and the host can",
                ),
                ("--windowed", "a window, not full screen"),
                ("--no-vsync", "present frames at once, tearing allowed"),
                ("--frame-pacing", "one frame per display refresh"),
                ("--stats", "show the statistics from the start"),
                ("--cmd-is-win", "send Command as the Windows key (macOS)"),
                (
                    "--mute-in-background",
                    "silence the stream while its window is not in front",
                ),
                ("--audio-channels 2|6|8", "stereo, 5.1 or 7.1"),
                ("--no-audio", "no sound"),
                ("--host-audio", "play the sound on the host too"),
                (
                    "--steam",
                    "open Steam Big Picture on the host for the session",
                ),
                ("--keep-host-displays", "leave the host's own monitors on"),
                ("--no-clipboard", "do not share the clipboard"),
                ("--wan-only", "only the internet path: to test it"),
                (
                    "--via ADDR",
                    "only this address (repeatable): to diagnose a path",
                ),
            ],
        },
    ],
    notes: &[
        "Ctrl-C ends a stream as Ctrl+Alt+Shift+Q does. More in docs/cli.md:\n\
         https://github.com/mihaicristianfarcas/helio/blob/main/docs/cli.md",
    ],
};

const HOST: Page = Page {
    title: (
        "helio host",
        "this computer as a host (Helio Host, which hosting runs)",
    ),
    usage: "helio host COMMAND [ARGS]",
    sections: &[Section {
        heading: "Commands",
        entries: &[
            (
                "enable",
                "hosting on: the host starts now and at every start after",
            ),
            (
                "disable",
                "hosting off: the host stops and no longer starts",
            ),
            (
                "status [--json]",
                "off, starting, on or failed (with why); for on, whether\nthe host's web \
                 UI answers on this computer. --json:\none line, for programs",
            ),
            (
                "restart",
                "the running host starts again: now, or when the session\nthat runs ends",
            ),
            (
                "identity",
                "the host's public keys, for `helio add-host` on a client",
            ),
            (
                "clients",
                "paired devices and agents: name, X25519 key, person or\nagent, permissions",
            ),
            ("permissions X25519", "what a paired device or agent may do"),
            (
                "permissions X25519 LIST",
                "set it: all, none, see-only, or names joined by commas:\nview, keyboard, \
                 mouse, controller, clipboard_read,\nclipboard_write, launch, take_over, \
                 watch (a person's\ndevice); view, keyboard, mouse, unwatched (an agent)",
            ),
            (
                "add-client NAME X25519 MLKEM [--agent] [--permissions LIST]",
                "pair a device by hand, with the keys `helio identity`\nprints on it (an \
                 agent: `helio-agent identity`, and\n--agent). Without --permissions, what \
                 pairing gives",
            ),
            ("remove-client X25519", "unpair a device or an agent"),
        ],
    }],
    notes: &[
        "On Windows, enable and disable need an administrator: Windows asks. identity,\n\
         clients, permissions, add-client, remove-client and restart read the host's\n\
         data, which only administrators may read there: run them in an administrator\n\
         terminal.",
        "A running host reads its clients when it starts, so permissions, add-client\n\
         and remove-client restart it: now, or when the session that runs ends.\n\
         Helio's window and the host's web UI change them without a restart.",
    ],
};

/// The help: `helio host --help`'s (`host`) or `helio --help`'s, in colour
/// or not.
pub fn text(host: bool, colour: bool) -> String {
    let page = if host { &HOST } else { &HELIO };
    let mut out = format!(
        "{}: {}\n\n{} {}\n",
        paint(colour, Look::Literal, page.title.0),
        page.title.1,
        paint(colour, Look::Heading, "Usage:"),
        typed(page.usage, colour),
    );
    for section in page.sections {
        out.push('\n');
        out.push_str(&paint(
            colour,
            Look::Heading,
            &format!("{}:", section.heading),
        ));
        out.push('\n');
        for (usage, does) in section.entries {
            entry(&mut out, usage, does, colour);
        }
    }
    for note in page.notes {
        out.push('\n');
        out.push_str(note);
        out.push('\n');
    }
    out
}

/// One entry: what to type, two spaces in; what it does at [`COLUMN`],
/// beside it when it fits, else on the next line.
fn entry(out: &mut String, usage: &str, does: &str, colour: bool) {
    out.push_str("  ");
    out.push_str(&typed(usage, colour));
    let mut lines = does.lines();
    let width = usage.chars().count();
    if width <= BESIDE {
        out.push_str(&" ".repeat(COLUMN - 2 - width));
    } else {
        out.push('\n');
        out.push_str(&" ".repeat(COLUMN));
    }
    out.push_str(lines.next().unwrap_or(""));
    out.push('\n');
    for line in lines {
        out.push_str(&" ".repeat(COLUMN));
        out.push_str(line);
        out.push('\n');
    }
}

/// What to type, each word as what it is: a command or a flag as it is
/// typed, a NAME or an [OPTION] as what goes in its place.
fn typed(usage: &str, colour: bool) -> String {
    usage
        .split(' ')
        .map(|word| {
            let literal = word.starts_with(|c: char| c.is_ascii_lowercase() || c == '-');
            let look = if literal {
                Look::Literal
            } else {
                Look::Placeholder
            };
            paint(colour, look, word)
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_help_in_plain_text_lines_up_as_a_terminal_shows_it() {
        let help = text(false, false);
        assert!(help.starts_with(
            "helio: play your other computers from this one, and let them play this \
             one\n\nUsage: helio COMMAND [ARGS]\n\nStreaming:\n  stream NAME [FLAGS]     \
             stream a paired host (FLAGS below)\n"
        ));
        // Too wide to share a line: what it does goes under it.
        assert!(help.contains(
            "  add-host NAME ADDR X25519 MLKEM\n                          pair by hand, \
             with the keys `helio host identity`\n                          prints on the \
             host\n"
        ));
        assert!(!help.contains('\x1b'));
    }

    #[test]
    fn the_help_in_colour_is_the_same_text_with_escape_codes() {
        for host in [false, true] {
            let painted = text(host, true);
            assert!(painted.contains("\x1b[1;32mUsage:\x1b[0m"));
            let mut plain = String::new();
            let mut rest = painted.as_str();
            while let Some(start) = rest.find('\x1b') {
                plain.push_str(&rest[..start]);
                let end = rest[start..].find('m').unwrap();
                rest = &rest[start + end + 1..];
            }
            plain.push_str(rest);
            assert_eq!(plain, text(host, false));
        }
    }

    #[test]
    fn a_command_is_painted_as_typed_and_a_placeholder_as_what_goes_there() {
        assert_eq!(
            typed("stream NAME [FLAGS]", true),
            "\x1b[1;36mstream\x1b[0m \x1b[36mNAME\x1b[0m \x1b[36m[FLAGS]\x1b[0m"
        );
        assert_eq!(
            typed("--codec h264|hevc|av1", true),
            "\x1b[1;36m--codec\x1b[0m \x1b[1;36mh264|hevc|av1\x1b[0m"
        );
    }
}
