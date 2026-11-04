//! The scale an X desktop runs at, read from its XSETTINGS, for the X11 sink:
//! a browser's pixel is that many of the screen's, so a notch goes that much
//! further. Pure, so the parsing is tested on any machine.
//!
//! The desktop's settings daemon (GNOME's `gsd-xsettings`, Xfce's
//! `xfsettingsd`, `xsettingsd`) owns the selection `_XSETTINGS_S<screen>` and
//! keeps its settings in the property `_XSETTINGS_SETTINGS` of its window. The
//! scale is `Gdk/WindowScalingFactor` times `Gdk/UnscaledDPI` (else `Xft/DPI`)
//! over 96, which is how GTK reads them (`gdk/x11/xsettings-client.c`), and
//! what Chromium's and Firefox's device pixel ratio came to in every case
//! measured (`docs/benchmarks.md`). KDE writes the first two and removes the
//! third (`kde-gtk-config`, `kded/gtkconfig.cpp`).
//!
//! `Xft.dpi` in the X resources looks like the simple way to the same number,
//! and is not: without a settings daemon both browsers ignored it, and Xfce at
//! 200% leaves it at 96 (same measurements).

/// The DPI a scale of 1 is.
const BASE_DPI: f64 = 96.0;

/// XSETTINGS holds a DPI in 1024ths of a dot per inch.
const DPI_UNIT: f64 = 1024.0;

/// A scale outside these is a broken setting, not a desktop: a very small one
/// would turn one trackpad event into a long run of notches.
const PLAUSIBLE_SCALE: std::ops::RangeInclusive<f64> = 0.5..=8.0;

/// The desktop's scale from the contents of `_XSETTINGS_SETTINGS`; None when
/// they cannot be read or make no sense. Settings left out, and a DPI of -1,
/// count as a scale of 1, as both browsers took them.
pub fn desktop_scale(settings: &[u8]) -> Option<f64> {
    let (mut xft_dpi, mut unscaled_dpi, mut window_scale) = (None, None, None);
    for_each_integer(settings, |name, value| match name {
        b"Xft/DPI" => xft_dpi = Some(value),
        b"Gdk/UnscaledDPI" => unscaled_dpi = Some(value),
        b"Gdk/WindowScalingFactor" => window_scale = Some(value),
        _ => {}
    })?;
    let given = |v: Option<i32>| v.filter(|&v| v > 0);
    let dpi = given(unscaled_dpi)
        .or(given(xft_dpi))
        .map_or(BASE_DPI, |d| d as f64 / DPI_UNIT);
    let scale = given(window_scale).unwrap_or(1) as f64 * dpi / BASE_DPI;
    PLAUSIBLE_SCALE.contains(&scale).then_some(scale)
}

/// Each integer setting of the property, in the format of the XSETTINGS
/// specification; None when the property is cut short or malformed.
fn for_each_integer(data: &[u8], mut found: impl FnMut(&[u8], i32)) -> Option<()> {
    let mut r = Reader {
        rest: data,
        big_endian: match *data.first()? {
            0 => false,
            1 => true,
            _ => return None,
        },
    };
    r.take(8)?; // Byte order, three unused bytes, the serial.
    let count = r.card32()?;
    for _ in 0..count {
        let kind = r.take(2)?[0]; // The type, and an unused byte.
        let name_len = r.card16()? as usize;
        let name = r.padded(name_len)?;
        r.take(4)?; // The serial it last changed at.
        match kind {
            SETTING_INTEGER => found(name, r.card32()? as i32),
            SETTING_STRING => {
                let len = r.card32()? as usize;
                r.padded(len)?;
            }
            SETTING_COLOR => {
                r.take(8)?;
            }
            _ => return None,
        }
    }
    Some(())
}

const SETTING_INTEGER: u8 = 0;
const SETTING_STRING: u8 = 1;
const SETTING_COLOR: u8 = 2;

