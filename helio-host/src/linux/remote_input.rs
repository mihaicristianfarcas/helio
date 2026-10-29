//! Wayland: the client's input as a desktop's remote-input calls. The
//! portal (`portal`), mutter's own remote desktop (`mutter`) and KWin's fake
//! input (`kwin`) all take the same things — Linux key codes, keysyms for
//! text, buttons, wheel notches, the pointer in the stream's coordinates —
//! so the client's events are mapped once, here, into [`Op`]s, and each
//! backend sends them its way.

use helio_input::{HeldSet, InputError, InputSink};
use helio_proto::input::{Button, InputEvent};

/// One thing to tell the desktop.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Op {
    /// A Linux key code, down or up.
    Key(i32, bool),
    /// A keysym (text), down or up.
    Keysym(i32, bool),
    Motion(f64, f64),
    /// In the stream's own (logical) coordinates.
    Absolute(f64, f64),
    /// A Linux button code (BTN_*), down or up.
    Button(i32, bool),
    /// Wheel notches; `true` for the vertical wheel, positive down or right.
    Scroll(bool, i32),
}

/// A notch of the wheel, in the protocol's units (Windows' WHEEL_DELTA).
const NOTCH: i32 = 120;

/// Where a backend's ops go, in order; an error once it has gone.
pub type Deliver = Box<dyn FnMut(Vec<Op>) -> Result<(), InputError> + Send>;

/// The client's input, as a desktop's remote-input calls.
pub struct RemoteSink {
    deliver: Deliver,
    map: Mapping,
    held: HeldSet,
}

impl RemoteSink {
    /// `picture`: where the picture sits in the stream (x, y, w, h);
    /// `size`: the shared screen in the desktop's coordinates, when the
    /// desktop says (else the picture's).
    pub fn new(
        deliver: Deliver,
        picture: (u32, u32, u32, u32),
        size: Option<(u32, u32)>,
    ) -> RemoteSink {
        let size = size.unwrap_or((picture.2, picture.3));
        RemoteSink {
            deliver,
            map: Mapping {
                picture,
                size,
                wheel: (0, 0),
            },
            held: HeldSet::new(),
        }
    }
}

impl InputSink for RemoteSink {
    fn inject(&mut self, events: &[InputEvent]) -> Result<(), InputError> {
        let mut ops = Vec::with_capacity(events.len());
        for &ev in events {
            self.held.observe(ev);
            self.map.ops(ev, &mut ops);
        }
        if ops.is_empty() {
            return Ok(());
        }
        (self.deliver)(ops)
    }

    fn release_all(&mut self) -> Result<(), InputError> {
        let events = self.held.drain_release_events();
        self.map.wheel = (0, 0);
        self.inject(&events)
    }
}

/// Stream pixels and scancodes to the desktop's terms. Pure, so tested.
struct Mapping {
    /// Where the picture sits in the stream (x, y, w, h).
    picture: (u32, u32, u32, u32),
    /// The shared screen in the desktop's coordinates.
    size: (u32, u32),
    /// Wheel movement short of a notch, (vertical, horizontal).
    wheel: (i32, i32),
}

impl Mapping {
    fn ops(&mut self, ev: InputEvent, out: &mut Vec<Op>) {
        match ev {
            InputEvent::KeyDown(sc) | InputEvent::KeyUp(sc) => {
                if let Some(code) = helio_input::evdev::key_code(sc) {
                    out.push(Op::Key(code as i32, matches!(ev, InputEvent::KeyDown(_))));
                }
            }
            InputEvent::MouseMoveRel { dx, dy } => out.push(Op::Motion(dx as f64, dy as f64)),
            InputEvent::MouseMoveAbs { x, y } => {
                let (px, py, pw, ph) = self.picture;
                let (sw, sh) = self.size;
                let map = |v: u16, p: u32, len: u32, s: u32| {
                    let v = (v as f64 - p as f64).clamp(0.0, len.max(1) as f64 - 1.0);
                    v * s as f64 / len.max(1) as f64
                };
                out.push(Op::Absolute(map(x, px, pw, sw), map(y, py, ph, sh)));
            }
            InputEvent::ButtonDown(b) | InputEvent::ButtonUp(b) => {
                // BTN_LEFT, BTN_RIGHT, BTN_MIDDLE, BTN_SIDE, BTN_EXTRA.
                let code = match b {
                    Button::Left => 0x110,
                    Button::Right => 0x111,
                    Button::Middle => 0x112,
                    Button::X1 => 0x113,
                    Button::X2 => 0x114,
                };
                out.push(Op::Button(code, matches!(ev, InputEvent::ButtonDown(_))));
            }
            InputEvent::Wheel { dv, dh } => {
                // The protocol's wheel is Windows': up is positive; the
                // desktop's is Wayland's: down is.
                self.wheel.0 += dv as i32;
                self.wheel.1 += dh as i32;
                let (v, h) = (self.wheel.0 / NOTCH, self.wheel.1 / NOTCH);
                self.wheel.0 -= v * NOTCH;
                self.wheel.1 -= h * NOTCH;
                if v != 0 {
                    out.push(Op::Scroll(true, -v));
                }
                if h != 0 {
                    out.push(Op::Scroll(false, h));
                }
            }
            InputEvent::Text(c) => {
                if let Some(sym) = keysym(c) {
                    out.push(Op::Keysym(sym, true));
                    out.push(Op::Keysym(sym, false));
                }
            }
        }
    }
}

/// The X keysym that types `c`.
fn keysym(c: char) -> Option<i32> {
    let cp = c as u32;
    Some(match c {
        '\n' | '\r' => 0xFF0D,
        '\t' => 0xFF09,
        _ if cp < 0x20 || cp == 0x7F => return None,
        _ if cp <= 0x7E || (0xA0..=0xFF).contains(&cp) => cp as i32,
        _ => (0x0100_0000 | cp) as i32,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map() -> Mapping {
        // A 1920x1080 desktop, letterboxed into a 1280x800 stream.
        Mapping {
            picture: (0, 40, 1280, 720),
            size: (1920, 1080),
            wheel: (0, 0),
        }
    }

    #[test]
    fn pointer_lands_where_it_points() {
        let mut out = Vec::new();
        map().ops(InputEvent::MouseMoveAbs { x: 640, y: 400 }, &mut out);
        assert_eq!(out, [Op::Absolute(960.0, 540.0)]);
        out.clear();
        map().ops(InputEvent::MouseMoveAbs { x: 10, y: 5 }, &mut out);
        assert_eq!(
            out,
            [Op::Absolute(15.0, 0.0)],
            "above the picture: its top edge"
        );
    }

    #[test]
    fn keys_buttons_wheel_and_text() {
        let mut m = map();
        let mut out = Vec::new();
        m.ops(InputEvent::KeyDown(0x1E), &mut out);
        m.ops(InputEvent::ButtonUp(Button::Right), &mut out);
        m.ops(InputEvent::Wheel { dv: 60, dh: 0 }, &mut out);
        m.ops(InputEvent::Wheel { dv: 60, dh: -240 }, &mut out);
        m.ops(InputEvent::Text('é'), &mut out);
        assert_eq!(
            out,
            [
                Op::Key(30, true),
                Op::Button(0x111, false),
                Op::Scroll(true, -1),
                Op::Scroll(false, -2),
                Op::Keysym(0xE9, true),
                Op::Keysym(0xE9, false),
            ]
        );
    }
}
