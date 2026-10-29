//! How things move, as in asides (`controls.rs`' transitions): what opens and
//! closes fades in 140 ms, rising a few points into place; a width eases in
//! 100 ms; what is selected sits on a glass pill that slides from one item to
//! the next. All eased out (cubic): quick to start, gentle to settle.
//!
//! Each moving thing asks for the next frame only while it moves, so a still
//! window draws nothing. None of it is on the stream's window.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use gpui::{
    canvas, point, prelude::*, px, quad, size, Animation, AnimationElement, AnimationExt, App,
    BorderStyle, Bounds, ElementId, Entity, Pixels, Rgba, Window,
};

/// How long what opens and closes (menus, lists, pages, sheets, tips) takes
/// to fade.
pub const FADE: Duration = Duration::from_millis(140);

/// How long a width takes to ease ([`Grow`]): shorter than a fade, for the
/// edges to be there as the words they go around have faded in. As long,
/// they still closed in on short words for as long again, after the words
/// had changed (asides).
pub const RESIZE: Duration = Duration::from_millis(100);

/// Eased out: quick to start, gentle to settle.
pub fn ease(t: f32) -> f32 {
    1.0 - (1.0 - t.clamp(0.0, 1.0)).powi(3)
}

/// `element` fades in when it first shows under `id`, rising `rise` points
/// into place (its top margin: none of its own).
pub fn fade_in<E: IntoElement + Styled + 'static>(
    element: E,
    id: impl Into<ElementId>,
    rise: f32,
) -> AnimationElement<E> {
    element.with_animation(id, Animation::new(FADE).with_easing(ease), move |el, d| {
        el.opacity(d).mt(px(rise * (1.0 - d)))
    })
}

/// A selection whose glass pill slides from one item to the next: each item
/// notes where it is laid out ([`Glide::item`]), and a layer behind them
/// paints the pill there ([`Glide::layer`]), after.
#[derive(Clone)]
pub struct Glide(Entity<GlideState>);

#[derive(Default)]
pub struct GlideState {
    /// Where each item is in the window, this frame.
    items: HashMap<usize, Bounds<Pixels>>,
    selected: Option<usize>,
    /// Where the pill slides from, and since when; where it was last drawn.
    from: Option<(Bounds<Pixels>, Instant)>,
    last: Option<Bounds<Pixels>>,
}

impl Glide {
    pub fn new(cx: &mut App) -> Glide {
        Glide(cx.new(|_| GlideState::default()))
    }

    /// One kept in the window under `id` (for a control drawn anew each
    /// frame).
    pub fn keyed(id: impl Into<ElementId>, window: &mut Window, cx: &mut App) -> Glide {
        Glide(window.use_keyed_state(id, cx, |_, _| GlideState::default()))
    }

    /// Goes in item `i`, which is `relative()`: notes where it is.
    pub fn item(&self, i: usize) -> impl IntoElement {
        let state = self.0.clone();
        canvas(
            move |bounds, _, cx| {
                state.update(cx, |s, _| {
                    s.items.insert(i, bounds);
                })
            },
            |_, _, _, _| {},
        )
        .absolute()
        .top_0()
        .left_0()
        .size_full()
    }

    /// Goes behind the items (their container's first child): the pill
    /// behind item `selected`.
    pub fn layer(
        &self,
        selected: Option<usize>,
        fill: Rgba,
        stroke: Rgba,
        radius: f32,
    ) -> impl IntoElement {
        let state = self.0.clone();
        canvas(
            |_, _, _| (),
            move |_, _, window, cx| {
                let (rect, moving) = state.update(cx, |s, _| s.step(selected));
                if let Some(r) = rect {
                    window.paint_quad(quad(
                        r,
                        px(radius),
                        fill,
                        px(1.0),
                        stroke,
                        BorderStyle::Solid,
                    ));
                }
                if moving {
                    window.request_animation_frame();
                }
            },
        )
        // It paints where the items are: it takes no room (in a scroll area,
        // none to scroll).
        .absolute()
        .top_0()
        .left_0()
        .size_0()
    }
}

impl GlideState {
    /// Where the pill is now, and whether it is still sliding. Selected anew,
    /// it slides from where it was; the same one moving (scrolled), it
    /// follows.
    fn step(&mut self, selected: Option<usize>) -> (Option<Bounds<Pixels>>, bool) {
        let target = selected.and_then(|i| self.items.get(&i).copied());
        if selected != self.selected {
            self.from = self
                .last
                .filter(|_| target.is_some())
                .map(|last| (last, Instant::now()));
            self.selected = selected;
        }
        let (rect, moving) = match (self.from, target) {
            (Some((from, since)), Some(to)) => {
                let t = since.elapsed().as_secs_f32() / FADE.as_secs_f32();
                (Some(lerp(from, to, ease(t))), t < 1.0)
            }
            _ => (target, false),
        };
        if !moving {
            self.from = None;
        }
        self.last = rect;
        // Measured again next frame.
        self.items.clear();
        (rect, moving)
    }
}

fn lerp(a: Bounds<Pixels>, b: Bounds<Pixels>, t: f32) -> Bounds<Pixels> {
    let l = |x: Pixels, y: Pixels| x + (y - x) * t;
    Bounds::new(
        point(l(a.origin.x, b.origin.x), l(a.origin.y, b.origin.y)),
        size(
            l(a.size.width, b.size.width),
            l(a.size.height, b.size.height),
        ),
    )
}

