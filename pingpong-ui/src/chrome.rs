//! The window's parts, as asides' window has them (`window.rs`): a sidebar
//! on the glass, its rows on a glass pill that slides to the page picked, a
//! header over each group and a foot; and pages, a strip along the title bar
//! over a column that scrolls, with its bar.
//!
//! The window's root paints nothing: the sidebar tints the glass with
//! `Theme::sidebar`, the page column with `Theme::background` (setup with
//! `Theme::reading`).

use gpui::{div, prelude::*, px, AnyElement, Div, ElementId, FontWeight, ScrollHandle, Stateful};

use crate::focus::{focus_ring, scrolled};
use crate::icon::{icon, IconName};
use crate::motion::Glide;
use crate::theme::{rgba, Metrics, Radius, Theme, Type};

/// The sidebar's column: the title bar's lane (the traffic lights on a Mac;
/// elsewhere the system's title bar is above, and a little air does), then
/// what goes in it.
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
        .child(div().flex_none().h(px(if cfg!(target_os = "macos") {
            Metrics::TOOLBAR
        } else {
            10.0
        })))
}

/// The sidebar's rows, the glass pill behind the one `selected` (its index
/// among those noted with `glide`), sliding to the next.
pub fn nav_list(glide: &Glide, selected: Option<usize>, t: Theme) -> Div {
    div()
        .flex_1()
        .min_h_0()
        .px(px(8.0))
        .pt(px(6.0))
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
        .pb(px(14.0))
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
        .child(toolbar.unwrap_or_else(|| div().flex_none().h(px(Metrics::TOOLBAR))))
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
