//! The keyboard, as in asides: Tab reaches whatever can be clicked (a group,
//! like a segmented control or a list, is one stop its arrows go through),
//! and a ring shows what has the keyboard only after a key moved it there; a
//! click hides it, as in macOS, and a click on a button leaves the keyboard
//! where it was. What the keyboard moves to is scrolled into view. And the
//! scroll areas' bars, which GPUI does not draw.

use std::cell::{Cell, RefCell};

use gpui::{
    canvas, div, point, prelude::*, px, quad, size, App, BorderStyle, Bounds, DispatchPhase,
    ElementId, FocusHandle, HitboxBehavior, KeyDownEvent, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, Pixels, ScrollHandle, Window,
};

use crate::theme::{rgba, Theme};

thread_local! {
    /// The keyboard moved the focus last (Tab, the arrows), not a click:
    /// what has it shows a ring.
    static KEYBOARD: Cell<bool> = const { Cell::new(false) };
    /// What the keyboard just moved to is still to be scrolled into view.
    static REVEAL: Cell<bool> = const { Cell::new(false) };
    /// The window's scroll areas, as last drawn: what the keyboard moves to
    /// is scrolled into view in the one it is in.
    static SCROLL_AREAS: RefCell<Vec<ScrollHandle>> = const { RefCell::new(Vec::new()) };
}

/// The keyboard moved the focus: what has it shows a ring, scrolled into
/// view.
pub fn keyboard_moved() {
    KEYBOARD.set(true);
    REVEAL.set(true);
}

/// A click: rings go. Whether one showed (to draw again).
pub fn clicked() -> bool {
    KEYBOARD.replace(false)
}

/// The ring shows on what has the focus.
pub fn keyboard_focus() -> bool {
    KEYBOARD.get()
}

/// The scroll areas drawn, for what the keyboard moves to to be scrolled
/// into view.
pub fn scroll_areas(areas: impl IntoIterator<Item = ScrollHandle>) {
    SCROLL_AREAS.set(areas.into_iter().collect());
}

/// A control's focus handle, in the tab order: kept under `id` while the
/// control is drawn.
pub fn focus_handle(id: String, window: &mut Window, cx: &mut App) -> FocusHandle {
    let id = ElementId::Name(id.into());
    window
        .use_keyed_state(id, cx, |_, cx| cx.focus_handle().tab_stop(true))
        .read(cx)
        .clone()
}

/// Whether `focus` shows a ring: it has the focus, and the keyboard moved it
/// there.
pub fn ringed(focus: &FocusHandle, window: &Window) -> bool {
    keyboard_focus() && focus.is_focused(window)
}

/// A press on a control that leaves the focus where it was (a button's:
/// Return or Space would press it again).
pub fn keep_focus(_: &MouseDownEvent, window: &mut Window, _: &mut App) {
    window.prevent_default();
}

/// The ring around what has the focus (in a parent with `relative()`),
/// `outset` outside its edge (negative: inside, where a scroll area or a
/// card would cut it), its corners `radius` round as the edge's. Just moved
/// to, it is scrolled into view.
pub fn focus_ring(t: Theme, radius: f32, outset: f32) -> impl IntoElement {
    let ink = t.focus;
    canvas(
        |bounds, window, _| {
            if REVEAL.replace(false) {
                reveal(bounds, window);
            }
        },
        move |bounds, _, window, _| {
            let corner = px((radius + outset).max(2.0));
            window.paint_quad(quad(
                bounds,
                corner,
                rgba(0.0, 0.0, 0.0, 0.0),
                px(2.0),
                ink,
                BorderStyle::Solid,
            ));
        },
    )
    .absolute()
    .top(px(-outset))
    .left(px(-outset))
    .right(px(-outset))
    .bottom(px(-outset))
}

