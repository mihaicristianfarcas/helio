//! The pointer, as the stream sees it: relative motion, or a pointer at a
//! position in stream pixels. Shared with the network thread, which hears
//! from the host.
//!
//! Moonlight's way: the host draws the pointer into the picture, as
//! Sunshine does, so the client draws none, and motion is relative unless
//! the user switches to positions with Ctrl+Alt+Shift+M (Moonlight's
//! "remote desktop" mouse). Relative motion is what games read (raw input),
//! and the pointer seen is the host's own, so it cannot disagree with where
//! a click lands.

use helio_proto::control::SessionAck;
use helio_proto::input::InputEvent;

use crate::input::InputSender;

/// Where the platform's own pointer belongs, for a client that keeps it at
/// the stream's: a controller driving it, in positions.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CursorDraw {
    /// Motion goes as positions: the platform pointer belongs at (`x`, `y`)
    /// (hidden: the host draws its own).
    pub absolute: bool,
    /// Stream pixels.
    pub x: f32,
    pub y: f32,
}

#[derive(Debug, Clone, Copy)]
pub struct PointerState {
    /// Send relative motion, as Moonlight does once a session starts.
    pub relative: bool,
    /// Ctrl+Alt+Shift+M: the other mode, until pressed again (Moonlight's
    /// mouse-mode toggle).
    pub mode_override: Option<bool>,
    /// Pointer position, stream pixels.
    pub x: f32,
    pub y: f32,
    pub captured: bool,
    stream: (f32, f32),
}

impl PointerState {
    pub fn new(stream_w: u32, stream_h: u32) -> PointerState {
        PointerState {
            relative: false,
            mode_override: None,
            x: stream_w as f32 / 2.0,
            y: stream_h as f32 / 2.0,
            captured: false,
            stream: (stream_w as f32, stream_h as f32),
        }
    }

    /// The stream's size in pixels.
    pub fn stream_size(&self) -> (f32, f32) {
        self.stream
    }

    /// Move the pointer by (`dx`, `dy`) stream pixels: relative motion for
    /// the host; or, sending positions, our pointer moves (`show` puts it
    /// where it belongs) and the host's follows to the same place.
    pub fn nudge(&mut self, dx: f64, dy: f64, input: &InputSender, show: &dyn Fn(CursorDraw)) {
        if self.is_relative() {
            input.motion(dx, dy);
            return;
        }
        self.x = (self.x + dx as f32).clamp(0.0, self.stream.0 - 1.0);
        self.y = (self.y + dy as f32).clamp(0.0, self.stream.1 - 1.0);
        show(self.draw());
        input.send(InputEvent::MouseMoveAbs {
            x: self.x as u16,
            y: self.y as u16,
        });
    }

    /// Put the pointer at (`x`, `y`) stream pixels (the local pointer is
    /// there) and the host's with it. False when that is where it was.
    pub fn place(&mut self, x: f32, y: f32, input: &InputSender) -> bool {
        let (x, y) = (
            x.clamp(0.0, self.stream.0 - 1.0),
            y.clamp(0.0, self.stream.1 - 1.0),
        );
        if (x as u16, y as u16) == (self.x as u16, self.y as u16) {
            return false;
        }
        self.x = x;
        self.y = y;
        input.send(InputEvent::MouseMoveAbs {
            x: x as u16,
            y: y as u16,
        });
        true
    }

    /// Relative motion, or positions.
    pub fn is_relative(&self) -> bool {
        self.mode_override.unwrap_or(self.relative)
    }

    /// Switch to the other mode, or back to the usual one (relative).
    /// Returns whether motion is now relative.
    pub fn toggle_mode(&mut self) -> bool {
        self.mode_override = match self.mode_override {
            None => Some(!self.relative),
            Some(_) => None,
        };
        self.is_relative()
    }

    /// The session started (or was renegotiated): its size. Re-centred on
    /// the new size, relative; the user's mode switch is kept.
    pub fn started(&mut self, ack: &SessionAck) {
        let (captured, mode_override) = (self.captured, self.mode_override);
        *self = PointerState {
            captured,
            mode_override,
            relative: true,
            ..PointerState::new(ack.width as u32, ack.height as u32)
        };
    }

    pub fn draw(&self) -> CursorDraw {
        CursorDraw {
            absolute: self.captured && !self.is_relative(),
            x: self.x,
            y: self.y,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::PointerState;
    use helio_proto::control::{AckStatus, SessionAck};

    fn ack(width: u16, height: u16) -> SessionAck {
        SessionAck {
            status: AckStatus::Ok,
            codec: 0,
            width,
            height,
            refresh_mhz: 60_000,
            bitrate_kbps: 20_000,
            audio_channels: 2,
            nonce: 0,
            host: 0,
            features: 0,
            video: 0,
            permissions: 0,
        }
    }

    #[test]
    fn a_session_gets_relative_motion_until_the_user_switches() {
        let mut p = PointerState::new(1920, 1080);
        p.captured = true;
        p.started(&ack(1920, 1080));
        assert!(p.is_relative(), "Moonlight's default");
        assert!(!p.draw().absolute);
        assert!(!p.toggle_mode(), "M: positions, the remote desktop mouse");
        assert!(p.draw().absolute);
        assert!(p.toggle_mode(), "M again: relative");
    }

    #[test]
    fn a_new_mode_keeps_the_users_switch() {
        let mut p = PointerState::new(1920, 1080);
        p.started(&ack(1920, 1080));
        assert!(!p.toggle_mode());
        p.started(&ack(3024, 1890));
        assert_eq!(
            p.mode_override,
            Some(false),
            "a new mode keeps the override"
        );
        assert!(!p.is_relative());
        assert_eq!(p.stream_size(), (3024.0, 1890.0));
    }
}
