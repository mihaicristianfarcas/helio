//! The window's parts, as asides' window has them (`window.rs`): a sidebar
//! on the glass, its rows on a glass pill that slides to the page picked, a
//! header over each group and a foot; and pages, a strip along the title bar
//! over a column that scrolls, with its bar.
//!
//! The window's root paints nothing: the sidebar tints the glass with
//! `Theme::sidebar`, the page column with `Theme::background` (setup with
//! `Theme::reading`).
//!
//! The strips along the top are the title bar: where nothing is on them,
//! they move the window ([`drag_area`]). On Windows the app draws the
//! caption buttons too ([`caption_buttons`]); Windows itself moves,
//! snaps and maximizes the window from them, as it does from its own title
//! bar (GPUI's `WindowControlArea`).

use gpui::{
    div, prelude::*, px, AnyElement, Div, ElementId, FontWeight, Pixels, ScrollHandle, Stateful,
    Window, WindowControlArea,
};

use crate::focus::{focus_ring, scrolled};
use crate::icon::{icon, IconName};
use crate::motion::Glide;
use crate::theme::{rgba, Metrics, Radius, Theme, Type};

/// The air above the sidebar's rows and below its foot, where nothing else
/// sits there (no traffic lights).
const SIDEBAR_EDGE: f32 = 14.0;
/// The air the rows' list keeps above its first row.
const NAV_TOP: f32 = 6.0;

/// The sidebar's column: the title bar's lane, then what goes in it. On a
/// Mac the lane holds the traffic lights; elsewhere (the caption buttons
/// are over the page on Windows, the system's title bar above on Linux)
/// the first row is as far from the top as the foot's line is from the
/// bottom.
pub fn sidebar(t: Theme) -> Div {
    div()
        .flex_none()
        .w(px(Metrics::SIDEBAR))
        .h_full()
        .flex()
        .flex_col()
        .bg(t.sidebar)
        .border_r_1()
        .border_color(t.hairline)
        .child(drag_area().flex_none().h(px(if cfg!(target_os = "macos") {
            Metrics::TOOLBAR
        } else {
            SIDEBAR_EDGE - NAV_TOP
        })))
}

/// Where the window is moved from: an empty part of a strip along the top.
/// Nothing on it may be clickable: Windows takes a press there for its own
/// (a move, a double-click maximizing).
pub fn drag_area() -> Div {
    div().window_control_area(WindowControlArea::Drag)
}

/// The caption buttons' height, as Windows 11 draws them.
const CAPTION_HEIGHT: Pixels = px(32.0);

/// Windows: minimize, maximize (or restore) and close at the window's top
/// right, over everything (sheets too: the window can always be closed).
/// Windows' own glyphs (Segoe Fluent Icons; Segoe MDL2 Assets before Windows
/// 11), and its own red under the close button. Windows handles the presses
/// and the maximize button's snap layouts. Elsewhere none.
pub fn caption_buttons(window: &Window, t: Theme) -> Option<AnyElement> {
    if !cfg!(windows) {
        return None;
    }
    let font = if crate::glass::windows_build() >= 22000 {
        "Segoe Fluent Icons"
    } else {
        "Segoe MDL2 Assets"
    };
    let ink = if window.is_window_active() {
        t.primary
    } else {
        t.tertiary
    };
    let button = |id: &'static str, glyph: &'static str, area: WindowControlArea| {
        let close = area == WindowControlArea::Close;
        div()
            .id(id)
            // The strip under it moves the window; this one does not.
            .occlude()
            .window_control_area(area)
            .w(px(Metrics::CAPTION_BUTTON))
            .h_full()
            .flex()
            .items_center()
            .justify_center()
            .text_size(px(10.0))
            .text_color(ink)
            .hover(move |s| {
                if close {
                    // Windows 11's close red, with a white glyph.
                    s.bg(crate::theme::hex(0xc42b1c, 1.0))
                        .text_color(gpui::white())
                } else {
                    s.bg(t.hover)
                }
            })
            .active(move |s| {
                if close {
                    s.bg(crate::theme::hex(0xc42b1c, 0.9))
                        .text_color(gpui::white())
                } else {
                    s.bg(t.pressed)
                }
            })
            .child(glyph)
    };
    Some(
        div()
            .absolute()
            .top_0()
            .right_0()
            .h(CAPTION_HEIGHT)
            .flex()
            .font_family(font)
            .child(button(
                "caption-minimize",
                "\u{e921}",
                WindowControlArea::Min,
            ))
            .child(if window.is_maximized() {
                button("caption-restore", "\u{e923}", WindowControlArea::Max)
            } else {
                button("caption-maximize", "\u{e922}", WindowControlArea::Max)
            })
            .child(button(
                "caption-close",
                "\u{e8bb}",
                WindowControlArea::Close,
            ))
            .into_any_element(),
    )
}