struct Reader<'a> {
    rest: &'a [u8],
    big_endian: bool,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        if n > self.rest.len() {
            return None;
        }
        let (taken, rest) = self.rest.split_at(n);
        self.rest = rest;
        Some(taken)
    }

    /// `n` bytes, and the padding that brings them to a multiple of four.
    fn padded(&mut self, n: usize) -> Option<&'a [u8]> {
        let bytes = self.take(n)?;
        self.take(n.next_multiple_of(4) - n)?;
        Some(bytes)
    }

    fn card16(&mut self) -> Option<u16> {
        let b: [u8; 2] = self.take(2)?.try_into().ok()?;
        Some(if self.big_endian {
            u16::from_be_bytes(b)
        } else {
            u16::from_le_bytes(b)
        })
    }

    fn card32(&mut self) -> Option<u32> {
        let b: [u8; 4] = self.take(4)?.try_into().ok()?;
        Some(if self.big_endian {
            u32::from_be_bytes(b)
        } else {
            u32::from_le_bytes(b)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DPI: i32 = 1024;

    /// A property as a settings daemon writes it, with a string and a colour
    /// among the integers to be stepped over.
    fn property(big_endian: bool, integers: &[(&str, i32)]) -> Vec<u8> {
        let card16 = |v: u16| {
            if big_endian {
                v.to_be_bytes()
            } else {
                v.to_le_bytes()
            }
        };
        let card32 = |v: u32| {
            if big_endian {
                v.to_be_bytes()
            } else {
                v.to_le_bytes()
            }
        };
        let name_of = |out: &mut Vec<u8>, kind: u8, name: &str| {
            out.extend([kind, 0]);
            out.extend(card16(name.len() as u16));
            out.extend(name.as_bytes());
            out.resize(out.len().next_multiple_of(4), 0);
            out.extend(card32(7));
        };
        let mut out = vec![u8::from(big_endian), 0, 0, 0];
        out.extend(card32(1));
        out.extend(card32(integers.len() as u32 + 2));
        name_of(&mut out, SETTING_STRING, "Net/ThemeName");
        out.extend(card32(7));
        out.extend(b"Adwaita");
        out.resize(out.len().next_multiple_of(4), 0);
        name_of(&mut out, SETTING_COLOR, "Gtk/Color");
        out.extend([0; 8]);
        for &(name, value) in integers {
            name_of(&mut out, SETTING_INTEGER, name);
            out.extend(card32(value as u32));
        }
        out
    }

    fn scale(integers: &[(&str, i32)]) -> Option<f64> {
        desktop_scale(&property(false, integers))
    }

    // The values below are what the daemons wrote when measured
    // (docs/benchmarks.md), and what KDE's source says it writes.

    #[test]
    fn gnome_at_200_percent_is_2() {
        let at_200 = [
            ("Xft/DPI", 192 * DPI),
            ("Gdk/UnscaledDPI", 96 * DPI),
            ("Gdk/WindowScalingFactor", 2),
        ];
        assert_eq!(scale(&at_200), Some(2.0));
    }

    #[test]
    fn gnome_with_large_text_is_scaled_too() {
        let large_text = [
            ("Xft/DPI", 120 * DPI),
            ("Gdk/UnscaledDPI", 120 * DPI),
            ("Gdk/WindowScalingFactor", 1),
        ];
        assert_eq!(scale(&large_text), Some(1.25));
    }

    #[test]
    fn xfce_at_200_percent_leaves_the_dpi_at_96() {
        let at_200 = [("Xft/DPI", 96 * DPI), ("Gdk/WindowScalingFactor", 2)];
        assert_eq!(scale(&at_200), Some(2.0));
    }

    #[test]
    fn a_dpi_alone_is_the_scale() {
        assert_eq!(scale(&[("Xft/DPI", 144 * DPI)]), Some(1.5));
    }

    #[test]
    fn a_window_scale_multiplies_a_dpi_given_alongside_it() {
        // Measured: both browsers at 4.
        let both = [("Xft/DPI", 192 * DPI), ("Gdk/WindowScalingFactor", 2)];
        assert_eq!(scale(&both), Some(4.0));
    }

    #[test]
    fn kde_at_150_percent_has_only_the_unscaled_dpi_to_say_so() {
        let at_150 = [
            ("Gdk/WindowScalingFactor", 1),
            ("Gdk/UnscaledDPI", 144 * DPI),
        ];
        assert_eq!(scale(&at_150), Some(1.5));
    }

    #[test]
    fn no_setting_and_the_default_dpi_are_a_scale_of_1() {
        assert_eq!(scale(&[]), Some(1.0));
        assert_eq!(scale(&[("Xft/DPI", -1)]), Some(1.0));
    }

    #[test]
    fn either_byte_order_reads_the_same() {
        let at_150 = [("Xft/DPI", 144 * DPI)];
        assert_eq!(desktop_scale(&property(true, &at_150)), Some(1.5));
    }

    #[test]
    fn a_scale_no_desktop_runs_at_is_not_believed() {
        assert_eq!(scale(&[("Xft/DPI", 1)]), None);
        assert_eq!(scale(&[("Gdk/WindowScalingFactor", i32::MAX)]), None);
    }

    #[test]
    fn a_property_cut_anywhere_is_refused_not_panicked_on() {
        let whole = property(false, &[("Xft/DPI", 144 * DPI)]);
        assert_eq!(desktop_scale(&whole), Some(1.5));
        for len in 0..whole.len() {
            assert_eq!(desktop_scale(&whole[..len]), None, "cut at {len}");
        }
    }

    #[test]
    fn nonsense_is_refused_not_panicked_on() {
        assert_eq!(desktop_scale(&[2, 0, 0, 0, 0, 0, 0, 0]), None, "byte order");
        let many = [0, 0, 0, 0, 0, 0, 0, 0, 255, 255, 255, 255];
        assert_eq!(desktop_scale(&many), None, "more settings than there are");
        let long_name = [0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 255, 255];
        assert_eq!(desktop_scale(&long_name), None, "a name past the end");
        let unknown = [0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 9, 0, 0, 0, 0, 0, 0, 0];
        assert_eq!(desktop_scale(&unknown), None, "a type there is not");
    }
}
