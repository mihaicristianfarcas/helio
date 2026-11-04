//! A computer's scrolling in the protocol's terms: a wheel's lines as
//! Windows' units (120 a notch, `InputEvent::Wheel`), a trackpad's distance
//! as stream pixels (`InputEvent::Scroll`), each with what is short of a
//! whole unit kept for the next event.
//!
//! A trackpad goes as a distance because only the host knows what a notch
//! is worth there (its scaling, its lines per notch): see
//! `helio_input::scroll`. Pure, so the arithmetic is tested on any machine;
//! the windows (`mac::window`, `linux`) read the events.

use helio_proto::input::whole_units;

/// What is short of a whole unit, (vertical, horizontal).
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct Residue(f64, f64);

impl Residue {
    /// `dv`, `dh` added to what was kept, as whole units clamped to the
    /// protocol's i16; what is short of one is kept.
    pub fn whole(&mut self, dv: f64, dh: f64) -> (i16, i16) {
        let (fv, fh) = (self.0 + dv, self.1 + dh);
        let (wv, wh) = (whole_units(fv), whole_units(fh));
        *self = Residue(fv - wv, fh - wh);
        let unit = |v: f64| v.clamp(i16::MIN as f64, i16::MAX as f64) as i16;
        (unit(wv), unit(wh))
    }
}

/// Stream pixels per point of a `stream`-sized picture fitted into `area`
/// (in points, or any unit of the client's), aspect kept, as the renderers
/// fit it; None before the stream has a size.
pub fn stream_pixels_per_point(area: (f64, f64), stream: (f64, f64)) -> Option<f64> {
    let points_per_pixel = (area.0 / stream.0).min(area.1 / stream.1);
    (points_per_pixel.is_finite() && points_per_pixel > 0.0).then(|| 1.0 / points_per_pixel)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_retina_screen_at_its_own_size_is_two_pixels_a_point() {
        // A MacBook Pro's 1512x945 points below the notch, streamed at
        // 3024x1890.
        assert_eq!(
            stream_pixels_per_point((1512.0, 945.0), (3024.0, 1890.0)),
            Some(2.0)
        );
    }

    #[test]
    fn a_smaller_stream_is_fewer_pixels_a_point() {
        // 1920x1080 in that area: the width touches, 1.27 pixels a point.
        let p = stream_pixels_per_point((1512.0, 945.0), (1920.0, 1080.0)).unwrap();
        assert!((p - 1920.0 / 1512.0).abs() < 1e-9);
    }

    #[test]
    fn no_stream_yet_has_no_scale() {
        assert_eq!(stream_pixels_per_point((1512.0, 945.0), (0.0, 0.0)), None);
        assert_eq!(stream_pixels_per_point((0.0, 0.0), (1920.0, 1080.0)), None);
    }

    #[test]
    fn a_slow_scroll_adds_up_to_whole_pixels() {
        let mut r = Residue::default();
        let moved: Vec<_> = (0..4).map(|_| r.whole(0.5, -0.25)).collect();
        assert_eq!(moved, [(0, 0), (1, 0), (0, 0), (1, -1)]);
    }

    #[test]
    fn a_huge_delta_is_clamped_not_wrapped() {
        let mut r = Residue::default();
        assert_eq!(r.whole(1e9, -1e9), (i16::MAX, i16::MIN));
    }
}
