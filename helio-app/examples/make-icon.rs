//! Draws every icon Helio uses from one source, `helio-app/assets/icon.svg`
//! (a lowercase h on Apple's icon grid), with resvg.
//!
//!   cargo run -p helio-app --example make-icon -- OUT_DIR
//!
//! writes:
//!
//! - OUT_DIR/AppIcon.iconset, for macOS's `iconutil`: the whole 1024-pixel
//!   canvas with its margin and shadow, as macOS draws an app's icon. The
//!   app's bundle and Helio Host's both use it (`tools/build-helio-app`).
//! - OUT_DIR/assets, what `helio-app/assets` holds (refresh it with
//!   `cp -R OUT_DIR/assets/. helio-app/assets/`):
//!   - `Helio.ico` (Windows) and `hicolor/` (the Linux icon theme's sizes
//!     and its scalable SVG): the rounded square alone, without the margin
//!     and the shadow, which neither system expects: with them the icon
//!     looks small beside the others on a taskbar or in a launcher;
//!   - `tray.png` and `tray@2x.png`, the host's menu bar mark: the h alone,
//!     black on nothing, which macOS tints as a template image;
//!   - `mark.svg`, the same h as an SVG, the mark on the app's welcome page,
//!     which GPUI tints with the text's colour.
//! - OUT_DIR/site/helio.svg: the rounded square as an SVG, the site's
//!   favicon and the mark on its pages and share image (the site is on the
//!   `landing-page` branch).

use std::path::Path;

use resvg::tiny_skia::{Pixmap, Transform};
use resvg::usvg;

/// The one source, on Apple's 1024-pixel grid.
const SOURCE: &str = include_str!("../assets/icon.svg");

/// The rounded square alone: the 824-pixel body with 16 pixels around it,
/// room for the light edge drawn on its border.
const SQUARE: &str = "84 84 856 856";

/// The h alone. The letter is 500 pixels tall (262 to 762) and centred, so
/// in this frame it fills a little under nine tenths of the height, as
/// macOS's own menu bar marks do.
const MARK: &str = "224 224 576 576";

/// The launcher entry's name, the window's app id on Linux.
const LINUX_ID: &str = "dev.mihaicristian.helio";

/// Windows picks among these: 16 for lists, 20 to 40 for the taskbar at
/// 100 to 200% scale, 48 and 64 for Explorer, 256 for its large views.
const ICO_SIZES: [u32; 8] = [16, 20, 24, 32, 40, 48, 64, 256];

/// The hicolor theme's usual sizes; 48 is the one the specification
/// requires.
const LINUX_SIZES: [u32; 8] = [16, 24, 32, 48, 64, 128, 256, 512];

/// The menu bar mark: 18 points, as macOS's own status items are.
const TRAY_POINTS: u32 = 18;

/// `text` with `from`, which must occur exactly once, replaced by `to`.
fn replace_once(text: &str, from: &str, to: &str) -> String {
    assert_eq!(
        text.matches(from).count(),
        1,
        "icon.svg should hold {from:?} once"
    );
    text.replacen(from, to, 1)
}

/// The source as macOS shows it.
fn mac() -> String {
    SOURCE.to_owned()
}

/// The rounded square, cropped, without the shadow.
fn square() -> String {
    let svg = replace_once(
        SOURCE,
        r#"viewBox="0 0 1024 1024""#,
        &format!(r#"viewBox="{SQUARE}""#),
    );
    replace_once(&svg, r#" filter="url(#shadow)""#, "")
}

/// The h alone, black.
fn template() -> String {
    let start = SOURCE
        .find(r#"<g id="mark""#)
        .expect("icon.svg has a group with the id mark");
    let end = start + SOURCE[start..].find("</g>").expect("the mark group closes") + 4;
    let mark = &SOURCE[start..end];
    let colour = mark
        .find(r#"color=""#)
        .expect("the mark group has a colour")
        + 7;
    let colour_end = colour + mark[colour..].find('"').expect("the colour closes");
    let mark = format!("{}#000000{}", &mark[..colour], &mark[colour_end..]);
    format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="{MARK}">{mark}</svg>
"#
    )
}

/// `svg` drawn `size` pixels square, as a PNG.
fn png(svg: &str, size: u32) -> Vec<u8> {
    let tree = usvg::Tree::from_str(svg, &usvg::Options::default()).expect("the icon parses");
    let mut pixmap = Pixmap::new(size, size).expect("a nonzero size");
    let scale = size as f32 / tree.size().width();
    resvg::render(
        &tree,
        Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    pixmap.encode_png().expect("a PNG encodes")
}

/// An .ico of PNG images, one for each of [`ICO_SIZES`].
fn ico(svg: &str) -> Vec<u8> {
    let images: Vec<(u32, Vec<u8>)> = ICO_SIZES.iter().map(|&s| (s, png(svg, s))).collect();
    let mut ico = vec![0u8, 0, 1, 0];
    ico.extend((images.len() as u16).to_le_bytes());
    let mut offset = 6 + 16 * images.len() as u32;
    for (size, data) in &images {
        let dim = if *size >= 256 { 0 } else { *size as u8 };
        ico.extend([dim, dim, 0, 0]);
        ico.extend(1u16.to_le_bytes());
        ico.extend(32u16.to_le_bytes());
        ico.extend((data.len() as u32).to_le_bytes());
        ico.extend(offset.to_le_bytes());
        offset += data.len() as u32;
    }
    for (_, data) in &images {
        ico.extend(data);
    }
    ico
}

fn write(path: &Path, data: impl AsRef<[u8]>) {
    std::fs::create_dir_all(path.parent().expect("a file in a folder")).unwrap();
    std::fs::write(path, data).unwrap();
}

fn main() {
    let out = std::env::args().nth(1).expect("usage: make-icon OUT_DIR");
    let out = Path::new(&out);

    let mac = mac();
    let iconset = out.join("AppIcon.iconset");
    for size in [16, 32, 128, 256, 512] {
        write(
            &iconset.join(format!("icon_{size}x{size}.png")),
            png(&mac, size),
        );
        write(
            &iconset.join(format!("icon_{size}x{size}@2x.png")),
            png(&mac, size * 2),
        );
    }

    let assets = out.join("assets");
    let square = square();
    write(&assets.join("Helio.ico"), ico(&square));
    let hicolor = assets.join("hicolor");
    for size in LINUX_SIZES {
        write(
            &hicolor.join(format!("{size}x{size}/apps/{LINUX_ID}.png")),
            png(&square, size),
        );
    }
    write(
        &hicolor.join(format!("scalable/apps/{LINUX_ID}.svg")),
        &square,
    );
    let template = template();
    write(&assets.join("tray.png"), png(&template, TRAY_POINTS));
    write(&assets.join("tray@2x.png"), png(&template, TRAY_POINTS * 2));
    write(&assets.join("mark.svg"), &template);

    write(&out.join("site/helio.svg"), &square);
}