/// What fades in as it opens and out as it closes: open or not, and since
/// when.
#[derive(Clone, Copy, Debug, Default)]
pub struct Fade {
    open: bool,
    since: Option<Instant>,
}

impl Fade {
    pub fn new(open: bool) -> Fade {
        Fade { open, since: None }
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    pub fn set(&mut self, open: bool) {
        if self.open != open {
            *self = Fade {
                open,
                since: Some(Instant::now()),
            };
        }
    }

    /// Still fading: draw it again next frame.
    pub fn moving(&self) -> bool {
        self.since.is_some_and(|since| since.elapsed() < FADE)
    }

    /// How much of it shows, 0 to 1.
    pub fn shown(&self) -> f32 {
        let t = self.since.map_or(1.0, |since| {
            since.elapsed().as_secs_f32() / FADE.as_secs_f32()
        });
        if self.open {
            ease(t)
        } else {
            1.0 - ease(t)
        }
    }
}

/// A width easing to another ([`RESIZE`] long), a step at a time: what grows
/// in or shrinks away beside others, which move along with it. It changes
/// only when stepped, for its window to take the new size as it is drawn.
#[derive(Clone, Copy, Debug, Default)]
pub struct Grow {
    drawn: f32,
    from: f32,
    to: f32,
    since: Option<Instant>,
}

impl Grow {
    /// At `width`, going nowhere.
    pub fn at(width: f32) -> Grow {
        Grow {
            drawn: width,
            from: width,
            to: width,
            since: None,
        }
    }

    /// Heads for `to`, from the width drawn now.
    pub fn set(&mut self, to: f32) {
        if !self.heads_for(to) {
            *self = Grow {
                drawn: self.drawn,
                from: self.drawn,
                to,
                since: Some(Instant::now()),
            };
        }
    }

    pub fn heads_for(&self, to: f32) -> bool {
        (self.to - to).abs() < 0.01
    }

    /// The width to draw now; whether it changed.
    pub fn step(&mut self) -> bool {
        let Some(since) = self.since else {
            return false;
        };
        let t = since.elapsed().as_secs_f32() / RESIZE.as_secs_f32();
        let before = self.drawn;
        if t >= 1.0 {
            (self.drawn, self.since) = (self.to, None);
        } else {
            self.drawn = self.from + (self.to - self.from) * ease(t);
        }
        self.drawn != before
    }

    pub fn moving(&self) -> bool {
        self.since.is_some()
    }

    pub fn drawn(&self) -> f32 {
        self.drawn
    }

    /// How much of it shows, 0 to 1: growing in from nothing or shrinking
    /// away to nothing, as wide as it is drawn of the most it takes;
    /// otherwise all of it.
    pub fn shown(&self) -> f32 {
        let most = self.from.max(self.to);
        if self.from.min(self.to) > 0.0 {
            1.0
        } else if most > 0.0 {
            self.drawn / most
        } else {
            0.0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn easing_starts_quick_and_settles() {
        assert_eq!((ease(0.0), ease(1.0), ease(2.0)), (0.0, 1.0, 1.0));
        assert!(ease(0.5) > 0.8);
    }

    #[test]
    fn a_fade_opens_and_closes_over_its_time() {
        let mut fade = Fade::new(false);
        assert_eq!((fade.shown(), fade.moving()), (0.0, false));
        fade.set(true);
        assert!(fade.moving() && fade.shown() < 0.5);
        fade.since = Some(Instant::now() - FADE);
        assert_eq!((fade.shown(), fade.moving()), (1.0, false));
        fade.set(false);
        assert!(fade.moving() && fade.shown() > 0.5);
    }

    #[test]
    fn grows_a_step_at_a_time() {
        let mut grow = Grow::default();
        grow.set(90.0);
        assert_eq!((grow.drawn(), grow.shown()), (0.0, 0.0));
        // Halfway, and done: it changes only when stepped.
        grow.since = Some(Instant::now() - RESIZE / 2);
        assert!(grow.step() && grow.drawn() > 45.0 && grow.drawn() < 90.0);
        assert_eq!(grow.shown(), grow.drawn() / 90.0);
        grow.since = Some(Instant::now() - RESIZE);
        assert!(grow.step() && !grow.moving());
        assert_eq!((grow.drawn(), grow.shown()), (90.0, 1.0));
        // From one width to another, it shows all along.
        grow.set(60.0);
        assert_eq!(grow.shown(), 1.0);
        assert!(grow.heads_for(60.0) && !grow.heads_for(0.0));
    }

    #[test]
    fn the_pill_slides_to_the_item_picked_and_stops_there() {
        let at = |x: f32| Bounds::new(point(px(x), px(0.0)), size(px(10.0), px(10.0)));
        let mut glide = GlideState::default();
        glide.items.insert(0, at(0.0));
        assert_eq!(glide.step(Some(0)), (Some(at(0.0)), false));
        // Picked anew: it slides from where it was.
        glide.items.insert(1, at(100.0));
        let (rect, moving) = glide.step(Some(1));
        assert!(moving && rect.is_some_and(|r| r.origin.x < px(100.0)));
        glide.from = glide.from.map(|(from, _)| (from, Instant::now() - FADE));
        glide.items.insert(1, at(100.0));
        assert_eq!(glide.step(Some(1)), (Some(at(100.0)), false));
    }
}