/// Scrolls the area `bounds` is drawn in (as its clip says) to show all of
/// it, or its top.
fn reveal(bounds: Bounds<Pixels>, window: &mut Window) {
    let clip = window.content_mask().bounds;
    // In a scroll area it is clipped to it: inside it across, from its top
    // down. The strip above a page is not.
    let area = SCROLL_AREAS.with_borrow(|areas| {
        areas
            .iter()
            .find(|a| {
                let view = a.bounds();
                clip.left() >= view.left() - px(1.0)
                    && clip.right() <= view.right() + px(1.0)
                    && clip.top() >= view.top() - px(1.0)
            })
            .cloned()
    });
    if let Some(area) = area {
        scroll_into_view(&area, bounds, px(12.0), window);
    }
}

/// Scrolls `area` to show all of `bounds` (drawn in it), `margin` clear of
/// its edges, or its top.
pub fn scroll_into_view(
    area: &ScrollHandle,
    bounds: Bounds<Pixels>,
    margin: Pixels,
    window: &mut Window,
) {
    let (view, offset) = (area.bounds(), area.offset());
    let mut y = offset.y;
    if bounds.bottom() + margin > view.bottom() {
        y -= bounds.bottom() + margin - view.bottom();
    }
    let top = bounds.top() + (y - offset.y);
    if top - margin < view.top() {
        y += view.top() - (top - margin);
    }
    let y = y.clamp(-area.max_offset().y, px(0.0));
    if y != offset.y {
        area.set_offset(point(offset.x, y));
        // Drawn again next frame (`refresh` does nothing while drawing).
        window.request_animation_frame();
    }
}

/// Up and Down (Left and Right, `across`), Home and End, among `count`
/// things side by side, the `at`th one the keyboard's: the one they go to,
/// with no modifier down.
pub fn arrow_step(
    e: &KeyDownEvent,
    at: Option<usize>,
    count: usize,
    across: bool,
) -> Option<usize> {
    let m = &e.keystroke.modifiers;
    if m.platform || m.alt || m.control || m.shift {
        return None;
    }
    step(&e.keystroke.key, at, count, across)
}

/// `key` among `count` things, the `at`th one the keyboard's: the one it
/// goes to, if it is an arrow (Left and Right, `across`; else Up and Down),
/// Home or End. The ends stop it; from none, the first arrow goes to the
/// first, or back to the last, as in a menu.
pub fn step(key: &str, at: Option<usize>, count: usize, across: bool) -> Option<usize> {
    let last = count.checked_sub(1)?;
    let (back, on) = if across {
        ("left", "right")
    } else {
        ("up", "down")
    };
    Some(match (key, at) {
        ("home", _) => 0,
        ("end", _) => last,
        (k, None) if k == on => 0,
        (k, None) if k == back => last,
        (k, Some(i)) if k == on => (i + 1).min(last),
        (k, Some(i)) if k == back => i.saturating_sub(1),
        _ => return None,
    })
}

