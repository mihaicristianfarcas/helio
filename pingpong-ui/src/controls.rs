//! The controls the app is built from, as asides' (`controls.rs`): quiet
//! bordered buttons and the soft blue for the one main action, a glass pill
//! behind what is selected (sliding to the next), selects whose menus fade in
//! under them, settings as cards of rows that each say what they do. Every
//! control is reached by Tab and works from the keyboard (`crate::focus`).
//! Each takes the [`Theme`] it paints with, so a view decides its appearance
//! once per frame.

use std::rc::Rc;
use std::time::{Duration, Instant};

use gpui::{
    anchored, canvas, deferred, div, percentage, point, prelude::*, px, svg, Animation,
    AnimationExt, AnyElement, AnyView, App, Bounds, BoxShadow, ClickEvent, Div, ElementId,
    FontWeight, KeyDownEvent, MouseButton, Pixels, Rgba, SharedString, Stateful, Transformation,
    Window,
};

use crate::focus::{arrow_step, focus_handle, focus_ring, keep_focus, keyboard_moved, ringed};
use crate::icon::{icon, IconName, IconSize};
use crate::motion::{ease, fade_in, Fade, Glide, FADE};
use crate::theme::{rgba, Layer, Metrics, Radius, Theme, Type};

type OnClick = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>;
/// What a control calls with its new value.
type Handler<T> = Rc<dyn Fn(T, &mut Window, &mut App)>;

const CLEAR: Rgba = rgba(0.0, 0.0, 0.0, 0.0);

// ---------------------------------------------------------------------------
// Buttons

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum ButtonKind {
    /// Bordered and quiet: most actions.
    #[default]
    Quiet,
    /// asides' soft blue: the one main action on a page or sheet.
    Accent,
    /// Red text on a red wash: removing, stopping.
    Danger,
    /// No chrome until hovered: icon buttons along a strip, inline actions.
    Ghost,
}

#[derive(IntoElement)]
pub struct Button {
    id: ElementId,
    label: Option<SharedString>,
    icon: Option<IconName>,
    kind: ButtonKind,
    theme: Theme,
    disabled: bool,
    large: bool,
    full_width: bool,
    tooltip: Option<SharedString>,
    on_click: Option<OnClick>,
}

pub fn button(id: impl Into<ElementId>, label: impl Into<SharedString>, theme: Theme) -> Button {
    Button {
        id: id.into(),
        label: Some(label.into()),
        icon: None,
        kind: ButtonKind::Quiet,
        theme,
        disabled: false,
        large: false,
        full_width: false,
        tooltip: None,
        on_click: None,
    }
}

/// A square button with only an icon, and a tooltip saying what it does.
pub fn icon_button(
    id: impl Into<ElementId>,
    name: IconName,
    tooltip: impl Into<SharedString>,
    theme: Theme,
) -> Button {
    Button {
        id: id.into(),
        label: None,
        icon: Some(name),
        kind: ButtonKind::Ghost,
        theme,
        disabled: false,
        large: false,
        full_width: false,
        tooltip: Some(tooltip.into()),
        on_click: None,
    }
}

impl Button {
    pub fn kind(mut self, kind: ButtonKind) -> Self {
        self.kind = kind;
        self
    }

    pub fn accent(self) -> Self {
        self.kind(ButtonKind::Accent)
    }

    pub fn danger(self) -> Self {
        self.kind(ButtonKind::Danger)
    }

    pub fn ghost(self) -> Self {
        self.kind(ButtonKind::Ghost)
    }

