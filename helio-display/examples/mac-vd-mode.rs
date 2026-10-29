//! Plug in a virtual display of WIDTHxHEIGHT and report its mode in points
//! and pixels.
#[cfg(target_os = "macos")]
fn main() {
    use helio_display::macos::VirtualDisplay;
    use helio_display::DisplayMode;
    use objc2_core_graphics::{CGDisplayBounds, CGDisplayCopyDisplayMode, CGDisplayMode};
    let size = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "1920x1080".into());
    let (w, h): (u16, u16) = size
        .split_once('x')
        .map(|(w, h)| (w.parse().unwrap(), h.parse().unwrap()))
        .unwrap();
    match VirtualDisplay::new(
        DisplayMode {
            width: w,
            height: h,
            refresh_mhz: 60_000,
        },
        "Helio Host",
        false,
        // HiDPI as for a Retina client.
        helio_display::mac_hidpi(w as u32, 200),
    ) {
        Ok(vd) => println!(
            "{size}: {}x{} points, {}x{} pixels",
            CGDisplayBounds(vd.id).size.width,
            CGDisplayBounds(vd.id).size.height,
            CGDisplayMode::pixel_width(CGDisplayCopyDisplayMode(vd.id).as_deref()),
            CGDisplayMode::pixel_height(CGDisplayCopyDisplayMode(vd.id).as_deref())
        ),
        Err(e) => println!("{size}: {e}"),
    }
}
#[cfg(not(target_os = "macos"))]
fn main() {}
