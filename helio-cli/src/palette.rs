//! The colours `helio` paints with: the window's. Headings and what to type
//! in the window's soft blue (`helio_ui::theme::ACCENT`), what is said in
//! passing in its gray (`Ink::IDLE`), and colour otherwise kept for status,
//! in the window's green, amber and red. The values are copied from
//! `helio-ui/src/theme.rs` rather than shared: that crate is GPUI, which a
//! command line has no use for. Change them together.
//!
//! The soft blue is for a dark background, as the window's dark mode has it;
//! on a light one it would barely show (1.4:1 on white), so a light terminal
//! gets the same hue deep enough to read, and the window's light-mode status
//! colours. Which background it is: `COLORFGBG` when the terminal sets it
//! (rxvt's convention, which Konsole and iTerm2 follow), else on Linux and
//! macOS the terminal is asked (OSC 11, its background colour, followed by
//! a device-attributes query every terminal answers, so one that does not
//! know OSC 11 costs no wait), else dark, as the terminals on Windows open.
//!
//! In 24-bit colour where the terminal says it has it (`COLORTERM`, as
//! termstandard/colors describes) and on Windows, whose console has it once
//! escape codes are on; else the nearest of xterm's 256; the Linux console,
//! which has neither, gets the 16 basic colours.

/// What a colour is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// Headings, what to type: the window's soft blue.
    Accent,
    /// Said in passing, what to type in its place.
    Gray,
    Good,
    Warn,
    Bad,
}

type Rgb = (u8, u8, u8);

/// The window's colours on a dark surface (`helio_ui::theme`: `ACCENT`,
/// `Ink::IDLE`, `Ink::FRESH`, `Ink::ATTENTION`, `Ink::DANGER`).
const DARK: [Rgb; 5] = [
    (0xc2, 0xd6, 0xf6),
    (0x8e, 0x8e, 0x93),
    (0x34, 0xc7, 0x59),
    (0xf5, 0xa6, 0x23),
    (0xf5, 0x45, 0x3a),
];

/// On a light one: the soft blue's hue (217°) deepened to 6.5:1 on white,
/// the same gray (3.3:1, as the window's quiet text there), and the
/// window's light-mode status inks (`Theme::light`: `fresh`, `attention`,
/// `danger`).
const LIGHT: [Rgb; 5] = [
    (0x30, 0x5d, 0xa6),
    (0x8e, 0x8e, 0x93),
    (0x24, 0x8a, 0x3d),
    (0xb2, 0x6a, 0x00),
    (0xd7, 0x00, 0x15),
];

/// How many colours the terminal shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Depth {
    True,
    Xterm256,
    Basic,
}

/// The colours of one terminal: its depth and its background.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Palette {
    pub depth: Depth,
    pub light: bool,
}

impl Palette {
    /// The SGR parameters that paint `role`.
    pub fn sgr(self, role: Role) -> String {
        let i = role as usize;
        let (r, g, b) = if self.light { LIGHT[i] } else { DARK[i] };
        match self.depth {
            Depth::True => format!("38;2;{r};{g};{b}"),
            Depth::Xterm256 => format!("38;5;{}", xterm256((r, g, b))),
            Depth::Basic => match role {
                Role::Accent => "34",
                Role::Gray => "90",
                Role::Good => "32",
                Role::Warn => "33",
                Role::Bad => "31",
            }
            .to_string(),
        }
    }

    /// This terminal's. `ask`: the terminal may be asked for its
    /// background.
    pub fn of_terminal(ask: bool) -> Palette {
        let var = |name| std::env::var(name).ok();
        let depth = depth(
            var("COLORTERM").as_deref(),
            var("TERM").as_deref(),
            cfg!(windows),
        );
        let light = var("COLORFGBG")
            .as_deref()
            .and_then(light_from_colorfgbg)
            .or_else(|| if ask { ask_terminal() } else { None })
            .unwrap_or(false);
        Palette { depth, light }
    }
}

/// The depth, from `COLORTERM` and `TERM`.
fn depth(colorterm: Option<&str>, term: Option<&str>, windows: bool) -> Depth {
    if windows || matches!(colorterm, Some("truecolor" | "24bit")) {
        Depth::True
    } else if term == Some("linux") {
        Depth::Basic
    } else {
        Depth::Xterm256
    }
}

/// `COLORFGBG` is "FG;BG" (or "FG;default;BG"), two of the 16 basic colours:
/// a background of 7 (white) or 9 to 15 (the bright ones but black's) is
/// light.
fn light_from_colorfgbg(value: &str) -> Option<bool> {
    let bg: u8 = value.rsplit(';').next()?.trim().parse().ok()?;
    (bg < 16).then_some(bg == 7 || bg >= 9)
}