    pub fn icon(mut self, name: IconName) -> Self {
        self.icon = Some(name);
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    /// 32 pt tall: the main action of a sheet.
    pub fn large(mut self) -> Self {
        self.large = true;
        self
    }

    pub fn full_width(mut self) -> Self {
        self.full_width = true;
        self
    }

    pub fn tooltip(mut self, text: impl Into<SharedString>) -> Self {
        self.tooltip = Some(text.into());
        self
    }

    pub fn on_click(mut self, f: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Self {
        self.on_click = Some(Rc::new(f));
        self
    }
}

impl RenderOnce for Button {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = self.theme;
        // Tab reaches it, and Return or Space press it; a click leaves the
        // focus where it was, so a key pressed later does not press it again.
        let focus =
            (!self.disabled).then(|| focus_handle(format!("{:?}-focus", self.id), window, cx));
        let ring = focus.as_ref().is_some_and(|f| ringed(f, window));
        let height = if self.large { 32.0 } else { Metrics::CONTROL };
        let icon_only = self.label.is_none();
        let (fill, hover, stroke, ink) = match self.kind {
            ButtonKind::Quiet => (
                t.control,
                t.control_hover,
                Some(t.control_stroke),
                t.primary,
            ),
            ButtonKind::Accent => (t.accent, t.accent_hover, None, t.on_accent),
            ButtonKind::Danger => (
                t.danger.opacity(0.14),
                t.danger.opacity(0.22),
                None,
                t.danger,
            ),
            ButtonKind::Ghost => (CLEAR, t.hover, None, t.secondary),
        };
        let ink = if self.disabled {
            ink.opacity(0.45)
        } else {
            ink
        };
        let radius = if self.large {
            Radius::ROW
        } else {
            Radius::CONTROL
        };
        let icon_size = if icon_only { IconSize::REGULAR } else { 14.0 };
        let mut el = div()
            .id(self.id)
            .relative()
            .flex_none()
            .h(px(height))
            .when(icon_only, |d| d.w(px(height)).justify_center())
            .when(!icon_only, |d| {
                d.px(px(if self.large { 14.0 } else { 10.0 }))
            })
            .when(self.full_width, |d| d.w_full().justify_center())
            .flex()
            .items_center()
            .gap(px(6.0))
            .rounded(px(radius))
            .bg(fill)
            .when_some(stroke, |d, s| d.border_1().border_color(s))
            .text_size(px(if self.large { Type::BODY } else { 12.0 }))
            .font_weight(if self.kind == ButtonKind::Accent {
                FontWeight::SEMIBOLD
            } else {
                FontWeight::MEDIUM
            })
            .text_color(ink)
            .whitespace_nowrap()
            .when_some(self.icon, |d, name| d.child(icon(name, icon_size, ink)))
            .when_some(self.label, |d, l| d.child(l));
        if !self.disabled {
            let on_click = self.on_click.clone();
            el = el
                .cursor_pointer()
                .hover(move |s| s.bg(hover))
                .active(|s| s.opacity(0.82))
                .when_some(focus, |d, f| {
                    d.track_focus(&f)
                        .on_mouse_down(MouseButton::Left, keep_focus)
                })
                .when_some(on_click, |d, f| d.on_click(move |e, w, cx| f(e, w, cx)));
        }
        if let Some(tip) = self.tooltip {
            el = el.tooltip(move |_, cx| tooltip(tip.clone(), cx));
        }
        el.when(ring, |d| d.child(focus_ring(t, radius, 2.0)))
    }
}

// ---------------------------------------------------------------------------
// Tooltips

pub struct Tooltip {
    text: SharedString,
}

impl Render for Tooltip {
    fn render(&mut self, window: &mut Window, _cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let t = Theme::of(window);
        let tip = div()
            .font_family(crate::ui_font())
            .max_w(px(300.0))
            .px(px(8.0))
            .py(px(5.0))
            .rounded(px(Radius::CONTROL))
            .bg(t.floating)
            .border_1()
            .border_color(t.floating_stroke)
            .shadow(lift(0.25))
            .text_size(px(Type::META + 0.5))
            .text_color(t.secondary)
            .child(self.text.clone());
        fade_in(tip, "tooltip", 0.0)
    }
}

pub fn tooltip(text: impl Into<SharedString>, cx: &mut App) -> AnyView {
    let text = text.into();
    cx.new(|_| Tooltip { text }).into()
}

// ---------------------------------------------------------------------------
// Switch

#[derive(IntoElement)]
pub struct Switch {
    id: ElementId,
    on: bool,
    disabled: bool,
    theme: Theme,
    on_toggle: Option<Handler<bool>>,
}

pub fn switch(id: impl Into<ElementId>, on: bool, theme: Theme) -> Switch {
    Switch {
        id: id.into(),
        on,
        disabled: false,
        theme,
        on_toggle: None,
    }
}

impl Switch {
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn on_toggle(mut self, f: impl Fn(bool, &mut Window, &mut App) + 'static) -> Self {
        self.on_toggle = Some(Rc::new(f));
        self
    }
}

impl RenderOnce for Switch {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = self.theme;
        let on = self.on;
        // Off, a control's surface: on glass, a faint track disappears.
        let track = if on {
            t.fresh.opacity(if t.dark { 0.72 } else { 0.85 })
        } else {
            t.control_hover
        };
        let knob = if t.dark {
            t.primary
        } else {
            rgba(1.0, 1.0, 1.0, 1.0)
        };
        let toggle = self.on_toggle.clone();
        // Tab reaches it; Space or Return turns it over.
        let focus =
            (!self.disabled).then(|| focus_handle(format!("{:?}-focus", self.id), window, cx));
        let ring = focus.as_ref().is_some_and(|f| ringed(f, window));
        div()
            .id(self.id)
            .relative()
            .flex_none()
            .w(px(30.0))
            .h(px(18.0))
            .p(px(2.0))
            .rounded(px(9.0))
            .bg(track)
            .when(!on, |d| {
                d.border_1().border_color(t.control_stroke).p(px(1.0))
            })
            .flex()
            .when(on, |d| d.justify_end())
            .when(self.disabled, |d| d.opacity(0.4))
            .child(
                div()
                    .size(px(14.0))
                    .rounded(px(7.0))
                    .bg(knob)
                    .shadow(lift(0.18)),
            )
            .when_some(focus, |d, f| {
                d.track_focus(&f)
                    .on_mouse_down(MouseButton::Left, keep_focus)
                    .cursor_pointer()
                    .when_some(toggle, |d, f| d.on_click(move |_, w, cx| f(!on, w, cx)))
            })
            .when(ring, |d| {
                d.child(focus_ring(t, 9.0, if on { 2.0 } else { 3.0 }))
            })
    }
}

// ---------------------------------------------------------------------------
// Segmented control

#[derive(IntoElement)]
pub struct Segmented {
    id: SharedString,
    items: Vec<SharedString>,
    selected: usize,
    theme: Theme,
    on_select: Option<Handler<usize>>,
}

pub fn segmented(
    id: impl Into<SharedString>,
    items: impl IntoIterator<Item = impl Into<SharedString>>,
    selected: usize,
    theme: Theme,
) -> Segmented {
    Segmented {
        id: id.into(),
        items: items.into_iter().map(Into::into).collect(),
        selected,
        theme,
        on_select: None,
    }
}

impl Segmented {
    pub fn on_select(mut self, f: impl Fn(usize, &mut Window, &mut App) + 'static) -> Self {
        self.on_select = Some(Rc::new(f));
        self
    }
}

impl RenderOnce for Segmented {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = self.theme;
        let id = self.id.clone();
        // The glass pill slides to the segment picked.
        let glide = Glide::keyed(ElementId::Name(format!("{id}-glide").into()), window, cx);
        // One stop for Tab; the arrows pick the segment beside.
        let focus = focus_handle(format!("{id}-focus"), window, cx);
        let ring = ringed(&focus, window);
        let (count, selected, pick) = (self.items.len(), self.selected, self.on_select.clone());
        div()
            .id(ElementId::Name(self.id))
            .track_focus(&focus)
            .on_mouse_down(MouseButton::Left, keep_focus)
            .on_key_down(move |e, window, cx| {
                let Some(to) = arrow_step(e, Some(selected), count, true) else {
                    return;
                };
                cx.stop_propagation();
                keyboard_moved();
                if let Some(pick) = pick.as_ref().filter(|_| to != selected) {
                    pick(to, window, cx);
                }
            })
            .when(ring, |d| d.child(focus_ring(t, Radius::ROW, 2.0)))
            .relative()
            .flex_none()
            .flex()
            .p(px(2.0))
            .gap(px(2.0))
            .rounded(px(Radius::ROW))
            .bg(t.primary.opacity(0.06))
            .child(glide.layer(
                Some(self.selected),
                t.selected,
                t.selected_stroke,
                Radius::CONTROL - 1.0,
            ))
            .children(self.items.into_iter().enumerate().map(|(i, label)| {
                let on = self.selected == i;
                let select = self.on_select.clone();
                div()
                    .id(ElementId::Name(format!("{id}-{i}").into()))
                    .relative()
                    .child(glide.item(i))
                    .h(px(22.0))
                    .px(px(10.0))
                    .flex()
                    .items_center()
                    .rounded(px(Radius::CONTROL - 1.0))
                    .border_1()
                    .border_color(CLEAR)
                    .text_size(px(12.0))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(if on { t.primary } else { t.secondary })
                    .when(!on, |d| {
                        d.cursor_pointer()
                            .hover(|s| s.text_color(t.primary))
                            .when_some(select, |d, f| d.on_click(move |_, w, cx| f(i, w, cx)))
                    })
                    .child(label)
            }))
    }
}