/// A scroll area's bar, beside it (in a parent with `relative()`, after it):
/// a thin thumb as long as the part that shows, when there is more than
/// shows. Dragged, it scrolls; a click beside it jumps there.
pub fn scrollbar(id: impl Into<ElementId>, handle: &ScrollHandle, t: Theme) -> impl IntoElement {
    let (handle, id) = (handle.clone(), id.into());
    canvas(
        |bounds, window, _| window.insert_hitbox(bounds, HitboxBehavior::Normal),
        move |bounds, hitbox, window, cx| {
            let view = f32::from(handle.bounds().size.height);
            let max = f32::from(handle.max_offset().y);
            if max <= 0.5 || view <= 0.0 {
                return;
            }
            let length = px((view * (view / (view + max))).max(28.0)).min(bounds.size.height);
            let room = bounds.size.height - length;
            let thumb = |offset: Pixels| {
                bounds.origin.y + room * (-f32::from(offset) / max).clamp(0.0, 1.0)
            };
            // Where the thumb was grabbed, from its top, while it is dragged.
            let grab = window.use_keyed_state(id.clone(), cx, |_, _| None::<Pixels>);
            let dragging = grab.read(cx).is_some();
            let hovered = hitbox.is_hovered(window) || dragging;
            let top = thumb(handle.offset().y);
            let width = if hovered { px(7.0) } else { px(5.0) };
            let rect = Bounds::new(
                point(bounds.right() - width - px(3.0), top + px(2.0)),
                size(width, length - px(4.0)),
            );
            let ink = t.primary.opacity(if hovered { 0.38 } else { 0.22 });
            window.paint_quad(quad(rect, px(3.5), ink, px(0.0), ink, BorderStyle::Solid));
            // Scrolled to where the thumb's top goes.
            let scroll_to = {
                let handle = handle.clone();
                move |thumb_top: Pixels| {
                    let along = (f32::from(thumb_top - bounds.origin.y) / f32::from(room).max(1.0))
                        .clamp(0.0, 1.0);
                    handle.set_offset(point(handle.offset().x, px(-(max * along))));
                }
            };
            let (h, g, to) = (hitbox.clone(), grab.clone(), scroll_to.clone());
            window.on_mouse_event(move |e: &MouseDownEvent, phase, window, cx| {
                if phase != DispatchPhase::Bubble || !h.is_hovered(window) {
                    return;
                }
                let y = e.position.y;
                // On the thumb, it is held where it was grabbed; beside it,
                // the thumb comes there.
                let on_thumb = y >= top && y <= top + length;
                let at = if on_thumb { y - top } else { length / 2.0 };
                if !on_thumb {
                    to(y - at);
                }
                g.update(cx, |grab, _| *grab = Some(at));
                cx.stop_propagation();
                window.refresh();
            });
            let (g, to) = (grab.clone(), scroll_to);
            window.on_mouse_event(move |e: &MouseMoveEvent, _, window, cx| {
                if let Some(at) = *g.read(cx) {
                    if e.pressed_button.is_none() {
                        g.update(cx, |grab, _| *grab = None);
                    } else {
                        to(e.position.y - at);
                    }
                    window.refresh();
                }
            });
            window.on_mouse_event(move |_: &MouseUpEvent, _, window, cx| {
                if grab.read(cx).is_some() {
                    grab.update(cx, |grab, _| *grab = None);
                    window.refresh();
                }
            });
        },
    )
    .absolute()
    .top_0()
    .right_0()
    .h_full()
    .w(px(13.0))
}

/// A scroll area filling what is left, with its bar beside it.
pub fn scrolled(
    area: impl IntoElement,
    bar: impl Into<ElementId>,
    scroll: &ScrollHandle,
    t: Theme,
) -> impl IntoElement {
    div()
        .flex_1()
        .min_h_0()
        .relative()
        .child(area)
        .child(scrollbar(bar, scroll, t))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arrows_step_and_stop_at_the_ends() {
        assert_eq!(step("down", None, 3, false), Some(0));
        assert_eq!(step("up", None, 3, false), Some(2));
        assert_eq!(step("down", Some(1), 3, false), Some(2));
        assert_eq!(step("down", Some(2), 3, false), Some(2));
        assert_eq!(step("up", Some(0), 3, false), Some(0));
        assert_eq!(
            (step("home", Some(2), 3, false), step("end", None, 3, false)),
            (Some(0), Some(2))
        );
        // Side by side, Left and Right; the others are not theirs.
        assert_eq!(step("right", Some(0), 3, true), Some(1));
        assert_eq!(step("left", Some(1), 3, true), Some(0));
        assert_eq!(
            (
                step("down", Some(0), 3, true),
                step("right", Some(0), 3, false)
            ),
            (None, None)
        );
        assert_eq!(
            (step("down", None, 0, false), step("a", Some(0), 3, false)),
            (None, None)
        );
    }

    #[test]
    fn a_click_hides_the_ring_the_keyboard_showed() {
        assert!(!keyboard_focus());
        keyboard_moved();
        assert!(keyboard_focus());
        assert!(clicked());
        assert!(!keyboard_focus() && !clicked());
    }
}