/// The nearest of xterm's 256 colours: its 6×6×6 cube (16 to 231) or its
/// 24 grays (232 to 255), whichever is closer.
fn xterm256((r, g, b): Rgb) -> u8 {
    const LEVELS: [u8; 6] = [0, 95, 135, 175, 215, 255];
    let level = |c: u8| {
        (0..6)
            .min_by_key(|&i| (LEVELS[i] as i32 - c as i32).abs())
            .unwrap_or(0)
    };
    let distance = |(r2, g2, b2): Rgb| {
        let d = |a: u8, b: u8| (a as i32 - b as i32).pow(2);
        d(r, r2) + d(g, g2) + d(b, b2)
    };
    let (ri, gi, bi) = (level(r), level(g), level(b));
    let cube = (LEVELS[ri], LEVELS[gi], LEVELS[bi]);
    let average = (r as u32 + g as u32 + b as u32) / 3;
    let gray_index = (average.saturating_sub(3) / 10).min(23) as u8;
    let gray_value = 8 + 10 * gray_index;
    if distance((gray_value, gray_value, gray_value)) < distance(cube) {
        232 + gray_index
    } else {
        16 + 36 * ri as u8 + 6 * gi as u8 + bi as u8
    }
}

/// How long a terminal has to answer. Local terminals answer within a few
/// milliseconds, and the device-attributes answer ends the wait at once; the
/// rest is for a terminal at the far end of an ssh session.
#[cfg(unix)]
const ASK_FOR: std::time::Duration = std::time::Duration::from_millis(250);

/// Ask the terminal for its background: whether it is light. Only from the
/// foreground: a program in the background that changes the terminal's
/// modes is stopped (SIGTTOU), and its output would not be in front anyway.
#[cfg(unix)]
fn ask_terminal() -> Option<bool> {
    use std::io::{Read, Write};
    use std::os::fd::AsRawFd;
    use std::time::Instant;

    let mut tty = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open("/dev/tty")
        .ok()?;
    let fd = tty.as_raw_fd();
    if fd >= libc::FD_SETSIZE as i32 {
        return None;
    }
    // SAFETY: `fd` is the terminal opened above, open until `tty` drops;
    // the termios structs are plain data, read before they are written back.
    let saved = unsafe {
        if libc::tcgetpgrp(fd) != libc::getpgrp() {
            return None;
        }
        let mut saved: libc::termios = std::mem::zeroed();
        if libc::tcgetattr(fd, &mut saved) != 0 {
            return None;
        }
        let mut quiet = saved;
        // Neither echoed nor held for a line: the answer is read as it comes.
        quiet.c_lflag &= !(libc::ICANON | libc::ECHO);
        quiet.c_cc[libc::VMIN] = 0;
        quiet.c_cc[libc::VTIME] = 0;
        if libc::tcsetattr(fd, libc::TCSANOW, &quiet) != 0 {
            return None;
        }
        saved
    };
    let mut answer = Vec::new();
    if tty.write_all(b"\x1b]11;?\x1b\\\x1b[c").is_ok() && tty.flush().is_ok() {
        let until = Instant::now() + ASK_FOR;
        let mut buf = [0u8; 128];
        while !attributes_answered(&answer) && answer.len() < 1024 {
            let left = until.saturating_duration_since(Instant::now());
            if left.is_zero() {
                break;
            }
            // select, not poll: macOS's poll does not take a terminal
            // (it answers POLLNVAL at once).
            // SAFETY: one descriptor below FD_SETSIZE (checked above) in a
            // set on the stack, and a timeout on the stack.
            let n = unsafe {
                let mut set: libc::fd_set = std::mem::zeroed();
                libc::FD_ZERO(&mut set);
                libc::FD_SET(fd, &mut set);
                let mut wait = libc::timeval {
                    tv_sec: left.as_secs() as libc::time_t,
                    tv_usec: left.subsec_micros() as libc::suseconds_t,
                };
                libc::select(
                    fd + 1,
                    &mut set,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    &mut wait,
                )
            };
            if n <= 0 {
                break;
            }
            match tty.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => answer.extend_from_slice(&buf[..n]),
            }
        }
    }
    // SAFETY: the same terminal, its modes as they were.
    unsafe { libc::tcsetattr(fd, libc::TCSANOW, &saved) };
    background_is_light(&answer)
}

/// Windows' console has no query to answer; a dark background is what its
/// terminals open with.
#[cfg(not(unix))]
fn ask_terminal() -> Option<bool> {
    None
}

/// Whether the device-attributes answer ("ESC [ ? … c") has come: it comes
/// after the background's, when the terminal gives one.
#[cfg(any(unix, test))]
fn attributes_answered(answer: &[u8]) -> bool {
    answer
        .windows(3)
        .position(|w| w == b"\x1b[?")
        .is_some_and(|at| answer[at + 3..].contains(&b'c'))
}