// ---------------------------------------------------------------------------
// Select: a button that opens a menu of choices

#[derive(Clone)]
pub struct Choice {
    pub label: SharedString,
    pub detail: Option<SharedString>,
    pub disabled: bool,
    /// A hairline above it: it starts a group of its own.
    pub separated: bool,
}

impl Choice {
    pub fn new(label: impl Into<SharedString>) -> Choice {
        Choice {
            label: label.into(),
            detail: None,
            disabled: false,
            separated: false,
        }
    }

    pub fn detail(mut self, detail: impl Into<SharedString>) -> Choice {
        self.detail = Some(detail.into());
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Choice {
        self.disabled = disabled;
        self
    }

    pub fn separated(mut self) -> Choice {
        self.separated = true;
        self
    }
}

impl<S: Into<SharedString>> From<S> for Choice {
    fn from(s: S) -> Choice {
        Choice::new(s)
    }
}

/// The select a UI demo opens (`open=ID` in PING_UI_DEMO), so its menu can be
/// checked and screenshotted without a click.
static DEMO_OPEN: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

/// Open the select with id `id` the next time it is drawn (a UI demo step).
pub fn open_select(id: &str) {
    if let Ok(mut open) = DEMO_OPEN.lock() {
        *open = Some(id.to_string());
    }
}

/// Whether a demo asked for the select `id` to open; asked once.
fn demo_opens(id: &str) -> bool {
    let Ok(mut open) = DEMO_OPEN.lock() else {
        return false;
    };
    let asked = open.as_deref() == Some(id);
    if asked {
        *open = None;
    }
    asked
}

#[derive(IntoElement)]
pub struct Select {
    id: SharedString,
    choices: Vec<Choice>,
    selected: Option<usize>,
    /// What the button says; the selected choice's label by default.
    shown: Option<SharedString>,
    theme: Theme,
    width: Option<f32>,
    disabled: bool,
    chip: bool,
    leading: Option<IconName>,
    on_select: Option<Handler<usize>>,
}

pub fn select(
    id: impl Into<SharedString>,
    choices: impl IntoIterator<Item = impl Into<Choice>>,
    selected: Option<usize>,
    theme: Theme,
) -> Select {
    Select {
        id: id.into(),
        choices: choices.into_iter().map(Into::into).collect(),
        selected,
        shown: None,
        theme,
        width: None,
        disabled: false,
        chip: false,
        leading: None,
        on_select: None,
    }
}

impl Select {
    pub fn width(mut self, w: f32) -> Self {
        self.width = Some(w);
        self
    }