/// The sidebar's rows, the glass pill behind the one `selected` (its index
/// among those noted with `glide`), sliding to the next.
pub fn nav_list(glide: &Glide, selected: Option<usize>, t: Theme) -> Div {
    div()
        .flex_1()
        .min_h_0()
        .px(px(8.0))
        .pt(px(NAV_TOP))
        .flex()
        .flex_col()
        .gap(px(2.0))
        .relative()
        .child(glide.layer(selected, t.selected, t.selected_stroke, Radius::ROW))
}

/// A sidebar row: its icon and name, `index` among the rows the pill slides
/// between; `on` is the page shown. Hovered, it stays so once picked, as the
/// pill slides to it. `ring`: the keyboard has it.
#[allow(clippy::too_many_arguments)]
pub fn nav_row(
    id: impl Into<ElementId>,
    label: impl Into<gpui::SharedString>,
    glyph: IconName,
    on: bool,
    index: usize,
    glide: &Glide,
    ring: bool,
    t: Theme,
) -> Stateful<Div> {
    div()
        .id(id)
        .relative()
        .flex_none()
        .h(px(Metrics::NAV_ROW))
        .px(px(8.0))
        .flex()
        .items_center()
        .gap(px(8.0))
        .rounded(px(Radius::ROW))
        .border_1()
        .border_color(rgba(0.0, 0.0, 0.0, 0.0))
        .text_size(px(Type::BODY))
        .text_color(if on {
            t.primary
        } else {
            t.primary.opacity(0.78)
        })
        .child(glide.item(index))
        .hover(|s| s.bg(t.hover))
        .when(!on, |d| d.cursor_pointer())
        .child(icon(glyph, 15.0, if on { t.primary } else { t.secondary }))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .child(label.into()),
        )
        .when(ring, |d| d.child(focus_ring(t, Radius::ROW, 0.0)))
}

/// A sidebar row's count, at its right.
pub fn nav_count(n: usize, t: Theme) -> Div {
    div()
        .text_size(px(Type::META))
        .text_color(t.tertiary)
        .child(n.to_string())
}

/// The name over a group of the sidebar's rows.
pub fn sidebar_header(label: impl Into<gpui::SharedString>, t: Theme) -> Div {
    div()
        .flex_none()
        .pt(px(14.0))
        .pb(px(4.0))
        .px(px(9.0))
        .text_size(px(Type::META))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(t.tertiary)
        .child(label.into())
}

/// The sidebar's foot: a line of small text (the version, or what the app
/// is doing).
pub fn sidebar_foot(t: Theme) -> Div {
    div()
        .flex_none()
        .px(px(17.0))
        .pb(px(SIDEBAR_EDGE))
        .pt(px(6.0))
        .text_size(px(12.0))
        .text_color(t.tertiary)
}

/// A page: the strip along the title bar (`toolbar`, else empty), then its
/// content in a column, scrolling with a bar beside it.
pub fn page(
    id: &'static str,
    scroll: &ScrollHandle,
    toolbar: Option<Div>,
    width: f32,
    t: Theme,
    content: impl IntoElement,
) -> AnyElement {
    div()
        .size_full()
        .flex()
        .flex_col()
        .child(toolbar.unwrap_or_else(|| drag_area().flex_none().h(px(Metrics::TOOLBAR))))
        .child(scrolled(
            div()
                .id(id)
                .size_full()
                .overflow_y_scroll()
                .track_scroll(scroll)
                .child(
                    div()
                        .w_full()
                        .flex()
                        .justify_center()
                        .px(px(28.0))
                        .pb(px(32.0))
                        .child(div().w_full().min_w_0().max_w(px(width)).child(content)),
                ),
            ElementId::Name(format!("{id}-bar").into()),
            scroll,
            t,
        ))
        .into_any_element()
}
