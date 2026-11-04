//! Scrolling by a distance where apps scroll by wheel notches.
//!
//! A trackpad scrolls the client's content by a distance, which reaches the
//! host as stream pixels (`InputEvent::Scroll`). Windows' apps, and X11's,
//! take wheel notches instead, and how far a notch goes is the host's
//! business: its display scaling and its lines-per-notch setting. So the host
//! turns the distance into notches, with the figure a browser uses to turn
//! them back: Chromium on Windows scrolls a notch by lines × 100/3
//! device-independent pixels (`ui/events/blink/web_input_event_builders_win.cc`,
//! `kScrollbarPixelsPerLine`), the lines being Windows' setting, 3 unless the
//! user changed it. A browser on the host then moves its page as far as the
//! fingers moved the picture; an app whose line is shorter goes less far.
//!
//! Sending the trackpad as notches from the client instead, as Moonlight's
//! does (`mouse.cpp`, `handleMouseWheelEvent`), leaves the client to guess
//! what a notch is worth on the host. An earlier Helio guessed 40 points of a
//! Mac trackpad to a notch; on a Windows host at 175%, streamed at 3024x1890
//! to a MacBook Pro, a notch is 175 of the host's pixels, 87.5 of the Mac's
//! points, so by Chromium's figure a browser scrolled 2.2 times as far as the
//! fingers moved, the trackpad's glide after they lift included (2.81 times at
//! 225%, measured: `docs/benchmarks.md`).
//!
//! What an app does with parts of a notch is its own. Chromium and Windows'
//! Settings move for each; File Explorer keeps them and moves only on a whole
//! notch, so at 175% a movement under those 87.5 points moves nothing there.
//! Looks like a reason to multiply the distance, is not: that would shorten
//! Explorer's wait by scrolling every browser further than the fingers moved,
//! which is what this module is here to stop.

use helio_proto::input::whole_units;

/// A notch of the wheel, in the protocol's units (Windows' `WHEEL_DELTA`).
pub const NOTCH: i32 = 120;

/// How far a line of a notch scrolls, in device-independent pixels:
/// Chromium's on Windows (`kScrollbarPixelsPerLine`), "100 px per three
/// lines".
pub const PIXELS_PER_LINE: f64 = 100.0 / 3.0;

/// Lines a notch scrolls where the host does not say: Windows' default, for
/// both wheels (`SPI_GETWHEELSCROLLLINES`, `SPI_GETWHEELSCROLLCHARS`).
pub const DEFAULT_LINES: f64 = 3.0;

/// Distances in, wheel units out, with what is short of a unit kept for the
/// next: a slow scroll is many events of a pixel or two.
#[derive(Debug, Default, Clone)]
pub struct ScrollDistance {
    rest: (f64, f64),
}

impl ScrollDistance {
    pub fn new() -> ScrollDistance {
        ScrollDistance::default()
    }

    /// `dv`, `dh` stream pixels as wheel units (vertical, horizontal), in
    /// the protocol's directions. `scale` is the desktop's
    /// device-independent pixels per stream pixel; `lines` how many lines a
    /// notch scrolls on each wheel.
    pub fn wheel_units(&mut self, dv: i16, dh: i16, scale: f64, lines: (f64, f64)) -> (i32, i32) {
        let scale = if scale.is_finite() && scale > 0.0 {
            scale
        } else {
            1.0
        };
        let units_per_pixel = |lines: f64| {
            let lines = if lines.is_finite() && lines >= 1.0 {
                lines
            } else {
                DEFAULT_LINES
            };
            scale * NOTCH as f64 / (lines * PIXELS_PER_LINE)
        };
        let v = self.rest.0 + dv as f64 * units_per_pixel(lines.0);
        let h = self.rest.1 + dh as f64 * units_per_pixel(lines.1);
        let (whole_v, whole_h) = (whole_units(v), whole_units(h));
        self.rest = (v - whole_v, h - whole_h);
        (whole_v as i32, whole_h as i32)
    }

    /// Forget what is short of a unit (the session's input was reset).
    pub fn clear(&mut self) {
        self.rest = (0.0, 0.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DEFAULT: (f64, f64) = (DEFAULT_LINES, DEFAULT_LINES);

    #[test]
    fn a_hundred_pixels_at_100_percent_is_a_notch() {
        let mut s = ScrollDistance::new();
        assert_eq!(s.wheel_units(100, -100, 1.0, DEFAULT), (120, -120));
    }

    #[test]
    fn at_175_percent_a_notch_takes_175_stream_pixels() {
        // A 3024-pixel stream of a display at 175% is 1728 such pixels wide.
        let mut s = ScrollDistance::new();
        assert_eq!(s.wheel_units(175, 0, 1728.0 / 3024.0, DEFAULT), (120, 0));
    }

    #[test]
    fn a_slow_scroll_is_not_lost_to_rounding() {
        let mut s = ScrollDistance::new();
        let total: i32 = (0..100).map(|_| s.wheel_units(-1, 0, 1.0, DEFAULT).0).sum();
        assert_eq!(total, -120);
    }

    #[test]
    fn more_lines_to_a_notch_takes_fewer_units() {
        let mut s = ScrollDistance::new();
        assert_eq!(s.wheel_units(100, 100, 1.0, (6.0, 1.0)), (60, 360));
    }

    #[test]
    fn nonsense_from_the_system_falls_back_to_the_defaults() {
        let mut s = ScrollDistance::new();
        assert_eq!(s.wheel_units(100, 0, f64::NAN, (0.0, 0.0)), (120, 0));
        assert_eq!(s.wheel_units(100, 0, 0.0, (f64::INFINITY, 3.0)), (120, 0));
    }

    #[test]
    fn clearing_drops_what_was_short_of_a_unit() {
        // A pixel at half scale is 0.6 of a unit: two make one, unless the
        // first is cleared.
        let mut s = ScrollDistance::new();
        s.wheel_units(1, 0, 0.5, DEFAULT);
        assert_eq!(s.wheel_units(1, 0, 0.5, DEFAULT), (1, 0));
        s.wheel_units(1, 0, 0.5, DEFAULT);
        s.clear();
        assert_eq!(s.wheel_units(1, 0, 0.5, DEFAULT), (0, 0));
    }
}