    pub fn shown(mut self, text: impl Into<SharedString>) -> Self {
        self.shown = Some(text.into());
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    /// Borderless and compact: a picker inside a composer or toolbar.
    pub fn chip(mut self) -> Self {
        self.chip = true;
        self
    }

    pub fn leading(mut self, name: IconName) -> Self {
        self.leading = Some(name);
        self
    }

    pub fn on_select(mut self, f: impl Fn(usize, &mut Window, &mut App) + 'static) -> Self {
        self.on_select = Some(Rc::new(f));
        self
    }
}

impl RenderOnce for Select {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = self.theme;
        let key = |part: &str| ElementId::Name(format!("{}-{part}", self.id).into());
        let open = window.use_keyed_state(ElementId::Name(self.id.clone()), cx, |_, _| {
            Fade::new(false)
        });
        if demo_opens(&self.id) {
            open.update(cx, |o, _| *o = Fade::new(true));
        }
        // A press on the button while the menu is open closes it (as a press
        // outside the menu), and its click does not open it again: the
        // button's bounds, and when that press was.
        let button = window.use_keyed_state(key("button"), cx, |_, _| None::<Bounds<Pixels>>);
        let dismissed = window.use_keyed_state(key("dismissed"), cx, |_, _| None::<Instant>);
        // The choice the arrows are on, while it is open (the pointer puts it
        // where it is).
        let lit = window.use_keyed_state(key("lit"), cx, |_, _| None::<usize>);
        let fade = *open.read(cx);
        if fade.moving() {
            window.request_animation_frame();
        }
        let shown = self
            .shown
            .clone()
            .or_else(|| {
                self.selected
                    .and_then(|i| self.choices.get(i))
                    .map(|c| c.label.clone())
            })
            .unwrap_or_else(|| "Choose…".into());
        let ink = if self.disabled {
            t.quaternary
        } else if self.chip {
            t.secondary
        } else {
            t.primary
        };
        let chip = self.chip;
        // Tab reaches it. Space, Return or an arrow opens it; open, the
        // arrows go through its choices, Return or Space picks one, Escape
        // closes it.
        let focus =
            (!self.disabled).then(|| focus_handle(format!("{}-focus", self.id), window, cx));
        let ring = focus.as_ref().is_some_and(|f| ringed(f, window));
        let selected = self.selected;
        let enabled: Vec<bool> = self.choices.iter().map(|c| !c.disabled).collect();
        let keys = {
            let (open, lit, dismissed, pick) = (
                open.clone(),
                lit.clone(),
                dismissed.clone(),
                self.on_select.clone(),
            );
            let count = self.choices.len();
            move |e: &KeyDownEvent, window: &mut Window, cx: &mut App| {
                let is_open = open.read(cx).is_open();
                let key = e.keystroke.key.as_str();
                let at = *lit.read(cx);
                if !is_open {
                    if matches!(key, "up" | "down") && !e.keystroke.modifiers.modified() {
                        lit.update(cx, |l, _| *l = selected);
                        open.update(cx, |o, cx| {
                            o.set(true);
                            cx.notify();
                        });
                        cx.stop_propagation();
                    }
                    return;
                }
                let close = |cx: &mut App| {
                    open.update(cx, |o, cx| {
                        o.set(false);
                        cx.notify();
                    });
                    // The key's release would click the button, opening it
                    // again.
                    dismissed.update(cx, |at, _| *at = Some(Instant::now()));
                };
                match key {
                    "escape" => close(cx),
                    "enter" | "space" => {
                        close(cx);
                        if let (Some(i), Some(pick)) = (at, &pick) {
                            if enabled.get(i).copied().unwrap_or(false) {
                                pick(i, window, cx);
                            }
                        }
                    }
                    // Tab goes on to the next control, the menu closed.
                    "tab" => {
                        open.update(cx, |o, cx| {
                            o.set(false);
                            cx.notify();
                        });
                        return;
                    }
                    _ => match arrow_step(e, at, count, false) {
                        Some(to) => lit.update(cx, |l, cx| {
                            *l = Some(to);
                            cx.notify();
                        }),
                        None => return,
                    },
                }
                cx.stop_propagation();
            }
        };
        let (toggle, just_closed, lit_on_open) = (open.clone(), dismissed.clone(), lit.clone());
        let mut el = div()
            .id(ElementId::Name(self.id.clone()))
            .relative()
            .flex_none()
            .h(px(if chip { 24.0 } else { Metrics::CONTROL }))
            .when_some(self.width, |d, w| d.w(px(w)))
            .px(px(if chip { 7.0 } else { 9.0 }))
            .flex()
            .items_center()
            .gap(px(6.0))
            .rounded(px(Radius::CONTROL))
            .when(!chip, |d| {
                d.bg(t.control).border_1().border_color(t.control_stroke)
            })
            .when(chip && fade.is_open(), |d| d.bg(t.hover))
            .text_size(px(12.0))
            .font_weight(FontWeight::MEDIUM)
            .text_color(ink)
            .when_some(self.leading, |d, name| d.child(icon(name, 13.0, ink)))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .child(shown),
            )
            .child(icon(
                IconName::ChevronUpDown,
                12.0,
                if self.disabled {
                    t.quaternary
                } else {
                    t.tertiary
                },
            ))
            .child({
                let button = button.clone();
                canvas(
                    move |bounds, _, cx| button.update(cx, |b, _| *b = Some(bounds)),
                    |_, _, _, _| {},
                )
                .absolute()
                .top_0()
                .left_0()
                .size_full()
            });
        if let Some(focus) = focus {
            el = el
                .track_focus(&focus)
                .on_key_down(keys)
                .cursor_pointer()
                .hover(move |s| s.bg(if chip { t.hover } else { t.control_hover }))
                .on_click(move |e, _, cx| {
                    if just_closed
                        .update(cx, |at, _| at.take())
                        .is_some_and(|at| at.elapsed() < Duration::from_secs(1))
                    {
                        return;
                    }
                    // Opened from the keyboard, the arrows start at the
                    // choice made.
                    let lit = if e.is_keyboard() { selected } else { None };
                    lit_on_open.update(cx, |l, _| *l = lit);
                    toggle.update(cx, |o, cx| {
                        o.set(!o.is_open());
                        cx.notify();
                    })
                })
                .when(ring, |d| d.child(focus_ring(t, Radius::CONTROL, 2.0)));
        }
        // Open, or still fading out.
        if fade.shown() > 0.0 {
            let lit_now = *lit.read(cx);
            let rows = self.choices.into_iter().enumerate().flat_map(|(i, c)| {
                let (select, close, point_at) = (self.on_select.clone(), open.clone(), lit.clone());
                let line = c.separated.then(|| {
                    div()
                        .mx(px(8.0))
                        .my(px(4.0))
                        .h(px(1.0))
                        .bg(t.hairline)
                        .into_any_element()
                });
                let row = menu_row(ElementId::Name(format!("{}-choice-{i}", self.id).into()), t)
                    .when(lit_now == Some(i), |d| {
                        d.bg(t.selected).border_color(t.selected_stroke)
                    })
                    .when(c.disabled, |d| d.opacity(0.45))
                    .on_hover(move |hovered, _, cx| {
                        if *hovered {
                            point_at.update(cx, |l, cx| {
                                *l = Some(i);
                                cx.notify();
                            });
                        }
                    })
                    .child(
                        div()
                            .w(px(14.0))
                            .flex_none()
                            .when(self.selected == Some(i), |d| {
                                d.child(icon(IconName::Check, 13.0, t.primary))
                            }),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .min_w_0()
                            .child(div().text_color(t.primary).truncate().child(c.label))
                            .when_some(c.detail, |d, detail| {
                                d.child(
                                    div()
                                        .text_size(px(Type::META))
                                        .text_color(t.tertiary)
                                        .child(detail),
                                )
                            }),
                    )
                    .when(!c.disabled, |d| {
                        d.on_click(move |_, w, cx| {
                            close.update(cx, |o, cx| {
                                o.set(false);
                                cx.notify();
                            });
                            if let Some(f) = &select {
                                f(i, w, cx);
                            }
                        })
                    })
                    .into_any_element();
                line.into_iter().chain([row])
            });
            let close = open.clone();
            let shown = fade.shown();
            // Under the button, its left edge on the button's.
            let below = button
                .read(cx)
                .map(|b| point(b.left(), b.bottom() + px(4.0)));
            el = el.child(
                deferred(
                    anchored()
                        .when_some(below, |a, at| a.position(at))
                        .snap_to_window_with_margin(px(8.0))
                        .child(
                            div()
                                .mt(px(if below.is_some() {
                                    0.0
                                } else if chip {
                                    28.0
                                } else {
                                    30.0
                                }) - px(4.0 * (1.0 - shown)))
                                .opacity(shown)
                                .occlude()
                                .child(
                                    floating(t)
                                        .min_w(px(self.width.unwrap_or(180.0).max(180.0)))
                                        .max_w(px(420.0))
                                        .p(px(4.0))
                                        .text_size(px(Type::BODY))
                                        .text_color(t.primary)
                                        .children(rows),
                                )
                                .on_mouse_down_out(move |e, _, cx| {
                                    let closed = close.update(cx, |o, cx| {
                                        let was = o.is_open();
                                        if was {
                                            o.set(false);
                                            cx.notify();
                                        }
                                        was
                                    });
                                    if closed
                                        && button.read(cx).is_some_and(|b| b.contains(&e.position))
                                    {
                                        dismissed.update(cx, |at, _| *at = Some(Instant::now()));
                                    }
                                }),
                        ),
                )
                .priority(Layer::MENU),
            );
        }
        el
    }
}