/// Light or not, from the terminal's answer to OSC 11:
/// "ESC ] 11 ; rgb:RRRR/GGGG/BBBB" (1 to 4 hex digits a channel), then BEL
/// or ST. None when it gave none.
#[cfg(any(unix, test))]
fn background_is_light(answer: &[u8]) -> Option<bool> {
    let text = std::str::from_utf8(answer).ok()?;
    let at = text.find("]11;rgb:")? + "]11;rgb:".len();
    let channels: Vec<f32> = text[at..]
        .split(['/', '\x07', '\x1b'])
        .take(3)
        .map(|c| {
            let digits = c.len();
            let max = 16u32.checked_pow(digits as u32)?.checked_sub(1)?;
            (1..=4).contains(&digits).then_some(())?;
            Some(u32::from_str_radix(c, 16).ok()? as f32 / max as f32)
        })
        .collect::<Option<_>>()?;
    let [r, g, b] = channels[..] else {
        return None;
    };
    // Its luminance (Rec. 709 weights): over half, dark text reads on it.
    Some(0.2126 * r + 0.7152 * g + 0.0722 * b > 0.5)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_soft_blue_and_the_gray_are_the_windows_own_and_the_status_colours_too() {
        let dark = Palette {
            depth: Depth::True,
            light: false,
        };
        assert_eq!(dark.sgr(Role::Accent), "38;2;194;214;246");
        assert_eq!(dark.sgr(Role::Gray), "38;2;142;142;147");
        assert_eq!(dark.sgr(Role::Bad), "38;2;245;69;58");
        let light = Palette {
            light: true,
            ..dark
        };
        assert_eq!(light.sgr(Role::Good), "38;2;36;138;61");
    }

    #[test]
    fn without_24_bit_colour_each_is_the_nearest_of_xterms_256() {
        // #c2d6f6 → #afd7ff, #8e8e93 → the gray #949494.
        assert_eq!(xterm256((0xc2, 0xd6, 0xf6)), 153);
        assert_eq!(xterm256((0x8e, 0x8e, 0x93)), 246);
        assert_eq!(xterm256((0, 0, 0)), 16);
        assert_eq!(xterm256((255, 255, 255)), 231);
        let p = Palette {
            depth: Depth::Xterm256,
            light: false,
        };
        assert_eq!(p.sgr(Role::Accent), "38;5;153");
    }

    #[test]
    fn colorterm_says_24_bit_and_the_linux_console_has_only_the_basic_colours() {
        assert_eq!(depth(Some("truecolor"), None, false), Depth::True);
        assert_eq!(depth(Some("24bit"), None, false), Depth::True);
        assert_eq!(depth(None, Some("xterm-256color"), false), Depth::Xterm256);
        assert_eq!(depth(None, Some("linux"), false), Depth::Basic);
        assert_eq!(depth(None, None, true), Depth::True);
    }

    #[test]
    fn colorfgbg_names_a_light_background_by_its_last_field() {
        assert_eq!(light_from_colorfgbg("0;15"), Some(true));
        assert_eq!(light_from_colorfgbg("0;7"), Some(true));
        assert_eq!(light_from_colorfgbg("15;0"), Some(false));
        assert_eq!(light_from_colorfgbg("15;default;8"), Some(false));
        assert_eq!(light_from_colorfgbg("15;default;0"), Some(false));
        assert_eq!(light_from_colorfgbg("nonsense"), None);
        assert_eq!(light_from_colorfgbg("0;99"), None);
    }

    #[test]
    fn the_terminals_answer_says_its_background_whatever_its_digits() {
        let white = b"\x1b]11;rgb:ffff/ffff/ffff\x1b\\\x1b[?62;22c";
        assert_eq!(background_is_light(white), Some(true));
        let dark = b"\x1b]11;rgb:1d1d/1c1c/1a1a\x07\x1b[?1;2c";
        assert_eq!(background_is_light(dark), Some(false));
        assert_eq!(background_is_light(b"\x1b]11;rgb:f/f/e\x07"), Some(true));
        // No OSC 11, only the attributes: nothing said.
        assert_eq!(background_is_light(b"\x1b[?1;2c"), None);
        assert_eq!(background_is_light(b"\x1b]11;rgb:zz/00/00\x07"), None);
        assert_eq!(background_is_light(b"\x1b]11;rgb:12345/0/0\x07"), None);
    }

    #[test]
    fn the_wait_ends_with_the_attributes_answer() {
        assert!(!attributes_answered(b"\x1b]11;rgb:ffff/ffff/ffff\x1b\\"));
        assert!(!attributes_answered(b"\x1b[?62;2"));
        assert!(attributes_answered(b"\x1b]11;rgb:0/0/0\x07\x1b[?62;22c"));
    }
}
