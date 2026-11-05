//! Linux under Wayland: a virtual display at the client's mode, made by the
//! desktop's compositor, as SudoVDA makes one on Windows and
//! CGVirtualDisplay on macOS. GNOME's mutter (`mutter`) and KDE's KWin
//! (`kwin`) each make one through an interface of their own; neither the
//! desktop portal (whose virtual source KDE sizes from its own dialog,
//! 1920x1080 by default) nor X11 can. Elsewhere — wlroots desktops, X11
//! sessions — the host streams the screen as it is (`platform`).
//!
//! The client sees a monitor of its own, at its own size, rate and
//! density. On GNOME it is the whole desktop for the session, the host's
//! own monitors off, as on Windows, unless the client or the settings keep
//! them (`mutter_layout`); on KDE it extends the desktop, the host's
//! monitors as they are.

use helio_capture::pipewire::Mode;
use helio_display::DisplayMode;

use crate::remote_input::RemoteSink;

/// A desktop that makes virtual displays.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Desktop {
    Gnome,
    Kde,
}

/// The desktop this session runs, if it makes virtual displays for this
/// program; why not, for the log, when it is KDE but KWin refuses.
pub fn offered() -> Option<Desktop> {
    if crate::mutter::available() {
        return Some(Desktop::Gnome);
    }
    match crate::kwin::available() {
        Ok(()) => Some(Desktop::Kde),
        Err(e) => {
            if desktop_is_kde() {
                tracing::warn!(error = %e, "no virtual display from KWin");
            }
            None
        }
    }
}

fn desktop_is_kde() -> bool {
    std::env::var("XDG_CURRENT_DESKTOP").is_ok_and(|d| d.split(':').any(|d| d == "KDE"))
}

/// A virtual display, for as long as the session has it.
pub enum VirtualDisplay {
    Gnome(crate::mutter::VirtualMonitor),
    Kde(crate::kwin::VirtualOutput),
}

/// A virtual display made, and its stream: its PipeWire node on the
/// session's PipeWire, and the mode to ask the stream for, where the
/// display takes its size from it (mutter's).
pub struct Made {
    pub display: VirtualDisplay,
    pub node: u32,
    pub mode: Option<Mode>,
}

/// How long mutter may take to make its monitor.
const GNOME_WAIT: std::time::Duration = std::time::Duration::from_secs(10);

/// A virtual display on `desktop` in `mode`, at the density of a client
/// display at `scale_pct`; the whole desktop unless `keep_host_displays`.
pub fn make(
    desktop: Desktop,
    mode: DisplayMode,
    scale_pct: u16,
    keep_host_displays: bool,
) -> Result<Made, String> {
    let (width, height) = (mode.width as u32, mode.height as u32);
    match desktop {
        Desktop::Gnome => {
            let (monitor, node) = crate::mutter::VirtualMonitor::open(
                mode,
                !keep_host_displays,
                scale(scale_pct),
                GNOME_WAIT,
            )?;
            Ok(Made {
                display: VirtualDisplay::Gnome(monitor),
                node,
                mode: Some(Mode {
                    width,
                    height,
                    fps: mode.refresh_mhz.div_ceil(1000),
                }),
            })
        }
        Desktop::Kde => {
            let (output, node) = crate::kwin::VirtualOutput::open(width, height, scale(scale_pct))?;
            Ok(Made {
                display: VirtualDisplay::Kde(output),
                node,
                mode: None,
            })
        }
    }
}

impl VirtualDisplay {
    /// The client's input, to the display; `picture` is where the picture
    /// sits in the stream.
    pub fn sink(&self, picture: (u32, u32, u32, u32)) -> RemoteSink {
        match self {
            VirtualDisplay::Gnome(m) => m.sink(picture),
            VirtualDisplay::Kde(o) => o.sink(picture),
        }
    }
}

/// The scale a virtual display takes for a client display at `pct` percent:
/// the same, in the quarter steps desktops offer (125 %, 150 %…), 100 %
/// when the client does not say, at most 300 %.
pub fn scale(pct: u16) -> f64 {
    if pct == 0 {
        return 1.0;
    }
    ((pct as f64 / 25.0).round() * 0.25).clamp(1.0, 3.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_virtual_display_takes_the_clients_density() {
        assert_eq!(scale(200), 2.0, "a Retina Mac");
        assert_eq!(scale(125), 1.25, "Windows at 125 %");
        assert_eq!(scale(0), 1.0, "a client that does not say");
        assert_eq!(scale(90), 1.0, "never below 100 %");
        assert_eq!(scale(110), 1.0, "the nearest quarter");
        assert_eq!(scale(500), 3.0, "at most 300 %");
    }
}