/// A row in a floating menu: hovered, it lifts as a glass pill.
pub fn menu_row(id: impl Into<ElementId>, t: Theme) -> Stateful<Div> {
    div()
        .id(id)
        .min_h(px(26.0))
        .px(px(8.0))
        .py(px(3.0))
        .flex()
        .items_center()
        .gap(px(8.0))
        .rounded(px(Radius::CONTROL))
        .border_1()
        .border_color(CLEAR)
        .text_size(px(Type::BODY))
        .cursor_pointer()
        .hover(move |s| s.bg(t.selected).border_color(t.selected_stroke))
}

/// A menu of actions at a point (right click), or under a control.
pub struct MenuItem {
    pub label: SharedString,
    pub icon: Option<IconName>,
    pub danger: bool,
    pub separator_before: bool,
    pub on_click: OnClick,
}

impl MenuItem {
    pub fn new(
        label: impl Into<SharedString>,
        f: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> MenuItem {
        MenuItem {
            label: label.into(),
            icon: None,
            danger: false,
            separator_before: false,
            on_click: Rc::new(f),
        }
    }

    pub fn icon(mut self, name: IconName) -> MenuItem {
        self.icon = Some(name);
        self
    }

    pub fn danger(mut self) -> MenuItem {
        self.danger = true;
        self
    }

    pub fn separated(mut self) -> MenuItem {
        self.separator_before = true;
        self
    }
}

/// The menu's panel (place it with `anchored`). It fades in, rising into
/// place, as a select's menu does.
pub fn menu(id: &str, items: Vec<MenuItem>, t: Theme) -> impl IntoElement {
    let panel = floating(t)
        .min_w(px(200.0))
        .p(px(4.0))
        .text_size(px(Type::BODY))
        .children(items.into_iter().enumerate().flat_map(|(i, item)| {
            let color = if item.danger { t.danger } else { t.primary };
            let f = item.on_click.clone();
            let sep = item.separator_before.then(|| {
                div()
                    .my(px(4.0))
                    .mx(px(8.0))
                    .h(px(1.0))
                    .bg(t.hairline)
                    .into_any_element()
            });
            let row = menu_row(ElementId::Name(format!("{id}-{i}").into()), t)
                .text_color(color)
                .child(
                    div()
                        .w(px(16.0))
                        .flex_none()
                        .when_some(item.icon, |d, n| d.child(icon(n, 14.0, color))),
                )
                .child(item.label)
                .on_click(move |e, w, cx| f(e, w, cx))
                .into_any_element();
            sep.into_iter().chain(std::iter::once(row))
        }));
    fade_in(panel, ElementId::Name(format!("{id}-in").into()), 4.0)
}

// ---------------------------------------------------------------------------
// Surfaces

/// Menus, sheets and tooltips: a raised panel, its shadow soft and close (a
/// deep one read as heavy). Solid, so the shadow has nothing to darken under
/// it.
pub fn floating(t: Theme) -> Div {
    div()
        .flex()
        .flex_col()
        .rounded(px(Radius::PANEL))
        .bg(t.floating)
        .border_1()
        .border_color(t.floating_stroke)
        .shadow(vec![BoxShadow {
            color: rgba(0.0, 0.0, 0.0, if t.dark { 0.18 } else { 0.07 }).into(),
            offset: point(px(0.0), px(4.0)),
            blur_radius: px(14.0),
            spread_radius: px(0.0),
            inset: false,
        }])
        .font_family(crate::ui_font())
}

pub fn lift(alpha: f32) -> Vec<BoxShadow> {
    vec![BoxShadow {
        color: rgba(0.0, 0.0, 0.0, alpha).into(),
        offset: point(px(0.0), px(1.0)),
        blur_radius: px(3.0),
        spread_radius: px(0.0),
        inset: false,
    }]
}

/// A modal sheet over the window: a scrim and a centred panel, fading in
/// together, the panel rising into place.
pub fn sheet(
    id: impl Into<ElementId>,
    width: f32,
    t: Theme,
    content: impl IntoElement,
) -> impl IntoElement {
    let id = id.into();
    deferred(
        div()
            .id(id.clone())
            .absolute()
            .inset_0()
            .occlude()
            .bg(t.scrim)
            .flex()
            .items_center()
            .justify_center()
            .child(
                floating(t)
                    .rounded(px(Radius::SHEET))
                    .w(px(width))
                    .p(px(24.0))
                    .text_size(px(Type::BODY))
                    .text_color(t.primary)
                    .child(content)
                    .with_animation(
                        ElementId::Name(format!("{id:?}-rise").into()),
                        Animation::new(FADE).with_easing(ease),
                        |d, delta| d.mt(px(6.0 * (1.0 - delta))),
                    ),
            )
            .with_animation(
                ElementId::Name(format!("{id:?}-in").into()),
                Animation::new(FADE).with_easing(ease),
                |d, delta| d.opacity(delta),
            ),
    )
    .priority(Layer::SHEET)
}

/// A titled group of rows (a settings section).
pub fn section(title: impl Into<SharedString>, t: Theme) -> Div {
    let title: SharedString = title.into();
    div()
        .flex()
        .flex_col()
        .gap(px(6.0))
        .when(!title.is_empty(), |d| {
            d.child(
                div()
                    .px(px(2.0))
                    .text_size(px(Type::META))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(t.tertiary)
                    .child(title),
            )
        })
}

/// The card a section's rows sit in.
pub fn card(t: Theme) -> Div {
    div()
        .flex()
        .flex_col()
        .rounded(px(Radius::ROW + 1.0))
        .border_1()
        .border_color(t.card_stroke)
        .bg(t.card)
        .overflow_hidden()
}

/// A card of rows, hairlines between them.
pub fn rows(rows: impl IntoIterator<Item = AnyElement>, t: Theme) -> Div {
    let mut out = card(t);
    for (i, row) in rows.into_iter().enumerate() {
        if i > 0 {
            out = out.child(div().mx(px(12.0)).h(px(1.0)).bg(t.hairline));
        }
        out = out.child(row);
    }
    out
}

/// A setting: its name and a line on what it does, the control at the
/// right.
pub fn setting(
    label: impl Into<SharedString>,
    detail: Option<SharedString>,
    control: impl IntoElement,
    t: Theme,
) -> Div {
    setting_row(label_stack(label, detail, t), control)
}

/// A setting whose line says what the chosen option does: as tall as the
/// longest of `details`, so the row keeps its height as the choice changes.
pub fn choice_setting(
    label: impl Into<SharedString>,
    details: impl IntoIterator<Item = SharedString>,
    chosen: Option<usize>,
    control: impl IntoElement,
    t: Theme,
) -> Div {
    // All of them in one grid cell, only the chosen one drawn: the cell is
    // as tall as the tallest.
    let lines = details.into_iter().enumerate().map(|(i, detail)| {
        div()
            .row_start(1)
            .col_start(1)
            .when(Some(i) != chosen, |d| d.invisible())
            .child(detail)
    });
    setting_row(
        label_stack(label, None, t).child(detail_line(t).grid().grid_cols(1).children(lines)),
        control,
    )
}

fn setting_row(labels: Div, control: impl IntoElement) -> Div {
    div()
        .min_h(px(Metrics::SETTINGS_ROW))
        .px(px(12.0))
        .py(px(8.0))
        .flex()
        .items_center()
        .justify_between()
        .gap(px(16.0))
        .child(labels)
        .child(
            div()
                .flex_none()
                .flex()
                .items_center()
                .gap(px(8.0))
                .child(control),
        )
}

pub fn label_stack(label: impl Into<SharedString>, detail: Option<SharedString>, t: Theme) -> Div {
    div()
        .flex_1()
        .min_w_0()
        .flex()
        .flex_col()
        .gap(px(2.0))
        .child(
            div()
                .text_size(px(Type::BODY))
                .font_weight(FontWeight::MEDIUM)
                .text_color(t.primary)
                .child(label.into()),
        )
        .when_some(detail, |d, detail| d.child(detail_line(t).child(detail)))
}

/// The small print under a name.
fn detail_line(t: Theme) -> Div {
    div()
        .text_size(px(Type::META))
        .line_height(px(15.0))
        .text_color(t.tertiary)
}

/// An icon on a quiet square: the mark at the start of a row.
pub fn tile(name: IconName, t: Theme) -> Div {
    div()
        .flex_none()
        .size(px(28.0))
        .rounded(px(Radius::ROW))
        .bg(t.primary.opacity(0.06))
        .flex()
        .items_center()
        .justify_center()
        .child(icon(name, 15.0, t.secondary))
}

/// A choice among several, as a row: its icon, what it is, and a round mark
/// where it is chosen.
pub fn choice_row(
    id: impl Into<ElementId>,
    t: Theme,
    glyph: IconName,
    label: impl Into<SharedString>,
    detail: impl Into<SharedString>,
    chosen: bool,
) -> Stateful<Div> {
    let mark = div()
        .flex_none()
        .size(px(18.0))
        .rounded_full()
        .flex()
        .items_center()
        .justify_center()
        .when(chosen, |d| {
            d.bg(t.accent)
                .child(icon(IconName::Check, 11.0, t.on_accent))
        })
        .when(!chosen, |d| d.border_1().border_color(t.control_stroke));
    div()
        .id(id)
        .min_h(px(Metrics::SETTINGS_ROW))
        .px(px(12.0))
        .py(px(8.0))
        .flex()
        .items_center()
        .gap(px(12.0))
        .cursor_pointer()
        .hover(move |s| s.bg(t.hover))
        .child(tile(glyph, t))
        .child(label_stack(label, Some(detail.into()), t))
        .child(mark)
}

/// A page's title, a line under it, and actions at the right.
pub fn page_header(
    title: impl Into<SharedString>,
    subtitle: Option<SharedString>,
    trailing: Option<AnyElement>,
    t: Theme,
) -> Div {
    div()
        .flex()
        .items_center()
        .justify_between()
        .gap(px(16.0))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap(px(3.0))
                .child(
                    div()
                        .text_size(px(Type::DISPLAY))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(t.primary)
                        .child(title.into()),
                )
                .when_some(subtitle, |d, s| {
                    d.child(
                        div()
                            .text_size(px(Type::BODY))
                            .text_color(t.tertiary)
                            .child(s),
                    )
                }),
        )
        .when_some(trailing, |d, e| {
            d.child(
                div()
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(e),
            )
        })
}

/// Small print under a card.
pub fn footnote(text: impl Into<SharedString>, t: Theme) -> Div {
    div()
        .px(px(2.0))
        .text_size(px(Type::META))
        .line_height(px(15.0))
        .text_color(t.tertiary)
        .child(text.into())
}

pub fn hairline(t: Theme) -> Div {
    div().flex_none().h(px(1.0)).w_full().bg(t.hairline)
}

// ---------------------------------------------------------------------------
// Status

/// A compact label in a quiet pill (a state chip).
pub fn chip(label: impl Into<SharedString>, tint: Rgba, _t: Theme) -> Div {
    div()
        .flex_none()
        .px(px(6.0))
        .py(px(1.0))
        .rounded(px(Radius::CHIP))
        .bg(tint.opacity(0.14))
        .text_size(px(Type::META))
        .line_height(px(15.0))
        .font_weight(FontWeight::MEDIUM)
        .text_color(tint)
        .whitespace_nowrap()
        .child(label.into())
}

pub fn dot(color: Rgba, size: f32) -> Div {
    div()
        .flex_none()
        .size(px(size))
        .rounded(px(size / 2.0))
        .bg(color)
}

/// A status dot with a soft halo, for "live". Still: a halo that pulsed
/// asked for a frame at the display's rate for as long as it showed, beside
/// a stream too.
pub fn live_dot(color: Rgba) -> impl IntoElement {
    div()
        .relative()
        .flex_none()
        .size(px(14.0))
        .flex()
        .items_center()
        .justify_center()
        .child(
            div()
                .absolute()
                .size(px(14.0))
                .rounded(px(7.0))
                .bg(color.opacity(0.25)),
        )
        .child(dot(color, 7.0))
}

/// Work in progress: a turning arrow.
pub fn spinner(id: impl Into<ElementId>, size: f32, color: Rgba) -> impl IntoElement {
    svg()
        .path(IconName::Refresh.path())
        .flex_none()
        .size(px(size))
        .text_color(color)
        .with_animation(
            id,
            Animation::new(Duration::from_millis(900)).repeat(),
            |s, delta| s.with_transformation(Transformation::rotate(percentage(delta))),
        )
}

/// A key or chord, as a keycap.
pub fn kbd(keys: impl Into<SharedString>, t: Theme) -> Div {
    div()
        .flex_none()
        .h(px(20.0))
        .px(px(6.0))
        .flex()
        .items_center()
        .rounded(px(Radius::CHIP))
        .border_1()
        .border_color(t.control_stroke)
        .bg(t.control)
        .text_size(px(Type::META + 0.5))
        .font_weight(FontWeight::MEDIUM)
        .text_color(t.secondary)
        .child(keys.into())
}

/// A statistic: a large number and what it counts.
pub fn stat(value: impl Into<SharedString>, label: impl Into<SharedString>, t: Theme) -> Div {
    div()
        .flex_1()
        .min_w(px(96.0))
        .flex()
        .flex_col()
        .gap(px(2.0))
        .px(px(12.0))
        .py(px(10.0))
        .child(
            div()
                .text_size(px(18.0))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(t.primary)
                .child(value.into()),
        )
        .child(
            div()
                .text_size(px(Type::META))
                .text_color(t.tertiary)
                .child(label.into()),
        )
}

/// A message in a tinted card: success, warning, failure.
pub fn notice(icon_name: IconName, tint: Rgba, text: impl Into<SharedString>, t: Theme) -> Div {
    let ink = t.ink(tint);
    div()
        .flex()
        .items_start()
        .gap(px(8.0))
        .px(px(12.0))
        .py(px(9.0))
        .rounded(px(Radius::ROW))
        .bg(tint.alpha(if t.dark { 0.10 } else { 0.09 }))
        .text_size(px(12.0))
        .line_height(px(17.0))
        .text_color(t.primary.opacity(if t.dark { 0.88 } else { 0.8 }))
        .child(div().pt(px(1.0)).child(icon(icon_name, 14.0, ink)))
        .child(div().flex_1().min_w_0().child(text.into()))
}

// ---------------------------------------------------------------------------
// Slider and stepper

struct SliderState {
    bounds: Option<Bounds<Pixels>>,
    dragging: bool,
}

/// A horizontal slider over 0..=1.
#[derive(IntoElement)]
pub struct Slider {
    id: ElementId,
    value: f32,
    width: f32,
    theme: Theme,
    disabled: bool,
    on_change: Option<Handler<f32>>,
}

pub fn slider(id: impl Into<ElementId>, value: f32, width: f32, theme: Theme) -> Slider {
    Slider {
        id: id.into(),
        value: value.clamp(0.0, 1.0),
        width,
        theme,
        disabled: false,
        on_change: None,
    }
}

impl Slider {
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn on_change(mut self, f: impl Fn(f32, &mut Window, &mut App) + 'static) -> Self {
        self.on_change = Some(Rc::new(f));
        self
    }
}

impl RenderOnce for Slider {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = self.theme;
        let state = window.use_keyed_state(self.id.clone(), cx, |_, _| SliderState {
            bounds: None,
            dragging: false,
        });
        const KNOB: f32 = 14.0;
        /// How far an arrow key moves it.
        const KEY_STEP: f32 = 0.05;
        let value_at = |bounds: Bounds<Pixels>, x: Pixels| -> f32 {
            let travel = f32::from(bounds.size.width) - KNOB;
            ((f32::from(x - bounds.origin.x) - KNOB / 2.0) / travel.max(1.0)).clamp(0.0, 1.0)
        };
        let fill_color = if self.disabled {
            t.primary.opacity(0.18)
        } else {
            t.primary.opacity(0.55)
        };
        let knob_color = if t.dark {
            t.primary
        } else {
            rgba(1.0, 1.0, 1.0, 1.0)
        };
        // One stop for Tab: the arrows move it.
        let focus =
            (!self.disabled).then(|| focus_handle(format!("{:?}-focus", self.id), window, cx));
        let ring = focus.as_ref().is_some_and(|f| ringed(f, window));
        let (s1, s2, s3) = (state.clone(), state.clone(), state.clone());
        let (f1, f2, f3) = (
            self.on_change.clone(),
            self.on_change.clone(),
            self.on_change.clone(),
        );
        let value = self.value;
        let disabled = self.disabled;
        div()
            .id(self.id)
            .relative()
            .flex_none()
            .w(px(self.width))
            .h(px(18.0))
            .flex()
            .items_center()
            .when(disabled, |d| d.opacity(0.5))
            .child(
                canvas(
                    move |bounds, _, cx| s1.update(cx, |s, _| s.bounds = Some(bounds)),
                    move |_, _, window, _| {
                        // Follow the pointer anywhere while dragging.
                        window.on_mouse_event(
                            move |e: &gpui::MouseMoveEvent, phase, window, cx| {
                                if phase != gpui::DispatchPhase::Bubble {
                                    return;
                                }
                                let (dragging, bounds) = {
                                    let s = s2.read(cx);
                                    (s.dragging, s.bounds)
                                };
                                if !dragging || e.pressed_button != Some(MouseButton::Left) {
                                    return;
                                }
                                if let (Some(b), Some(f)) = (bounds, &f1) {
                                    f(value_at(b, e.position.x), window, cx);
                                }
                            },
                        );
                        let s = s3.clone();
                        window.on_mouse_event(move |_: &gpui::MouseUpEvent, _, _, cx| {
                            s.update(cx, |s, _| s.dragging = false);
                        });
                    },
                )
                .absolute()
                .inset_0(),
            )
            .child(
                div()
                    .mx(px(KNOB / 2.0))
                    .flex_1()
                    .h(px(4.0))
                    .rounded(px(2.0))
                    .bg(t.primary.opacity(0.10))
                    .child(
                        div()
                            .h_full()
                            .w(gpui::relative(self.value))
                            .rounded(px(2.0))
                            .bg(fill_color),
                    ),
            )
            .child(
                div()
                    .absolute()
                    .top(px(2.0))
                    .left(px(self.value * (self.width - KNOB)))
                    .size(px(KNOB))
                    .rounded(px(KNOB / 2.0))
                    .bg(knob_color)
                    .border_1()
                    .border_color(rgba(0.0, 0.0, 0.0, 0.12))
                    .shadow(lift(0.25))
                    .when(ring, |d| d.child(focus_ring(t, KNOB / 2.0, 3.0))),
            )
            .when_some(focus, |d, focus| {
                d.track_focus(&focus)
                    .on_key_down(move |e, window, cx| {
                        let dir = match e.keystroke.key.as_str() {
                            "left" | "down" => -1.0,
                            "right" | "up" => 1.0,
                            _ => return,
                        };
                        cx.stop_propagation();
                        keyboard_moved();
                        if let Some(f) = &f3 {
                            f((value + dir * KEY_STEP).clamp(0.0, 1.0), window, cx);
                        }
                    })
                    .cursor_pointer()
                    .on_mouse_down(MouseButton::Left, move |e, window, cx| {
                        window.prevent_default();
                        let bounds = state.update(cx, |s, _| {
                            s.dragging = true;
                            s.bounds
                        });
                        if let (Some(b), Some(f)) = (bounds, &f2) {
                            f(value_at(b, e.position.x), window, cx);
                        }
                    })
            })
    }
}

/// A number between − and + buttons, for a bounded count.
#[derive(IntoElement)]
pub struct Stepper {
    id: SharedString,
    value: u32,
    range: std::ops::RangeInclusive<u32>,
    step: u32,
    theme: Theme,
    on_change: Handler<u32>,
}

/// − value +: `on_change` gets the new value, within `range`, `step` from
/// the old.
pub fn stepper(
    id: &str,
    value: u32,
    range: std::ops::RangeInclusive<u32>,
    step: u32,
    t: Theme,
    on_change: impl Fn(u32, &mut Window, &mut App) + 'static,
) -> Stepper {
    Stepper {
        id: id.to_string().into(),
        value,
        range,
        step,
        theme: t,
        on_change: Rc::new(on_change),
    }
}

impl RenderOnce for Stepper {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let (t, id, change) = (self.theme, self.id, self.on_change);
        let (lo, hi, value, step) = (
            *self.range.start(),
            *self.range.end(),
            self.value,
            self.step,
        );
        let next = move |dir: i32| {
            if dir < 0 {
                value.saturating_sub(step).max(lo)
            } else {
                value.saturating_add(step).min(hi)
            }
        };
        let side = |dir: i32, glyph: IconName| {
            let change = change.clone();
            let enabled = if dir < 0 { value > lo } else { value < hi };
            div()
                .id(ElementId::Name(format!("{id}-{dir}").into()))
                .size(px(Metrics::CONTROL - 4.0))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(Radius::CONTROL - 1.0))
                .when(enabled, |d| {
                    d.cursor_pointer()
                        .hover(move |s| s.bg(t.control_hover))
                        .on_click(move |_, w, cx| change(next(dir), w, cx))
                })
                .child(icon(
                    glyph,
                    12.0,
                    if enabled { t.secondary } else { t.quaternary },
                ))
        };
        let mut number = div()
            .min_w(px(56.0))
            .flex()
            .justify_center()
            .text_size(px(12.0))
            .font_weight(FontWeight::MEDIUM)
            .text_color(t.primary)
            .child(value.to_string());
        number.text_style().font_features = Some(gpui::FontFeatures(std::sync::Arc::new(vec![(
            "tnum".into(),
            1,
        )])));
        // One stop for Tab: the arrows (or - and =) step it.
        let focus = focus_handle(format!("{id}-focus"), window, cx);
        let ring = ringed(&focus, window);
        let keys = {
            let change = change.clone();
            move |e: &KeyDownEvent, window: &mut Window, cx: &mut App| {
                if e.keystroke.modifiers.modified() && !e.keystroke.modifiers.shift {
                    return;
                }
                let dir = match e.keystroke.key.as_str() {
                    "left" | "down" | "-" => -1,
                    "right" | "up" | "=" | "+" => 1,
                    _ => return,
                };
                cx.stop_propagation();
                keyboard_moved();
                let to = next(dir);
                if to != value {
                    change(to, window, cx);
                }
            }
        };
        div()
            .id(ElementId::Name(format!("{id}-stepper").into()))
            .track_focus(&focus)
            .on_mouse_down(MouseButton::Left, keep_focus)
            .on_key_down(keys)
            .relative()
            .flex_none()
            .h(px(Metrics::CONTROL))
            .flex()
            .items_center()
            .px(px(1.0))
            .rounded(px(Radius::CONTROL))
            .bg(t.control)
            .border_1()
            .border_color(t.control_stroke)
            .child(side(-1, IconName::Minus))
            .child(number)
            .child(side(1, IconName::Plus))
            .when(ring, |d| d.child(focus_ring(t, Radius::CONTROL, 3.0)))
    }
}

/// A page's own strip along the title bar: its actions at the right.
pub fn toolbar_row(trailing: impl IntoIterator<Item = AnyElement>) -> Div {
    div()
        .flex_none()
        .h(px(Metrics::TOOLBAR))
        .px(px(14.0))
        .flex()
        .items_center()
        .justify_end()
        .gap(px(6.0))
        .children(trailing)
}
