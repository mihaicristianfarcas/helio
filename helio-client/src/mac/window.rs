//! The stream window (main thread): a window of its own, with a
//! layer-hosting view with a CAMetalLayer, and all keyboard and mouse input.
//!
//! Full screen is a second window, made borderless over the whole screen
//! (the camera housing's strip included) above the menu bar, that the view
//! moves into -- as Moonlight's full-screen mode is (`session.cpp`:
//! `SDL_HINT_VIDEO_MAC_FULLSCREEN_SPACES` off), not a Space of its own.
//! macOS takes only such a window straight to the display
//! (`FULL_SCREEN_LEVEL`): on macOS 27 a window in a full-screen Space always
//! has windows over it (its own title bar's, hidden; the menu bar's strip;
//! Notification Center's), and the titled window made borderless keeps
//! layers of its own; either way the window server composites the stream, a
//! refresh or more later to the glass. Helio's own window stays under the
//! stream, Ctrl+Option+Shift+X (a window) or +D (minimised) away.
//!
//! Feels like Moonlight's fullscreen stream:
//! - full screen, the menu bar and Dock are hidden outright (not
//!   auto-hidden), so nothing drops down when the pointer reaches the top
//!   edge;
//! - the system pointer is captured and hidden while streaming, so hot corners
//!   (Quick Note, Mission Control) and screen edges never trigger. The host
//!   draws its pointer into the picture and gets relative motion (Moonlight's
//!   default), or positions after +M. While ⌘⇧ is held the
//!   pointer is the Mac's, so a screenshot shortcut finds a live one
//!   (`shortcut_modifiers`);
//! - the picture fills the area below the notch, at the nearest standard
//!   aspect ratio (`aspect`): exactly at the Mac's default scaling, with 5
//!   rows of black above and below in a scaled desktop;
//! - Ctrl+Option+Shift+Q quits, +S toggles statistics, +Z releases/recaptures
//!   the mouse, +M switches mouse mode, +X full screen, +V types the
//!   clipboard, +D minimises (Moonlight's chords, Option standing for Alt).

use std::cell::{Cell, OnceCell, RefCell};
use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use helio_proto::input::{scancode, Button, InputEvent};
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObjectProtocol, ProtocolObject};
use objc2::{define_class, msg_send, DefinedClass, MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{
    NSApplication, NSApplicationPresentationOptions, NSBackingStoreType, NSColor, NSCursor,
    NSEvent, NSEventModifierFlags, NSMainMenuWindowLevel, NSNormalWindowLevel, NSResponder,
    NSScreen, NSView, NSWindow, NSWindowCollectionBehavior, NSWindowDelegate, NSWindowLevel,
    NSWindowStyleMask,
};
use objc2_core_foundation::{CGPoint, CGRect, CGSize};
use objc2_core_graphics::{CGAssociateMouseAndMouseCursorPosition, CGWarpMouseCursorPosition};
use objc2_foundation::{NSNotification, NSObject, NSPoint, NSRect, NSSize};
use objc2_quartz_core::CAMetalLayer;
use parking_lot::Mutex;

use super::render::{Layout, RenderShared};
use crate::input::InputSender;
use crate::keymap;
pub use crate::pointer::PointerState;
use crate::scroll::{stream_pixels_per_point, Residue};

/// What the view needs, owned by the main thread.
pub struct Handler {
    pub input: InputSender,
    pub render: Arc<RenderShared>,
    pub pointer: Arc<Mutex<PointerState>>,
    /// Forward Command keys: as the Windows key when the user asks for it,
    /// and always to a Mac host (set when the session says what it is).
    pub forward_command: Arc<std::sync::atomic::AtomicBool>,
    /// Stream pixels per point of pointer motion.
    pub pixels_per_point: f64,
    pub on_quit: Box<dyn Fn()>,
    /// The stream window gained (true) or lost (false) focus.
    pub on_focus: Option<Box<dyn Fn(bool)>>,
    /// Ctrl+Option+Shift+T, watching an AI agent: take over or hand back.
    pub on_take_over: Option<Box<dyn Fn()>>,
    held_keys: HashSet<u16>,
    held_buttons: HashSet<u8>,
    ctrl: bool,
    alt: bool,
    shift: bool,
    /// A trackpad's scroll short of a whole stream pixel, and a wheel's
    /// short of a whole unit.
    scroll_residue: Residue,
    wheel_residue: Residue,
    /// Keys swallowed as the last key of a hotkey chord: their key-up must not
    /// be forwarded either.
    swallowed: HashSet<u16>,
    /// The stream holds the Mac's pointer: hidden and cut off from the mouse
    /// (`hold_pointer`), kept here so every hide has its unhide.
    holding: bool,
    /// Whether the first motion since the pointer was held has made sure
    /// holding it took (`motion`), and the extra hides that did.
    hold_checked: bool,
    extra_hides: u32,
    /// ⌘⇧ is held while captured, so the pointer is the Mac's
    /// (`shortcut_modifiers`); true once a key came through, which a system
    /// shortcut's never does.
    shortcut: Option<bool>,
    /// Let go for a system shortcut on ⌘⇧ (a screenshot): taken back once
    /// the stream is under the pointer again (`after_shortcut`).
    shortcut_released: bool,
}

impl Handler {
    pub fn new(
        input: InputSender,
        render: Arc<RenderShared>,
        pointer: Arc<Mutex<PointerState>>,
        forward_command: Arc<std::sync::atomic::AtomicBool>,
        on_quit: Box<dyn Fn()>,
    ) -> Handler {
        Handler {
            input,
            render,
            pointer,
            forward_command,
            pixels_per_point: 2.0,
            on_quit,
            on_focus: None,
            on_take_over: None,
            held_keys: HashSet::new(),
            held_buttons: HashSet::new(),
            ctrl: false,
            alt: false,
            shift: false,
            scroll_residue: Residue::default(),
            wheel_residue: Residue::default(),
            swallowed: HashSet::new(),
            holding: false,
            hold_checked: false,
            extra_hides: 0,
            shortcut: None,
            shortcut_released: false,
        }
    }

    fn captured(&self) -> bool {
        self.pointer.lock().captured
    }

    fn send_key(&mut self, code: u16, down: bool) {
        let forward_command = self
            .forward_command
            .load(std::sync::atomic::Ordering::Relaxed);
        let Some(key) = keymap::key_for(code, forward_command) else {
            return;
        };
        let sc = scancode(key);
        if down {
            if !self.held_keys.insert(sc) {
                return; // already down (auto-repeat is the host's job)
            }
            self.input.send(InputEvent::KeyDown(sc));
        } else if self.held_keys.remove(&sc) {
            self.input.send(InputEvent::KeyUp(sc));
        }
    }

    /// Release everything held on the host: on focus loss, capture release and
    /// quit, so nothing stays stuck down.
    pub fn release_all(&mut self) {
        for sc in self.held_keys.drain() {
            self.input.send(InputEvent::KeyUp(sc));
        }
        for b in self.held_buttons.drain() {
            if let Some(b) = button(b) {
                self.input.mouse_button(b, false);
            }
        }
        self.ctrl = false;
        self.alt = false;
        self.shift = false;
    }

    fn hotkey(&mut self, code: u16) -> bool {
        if !(self.ctrl && self.alt && self.shift) {
            return false;
        }
        match code {
            0x0C => {
                // Q
                self.release_all();
                (self.on_quit)();
            }
            0x01 => {
                // S
                let on = self.render.toggle_overlay();
                tracing::info!(on, "statistics overlay");
            }
            0x06 => {
                // Z
                let captured = !self.captured();
                set_capture(self, captured);
            }
            0x09 => {
                // V: type the Mac's clipboard on the host (Moonlight's).
                self.paste_clipboard();
            }
            0x2E => {
                // M: mouse mode, relative <-> desktop pointer (Moonlight's).
                let mut p = self.pointer.lock();
                let relative = p.toggle_mode();
                tracing::info!(
                    relative,
                    automatic = p.mode_override.is_none(),
                    "mouse mode"
                );
            }
            0x07 => {
                // X: full screen <-> a window. After this event: the change
                // re-enters the view (focus, resize), whose handler is busy.
                dispatch2::DispatchQueue::main().exec_async(toggle_fullscreen);
            }
            0x11 => {
                // T: watching an agent, take over or hand back.
                if self.on_take_over.is_none() {
                    return false;
                }
                self.release_all();
                if let Some(f) = &self.on_take_over {
                    f();
                }
            }
            0x02 => {
                // D. After this event, as X: full screen, the view first
                // goes back into its titled window.
                self.release_all();
                set_capture(self, false);
                dispatch2::DispatchQueue::main().exec_async(minimise);
            }
            _ => return false,
        }
        true
    }

    /// Hide the Mac's pointer and cut it off from the mouse (the stream has
    /// it: motion arrives as deltas), or give it back. Back inside the
    /// window when held, if it was elsewhere: a click must land on the
    /// stream.
    fn hold_pointer(&mut self, hold: bool) {
        if hold == self.holding {
            return;
        }
        self.holding = hold;
        if hold {
            if let Some(window) = MainThreadMarker::new()
                .and_then(|mtm| NSApplication::sharedApplication(mtm).keyWindow())
            {
                if !pointer_inside(&window) {
                    centre_pointer(&window);
                }
            }
            self.hold_checked = false;
            let _ = CGAssociateMouseAndMouseCursorPosition(false);
            NSCursor::hide();
        } else {
            let _ = CGAssociateMouseAndMouseCursorPosition(true);
            for _ in 0..=std::mem::take(&mut self.extra_hides) {
                NSCursor::unhide();
            }
        }
    }

    /// Every screenshot shortcut starts with ⌘⇧ (⌘⇧3/4/5, CleanShot X's), and
    /// a screenshot tool that comes up over a pointer the stream holds can
    /// neither show its crosshair nor move it, even once the pointer is let
    /// go: only letting go before the shortcut works (seen with macOS's and
    /// CleanShot X's). Neither reliably takes the keyboard from the stream,
    /// so that is no sign either.
    ///
    /// So while ⌘⇧ is held the pointer is the Mac's (shown, free, nothing
    /// sent to the host). A key that comes through meanwhile was no system
    /// shortcut -- those never reach an app -- and the stream holds the
    /// pointer again. Let go of ⌘⇧ with none, and a shortcut took it: the
    /// stream lets go of the keyboard too, until a click on it (Moonlight's
    /// way back in).
    fn shortcut_modifiers(&mut self, held: bool) {
        match (held, self.shortcut) {
            (true, None) if self.captured() => {
                self.shortcut = Some(false);
                self.hold_pointer(false);
                tracing::debug!("⌘⇧ held: the pointer is the Mac's");
            }
            (false, Some(key_came_through)) => {
                self.shortcut = None;
                if !key_came_through {
                    tracing::info!("a system shortcut on ⌘⇧: the pointer is the Mac's");
                    set_capture(self, false);
                    self.shortcut_released = true;
                }
            }
            _ => {}
        }
    }

    /// A screenshot tool that comes up takes the keys from the stream, so
    /// letting go of ⌘⇧ often never reaches it: the stream would go on
    /// sending keys and clicks to the host, the pointer the Mac's. So any
    /// event of the stream's that comes after asks the keyboard itself.
    fn notice_shortcut_end(&mut self) {
        if self.shortcut.is_none() {
            return;
        }
        let held = NSEvent::modifierFlags_class();
        if !(held.contains(NSEventModifierFlags::Command)
            && held.contains(NSEventModifierFlags::Shift))
        {
            self.shortcut_modifiers(false);
        }
    }

    /// After a system shortcut, the pointer moved or a key came: the stream
    /// takes the pointer back once nothing else is under it (the tool is
    /// done), Helio has the keyboard and is the active app. A click on the
    /// stream does too.
    fn after_shortcut(&mut self) {
        if !self.shortcut_released || self.captured() {
            return;
        }
        let Some(mtm) = MainThreadMarker::new() else {
            return;
        };
        let app = NSApplication::sharedApplication(mtm);
        let Some(window) = app.keyWindow() else {
            return;
        };
        let under = NSWindow::windowNumberAtPoint_belowWindowWithWindowNumber(
            NSEvent::mouseLocation(),
            0,
            mtm,
        );
        if app.isActive() && under == window.windowNumber() {
            tracing::info!("the stream is under the pointer again; taking it");
            set_capture(self, true);
        }
    }

    /// The Mac's clipboard, typed on the host.
    fn paste_clipboard(&mut self) {
        let Some(text) = clipboard_text() else { return };
        // Ctrl, Alt and Shift are still down on the host: lift them, or the
        // text would type as shortcuts.
        for sc in self.held_keys.drain() {
            self.input.send(InputEvent::KeyUp(sc));
        }
        let chars = self.input.type_text(&text);
        tracing::info!(chars, "clipboard typed on the host");
    }

    fn key(&mut self, event: &NSEvent, down: bool) {
        let code = event.keyCode();
        if down && event.isARepeat() {
            return;
        }
        if down && self.shortcut == Some(false) {
            // A key came through while ⌘⇧ was held: no system shortcut. Or
            // after it, unseen: either way the stream's again.
            self.shortcut = Some(true);
            if self.captured() {
                self.hold_pointer(true);
            }
            self.notice_shortcut_end();
        }
        if down {
            self.after_shortcut();
        }
        if down && self.hotkey(code) {
            self.swallowed.insert(code);
            return;
        }
        if !down && self.swallowed.remove(&code) {
            return;
        }
        if !self.captured() {
            return;
        }
        self.send_key(code, down);
    }

    fn flags_changed(&mut self, event: &NSEvent) {
        let code = event.keyCode();
        let flags = event.modifierFlags();
        let Some(mask) = keymap::modifier_mask(code) else {
            return;
        };
        let down = flags.0 as u64 & mask != 0;
        match code {
            0x3B | 0x3E => self.ctrl = flags.contains(NSEventModifierFlags::Control),
            0x3A | 0x3D => self.alt = flags.contains(NSEventModifierFlags::Option),
            0x38 | 0x3C => self.shift = flags.contains(NSEventModifierFlags::Shift),
            _ => {}
        }
        self.shortcut_modifiers(
            flags.contains(NSEventModifierFlags::Command)
                && flags.contains(NSEventModifierFlags::Shift),
        );
        if !self.captured() {
            return;
        }
        if code == 0x39 {
            // Caps Lock reports its latched state, not the key: tap it.
            self.send_key(code, true);
            self.send_key(code, false);
            return;
        }
        self.send_key(code, down);
    }

    fn motion(&mut self, event: &NSEvent) {
        self.notice_shortcut_end();
        self.after_shortcut();
        let (dx, dy) = (event.deltaX(), event.deltaY());
        let mut p = self.pointer.lock();
        // Captured but not held: ⌘⇧ is down and the pointer is the Mac's.
        if !p.captured || !self.holding {
            return;
        }
        if !self.hold_checked {
            // Held while macOS was still handing Helio the front (back from a
            // screenshot tool), the pointer stayed visible and tied to the
            // mouse. The first motion is the app's: cut it off and hide it
            // again then (the extra hide is undone with the rest).
            self.hold_checked = true;
            let _ = CGAssociateMouseAndMouseCursorPosition(false);
            NSCursor::hide();
            self.extra_hides += 1;
        }
        // Points scaled to pixels; in a game, raw-ish relative motion with
        // the residue kept by the input thread's batcher.
        p.nudge(
            dx * self.pixels_per_point,
            dy * self.pixels_per_point,
            &self.input,
            // The host draws its pointer; the Mac's stays where it is.
            &|_| {},
        );
    }

    fn mouse_button(&mut self, number: isize, down: bool) {
        self.notice_shortcut_end();
        if !self.captured() {
            if down {
                // First click into an uncaptured window recaptures (Moonlight).
                set_capture(self, true);
            }
            return;
        }
        let n = number as u8;
        let Some(b) = button(n) else { return };
        if down {
            if self.held_buttons.insert(n) {
                self.input.mouse_button(b, true);
            }
        } else if self.held_buttons.remove(&n) {
            self.input.mouse_button(b, false);
        }
    }

    /// `area`: the points the picture is fitted into (the view below the
    /// notch).
    fn scroll(&mut self, event: &NSEvent, area: (f64, f64)) {
        if !self.captured() {
            return;
        }
        // The protocol's horizontal wheel is Windows', which runs the other
        // way.
        let (dy, dx) = (event.scrollingDeltaY(), -event.scrollingDeltaX());
        if event.hasPreciseScrollingDeltas() {
            // A trackpad or a Magic Mouse: the points the content moves,
            // glide included, sent as the stream's pixels for the host to
            // scroll as far. Moonlight sends them as notches instead
            // (`mouse.cpp`), which only the host can size: see
            // `helio_input::scroll`.
            let (sw, sh) = self.pointer.lock().stream_size();
            let per_point = stream_pixels_per_point(area, (sw as f64, sh as f64))
                .unwrap_or(self.pixels_per_point);
            let (dv, dh) = self.scroll_residue.whole(dy * per_point, dx * per_point);
            if dv != 0 || dh != 0 {
                self.input.scroll(dv, dh);
            }
        } else {
            // A wheel, in lines: 120 each, as Windows counts a notch.
            let (dv, dh) = self.wheel_residue.whole(dy * 120.0, dx * 120.0);
            if dv != 0 || dh != 0 {
                self.input.wheel(dv, dh);
            }
        }
    }
}

/// The text on the Mac's clipboard, if any.
pub fn clipboard_text() -> Option<String> {
    let text = unsafe {
        objc2_app_kit::NSPasteboard::generalPasteboard()
            .stringForType(objc2_app_kit::NSPasteboardTypeString)
    };
    text.map(|t| t.to_string())
}

fn button(n: u8) -> Option<Button> {
    Some(match n {
        0 => Button::Left,
        1 => Button::Right,
        2 => Button::Middle,
        3 => Button::X1,
        4 => Button::X2,
        _ => return None,
    })
}

/// Capture or release the system pointer.
pub fn set_capture(h: &mut Handler, captured: bool) {
    let mut p = h.pointer.lock();
    if p.captured == captured {
        return;
    }
    p.captured = captured;
    drop(p);
    h.shortcut = None;
    h.shortcut_released = false;
    if !captured {
        h.release_all();
    }
    h.hold_pointer(captured);
    tracing::info!(captured, "pointer capture");
}

pub struct ViewIvars {
    pub handler: RefCell<Option<Handler>>,
    pub render: Arc<RenderShared>,
    top_inset_points: std::cell::Cell<f64>,
}

define_class!(
    #[unsafe(super(NSView, NSResponder, NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "HelioStreamView"]
    #[ivars = ViewIvars]
    pub struct StreamView;

    unsafe impl NSObjectProtocol for StreamView {}

    impl StreamView {
        #[unsafe(method(acceptsFirstResponder))]
        fn accepts_first_responder(&self) -> bool {
            true
        }

        #[unsafe(method(acceptsFirstMouse:))]
        fn accepts_first_mouse(&self, _event: Option<&NSEvent>) -> bool {
            true
        }

        #[unsafe(method(keyDown:))]
        fn key_down(&self, event: &NSEvent) {
            self.with(|h| h.key(event, true));
        }

        #[unsafe(method(keyUp:))]
        fn key_up(&self, event: &NSEvent) {
            self.with(|h| h.key(event, false));
        }

        #[unsafe(method(flagsChanged:))]
        fn flags_changed(&self, event: &NSEvent) {
            self.with(|h| h.flags_changed(event));
        }

        // Cmd-combinations would otherwise go to the menu (Cmd+Q, Cmd+W...).
        // While streaming they belong to the host.
        #[unsafe(method(performKeyEquivalent:))]
        fn perform_key_equivalent(&self, event: &NSEvent) -> bool {
            self.with(|h| h.key(event, true));
            true
        }

        #[unsafe(method(mouseMoved:))]
        fn mouse_moved(&self, event: &NSEvent) {
            self.with(|h| h.motion(event));
        }

        #[unsafe(method(mouseDragged:))]
        fn mouse_dragged(&self, event: &NSEvent) {
            self.with(|h| h.motion(event));
        }

        #[unsafe(method(rightMouseDragged:))]
        fn right_mouse_dragged(&self, event: &NSEvent) {
            self.with(|h| h.motion(event));
        }

        #[unsafe(method(otherMouseDragged:))]
        fn other_mouse_dragged(&self, event: &NSEvent) {
            self.with(|h| h.motion(event));
        }

        #[unsafe(method(mouseDown:))]
        fn mouse_down(&self, event: &NSEvent) {
            self.with(|h| h.mouse_button(event.buttonNumber(), true));
        }

        #[unsafe(method(mouseUp:))]
        fn mouse_up(&self, event: &NSEvent) {
            self.with(|h| h.mouse_button(event.buttonNumber(), false));
        }

        #[unsafe(method(rightMouseDown:))]
        fn right_mouse_down(&self, event: &NSEvent) {
            self.with(|h| h.mouse_button(event.buttonNumber(), true));
        }

        #[unsafe(method(rightMouseUp:))]
        fn right_mouse_up(&self, event: &NSEvent) {
            self.with(|h| h.mouse_button(event.buttonNumber(), false));
        }

        #[unsafe(method(otherMouseDown:))]
        fn other_mouse_down(&self, event: &NSEvent) {
            self.with(|h| h.mouse_button(event.buttonNumber(), true));
        }

        #[unsafe(method(otherMouseUp:))]
        fn other_mouse_up(&self, event: &NSEvent) {
            self.with(|h| h.mouse_button(event.buttonNumber(), false));
        }

        #[unsafe(method(scrollWheel:))]
        fn scroll_wheel(&self, event: &NSEvent) {
            let area = self.picture_area();
            self.with(|h| h.scroll(event, area));
        }

        #[unsafe(method(setFrameSize:))]
        fn set_frame_size(&self, size: NSSize) {
            let _: () = unsafe { msg_send![super(self), setFrameSize: size] };
            self.publish_layout();
        }

        #[unsafe(method(viewDidChangeBackingProperties))]
        fn view_did_change_backing_properties(&self) {
            self.publish_layout();
        }
    }
);

impl StreamView {
    pub fn new(
        mtm: MainThreadMarker,
        frame: NSRect,
        render: Arc<RenderShared>,
        top_inset_points: f64,
    ) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(ViewIvars {
            handler: RefCell::new(None),
            render,
            top_inset_points: std::cell::Cell::new(top_inset_points),
        });
        unsafe { msg_send![super(this), initWithFrame: frame] }
    }

    pub fn set_handler(&self, h: Handler) {
        *self.ivars().handler.borrow_mut() = Some(h);
    }

    pub fn with<R>(&self, f: impl FnOnce(&mut Handler) -> R) -> Option<R> {
        let mut h = self.ivars().handler.try_borrow_mut().ok()?;
        h.as_mut().map(f)
    }

    fn scale(&self) -> f64 {
        self.window().map(|w| w.backingScaleFactor()).unwrap_or(2.0)
    }

    pub fn set_top_inset(&self, points: f64) {
        self.ivars().top_inset_points.set(points);
        self.publish_layout();
    }

    /// Points kept clear at the top (the notch).
    fn top_inset(&self) -> f64 {
        // Full screen, the window covers the camera housing's strip, and the
        // view's own safe area still says 0 (macOS 27.2): the strip is the
        // screen's, at its top edge.
        let housing = self
            .window()
            .and_then(|w| {
                let screen = w.screen()?;
                let (frame, strip) = (screen.frame(), screen.safeAreaInsets().top);
                let window = w.frame();
                Some(
                    window.origin.y + window.size.height
                        - (frame.origin.y + frame.size.height - strip),
                )
            })
            .unwrap_or(0.0);
        self.ivars()
            .top_inset_points
            .get()
            .max(self.safeAreaInsets().top)
            .max(housing)
    }

    /// The points the picture is fitted into: the view below the notch.
    fn picture_area(&self) -> (f64, f64) {
        let size = self.bounds().size;
        let inset = self.top_inset().min(size.height);
        (size.width, size.height - inset)
    }

    /// Tell the renderer the drawable's pixel size and the notch inset, and
    /// keep the layer's contents scale honest.
    pub fn publish_layout(&self) {
        let scale = self.scale();
        let size = self.bounds().size;
        if let Some(layer) = self.layer() {
            layer.setContentsScale(scale);
        }
        let inset = self.top_inset();
        self.ivars().render.set_layout(Layout {
            drawable_w: (size.width * scale).round(),
            drawable_h: (size.height * scale).round(),
            top_inset: (inset * scale).round(),
            // Full screen, the layer goes straight to the display; in a
            // window it is composited.
            windowed: self.window().is_some_and(|w| {
                let style = w.styleMask();
                style.contains(NSWindowStyleMask::Titled)
                    && !style.contains(NSWindowStyleMask::FullScreen)
            }),
            refresh: self
                .window()
                .and_then(|w| w.screen())
                .map_or(0.0, |s| s.minimumRefreshInterval()),
        });
    }
}

// ---------------------------------------------------------------------------

pub struct WindowIvars {
    pub view: Retained<StreamView>,
    /// The titled window the stream opens in, and comes back to from full
    /// screen.
    home: OnceCell<Retained<NSWindow>>,
    /// Full screen: the borderless window over the whole screen that the
    /// view is in (`enter_full_screen`).
    full: RefCell<Option<Retained<NSWindow>>>,
    /// That window off screen, between two spells of full screen.
    spare: RefCell<Option<Retained<NSWindow>>>,
    /// Minimised from full screen: full screen again when it is back.
    full_on_return: Cell<bool>,
}

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "HelioWindowDelegate"]
    #[ivars = WindowIvars]
    pub struct WindowDelegate;

    unsafe impl NSObjectProtocol for WindowDelegate {}

    unsafe impl NSWindowDelegate for WindowDelegate {
        #[unsafe(method(windowDidBecomeKey:))]
        fn window_did_become_key(&self, n: &NSNotification) {
            tracing::info!(key = true, "stream window");
            if let Some(window) = notified_window(n) {
                self.lift(&window, true);
            }
            self.ivars().view.with(|h| {
                if let Some(f) = &h.on_focus {
                    f(true);
                }
            });
            capture_when_settled();
        }

        #[unsafe(method(windowDidResignKey:))]
        fn window_did_resign_key(&self, n: &NSNotification) {
            tracing::info!(key = false, "stream window");
            // Another app, or Helio's own window, comes in front of the
            // stream, and the menu bar and Dock come back.
            if let Some(window) = notified_window(n) {
                self.lift(&window, false);
            }
            // Cmd+Tab away: never leave keys held on the host or the pointer
            // trapped in a window the user has left.
            self.ivars().view.with(|h| {
                set_capture(h, false);
                if let Some(f) = &h.on_focus {
                    f(false);
                }
            });
        }

        // Another display may refresh at another rate, and take the layer
        // another way.
        #[unsafe(method(windowDidChangeScreen:))]
        fn window_did_change_screen(&self, n: &NSNotification) {
            if let Some(window) = notified_window(n).filter(|w| self.is_full(w)) {
                cover_screen(&window);
            }
            self.ivars().view.publish_layout();
            self.ivars().view.ivars().render.new_path();
        }

        // Minimised from full screen (+D): back to it.
        #[unsafe(method(windowDidDeminiaturize:))]
        fn window_did_deminiaturize(&self, _n: &NSNotification) {
            if self.ivars().full_on_return.replace(false) {
                self.enter_full_screen();
            }
        }

        #[unsafe(method(windowShouldClose:))]
        fn window_should_close(&self, _sender: &NSWindow) -> bool {
            self.ivars().view.with(|h| {
                h.release_all();
                (h.on_quit)();
            });
            false
        }

        // Closed some other way (the close button asks first, above): the
        // stream must not go on without its window. After the app closed it
        // itself this says so again, which it ignores.
        #[unsafe(method(windowWillClose:))]
        fn window_will_close(&self, _n: &NSNotification) {
            self.ivars().view.with(|h| (h.on_quit)());
        }
    }
);

impl WindowDelegate {
    pub fn new(mtm: MainThreadMarker, view: Retained<StreamView>) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(WindowIvars {
            view,
            home: OnceCell::new(),
            full: RefCell::new(None),
            spare: RefCell::new(None),
            full_on_return: Cell::new(false),
        });
        unsafe { msg_send![super(this), init] }
    }

    /// The window the view is in: the full-screen one, or its titled one.
    fn current(&self) -> Option<Retained<NSWindow>> {
        self.ivars()
            .full
            .borrow()
            .clone()
            .or_else(|| self.ivars().home.get().cloned())
    }

    fn is_full(&self, window: &NSWindow) -> bool {
        self.ivars()
            .full
            .borrow()
            .as_deref()
            .is_some_and(|f| std::ptr::eq(f, window))
    }

    /// Into full screen, or back into the titled window.
    pub fn toggle_full_screen(&self) {
        if self.ivars().full.borrow().is_some() {
            self.leave_full_screen();
        } else {
            self.enter_full_screen();
        }
    }

    /// Move the view into a new borderless window over the whole screen,
    /// above the menu bar; the titled window waits off screen. A new
    /// window, not the titled one made borderless: that one keeps layers of
    /// its frame's, and the window server then composites the stream.
    fn enter_full_screen(&self) {
        let Some(home) = self.ivars().home.get() else {
            return;
        };
        if self.ivars().full.borrow().is_some() {
            return;
        }
        let mtm = self.mtm();
        let Some(screen) = home.screen().or_else(|| NSScreen::mainScreen(mtm)) else {
            return;
        };
        let spare = self.ivars().spare.borrow_mut().take();
        let full = spare.unwrap_or_else(|| {
            let full: Retained<NSWindow> =
                StreamNSWindow::new(mtm, screen.frame(), NSWindowStyleMask::Borderless)
                    .into_super();
            unsafe { full.setReleasedWhenClosed(false) };
            full.setTitle(&home.title());
            full.setBackgroundColor(Some(&NSColor::blackColor()));
            full.setOpaque(true);
            full.setAcceptsMouseMovedEvents(true);
            full
        });
        full.setFrame_display(screen.frame(), false);
        // Above the menu bar at once, even before it has the keyboard (Helio
        // not yet the active app): what the user started covers the screen.
        full.setLevel(FULL_SCREEN_LEVEL);
        full.setDelegate(Some(ProtocolObject::from_ref(self)));
        let view = &self.ivars().view;
        // Off a window it was shown in: a layer of its own here.
        if home.isVisible() {
            fresh_layer(view);
        }
        full.setContentView(Some(view));
        *self.ivars().full.borrow_mut() = Some(full.clone());
        full.makeKeyAndOrderFront(None);
        full.makeFirstResponder(Some(view));
        home.orderOut(None);
        self.changed_mode(true);
    }

    /// Move the view back into its titled window; the full-screen one waits
    /// off screen for the next time (`spare`).
    fn leave_full_screen(&self) {
        let Some(full) = self.ivars().full.borrow_mut().take() else {
            return;
        };
        let Some(home) = self.ivars().home.get() else {
            return;
        };
        // Its closing is not the stream's end (`windowWillClose:`).
        full.setDelegate(None);
        self.lift(&full, false);
        let view = &self.ivars().view;
        fresh_layer(view);
        home.setContentView(Some(view));
        home.makeKeyAndOrderFront(None);
        home.makeFirstResponder(Some(view));
        full.orderOut(None);
        *self.ivars().spare.borrow_mut() = Some(full);
        self.changed_mode(false);
    }

    /// The full-screen window with the keyboard: above the menu bar, the
    /// menu bar and Dock hidden outright. Without it: among other windows,
    /// so that another app or Helio's own window can come in front.
    fn lift(&self, window: &NSWindow, key: bool) {
        if !self.is_full(window) {
            return;
        }
        window.setLevel(if key {
            FULL_SCREEN_LEVEL
        } else {
            NSNormalWindowLevel
        });
        let options = if key {
            NSApplicationPresentationOptions::HideDock
                | NSApplicationPresentationOptions::HideMenuBar
        } else {
            NSApplicationPresentationOptions::Default
        };
        NSApplication::sharedApplication(self.mtm()).setPresentationOptions(options);
    }

    /// The window server's path for the layer is judged afresh in its new
    /// window (the layout follows the view's resize).
    fn changed_mode(&self, full: bool) {
        self.ivars().view.ivars().render.new_path();
        tracing::info!(fullscreen = full, "window mode");
    }

    /// Into the Dock: full screen, from the titled window, full screen again
    /// when it is back (a borderless window cannot be minimised, and one
    /// that can is composited).
    fn minimise(&self) {
        if self.ivars().full.borrow().is_some() {
            self.leave_full_screen();
            self.ivars().full_on_return.set(true);
        }
        if let Some(window) = self.current() {
            window.miniaturize(None);
        }
    }

    /// Off the screen for good: the full-screen window closed, the titled
    /// one left for its owner to close.
    fn close_full_screen(&self) {
        let full = self.ivars().full.borrow_mut().take();
        let spare = self.ivars().spare.borrow_mut().take();
        for window in full.into_iter().chain(spare) {
            window.setDelegate(None);
            window.orderOut(None);
            window.close();
        }
    }
}

/// Give the stream view a new layer, and the renderer with it, before the
/// view moves to another window. A layer moved from the window it was shown
/// in to a full-screen one is still drawn, but macOS no longer reports its
/// drawables on the glass (`presentedTime` 0, macOS 27.2): 1 of 118 in a
/// test window, 117 of 118 with a new layer. The renderer, its path to the
/// glass and its statistics need those reports.
fn fresh_layer(view: &StreamView) {
    let layer = CAMetalLayer::new();
    layer.setOpaque(true);
    if let Some(old) = view.layer() {
        layer.setContentsScale(old.contentsScale());
    }
    view.setLayer(Some(&layer));
    view.setWantsLayer(true);
    view.ivars().render.replace_layer(layer);
}

/// Fit the full-screen window to its screen.
fn cover_screen(window: &NSWindow) {
    if let Some(screen) = window.screen() {
        window.setFrame_display(screen.frame(), true);
    }
}

/// The window a window notification is about.
fn notified_window(n: &NSNotification) -> Option<Retained<NSWindow>> {
    n.object()?.downcast::<NSWindow>().ok()
}

/// The stream window's style in a window.
const WINDOWED_STYLE: NSWindowStyleMask = NSWindowStyleMask::Titled
    .union(NSWindowStyleMask::Closable)
    .union(NSWindowStyleMask::Miniaturizable)
    .union(NSWindowStyleMask::Resizable);

/// The full-screen window's level while it has the keyboard: above the menu
/// bar's. Lower, a window of the system's lies over it -- Notification
/// Center's, transparent and over the whole screen, at 23 (listed by
/// `CGWindowListCopyWindowInfo` on macOS 27.2) -- and the window server
/// composites the stream to draw it. With `mac-present` full screen on an
/// M4 Pro's 120 Hz panel at 3600x2338, macOS 27.2.1, nothing recording,
/// decoded to glass on average: 24.4 ms at 60 fps and 27.4 ms at 120 in a
/// full-screen Space; 6.9 and 12.6 ms in this window.
const FULL_SCREEN_LEVEL: NSWindowLevel = NSMainMenuWindowLevel + 1;

define_class!(
    #[unsafe(super(NSWindow, NSResponder, NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "HelioStreamWindow"]
    pub struct StreamNSWindow;

    impl StreamNSWindow {
        // A borderless window (full screen) refuses key status by default;
        // this one must take it.
        #[unsafe(method(canBecomeKeyWindow))]
        fn can_become_key_window(&self) -> bool {
            true
        }

        #[unsafe(method(canBecomeMainWindow))]
        fn can_become_main_window(&self) -> bool {
            true
        }

        // The green button, View > Enter Full Screen and Ctrl+Option+Shift+X
        // all come here: Helio's full screen, never a Space.
        #[unsafe(method(toggleFullScreen:))]
        fn toggle_full_screen(&self, _sender: Option<&AnyObject>) {
            let delegate = self.delegate();
            let delegate: Option<&AnyObject> = delegate.as_deref().map(|d| d.as_ref());
            if let Some(d) = delegate.and_then(|d| d.downcast_ref::<WindowDelegate>()) {
                d.toggle_full_screen();
            }
        }

        // Full screen (borderless), the window is its whole screen, whatever
        // AppKit fits it to (another scaling, say).
        #[unsafe(method(constrainFrameRect:toScreen:))]
        fn constrain_frame_rect(&self, frame: NSRect, screen: Option<&NSScreen>) -> NSRect {
            match screen {
                Some(screen) if !self.styleMask().contains(NSWindowStyleMask::Titled) => {
                    screen.frame()
                }
                _ => unsafe { msg_send![super(self), constrainFrameRect: frame, toScreen: screen] },
            }
        }
    }
);

impl StreamNSWindow {
    fn new(mtm: MainThreadMarker, rect: NSRect, style: NSWindowStyleMask) -> Retained<Self> {
        unsafe {
            msg_send![Self::alloc(mtm), initWithContentRect: rect, styleMask: style, backing: NSBackingStoreType::Buffered, defer: false]
        }
    }
}

/// Screen geometry that decides the stream mode.
#[derive(Debug, Clone, Copy)]
pub struct ScreenInfo {
    pub frame: NSRect,
    pub scale: f64,
    /// The notch: points at the top of the screen no picture should go under.
    pub top_inset: f64,
    /// The panel's own pixels (its native display mode). A scaled mode
    /// ("More Space") renders the desktop larger than this and shrinks it.
    pub panel: Option<(f64, f64)>,
}

impl ScreenInfo {
    pub fn main(mtm: MainThreadMarker) -> Option<ScreenInfo> {
        let screen = NSScreen::mainScreen(mtm)?;
        Some(ScreenInfo {
            frame: screen.frame(),
            scale: screen.backingScaleFactor(),
            top_inset: screen.safeAreaInsets().top,
            panel: panel_pixels(&screen),
        })
    }

    /// The panel's pixels below the notch: 3024x1890 on a 14" MacBook Pro
    /// at the default scaling, 3024x1900 in "More Space" (its menu bar is
    /// taller in panel pixels). `session::native_mode` fits it to a standard
    /// aspect ratio, which is Moonlight's 3024x1890 in both.
    pub fn native_mode(&self) -> (u16, u16) {
        let Some((pw, ph)) = self.panel else {
            return self.desktop_mode();
        };
        let notch = (self.top_inset * pw / self.frame.size.width).ceil();
        ((pw as u16) & !1, ((ph - notch) as u16) & !1)
    }

    /// The desktop's own pixels below the notch: what macOS renders before
    /// scaling to the panel (3600x2262 on that MacBook in "More Space").
    /// Sharper there than the panel size, at more pixels to stream.
    pub fn desktop_mode(&self) -> (u16, u16) {
        let w = (self.frame.size.width * self.scale).round() as u16;
        let h = ((self.frame.size.height - self.top_inset) * self.scale).round() as u16;
        (w & !1, h & !1)
    }
}

/// The pixel size of `screen`'s native display mode.
fn panel_pixels(screen: &NSScreen) -> Option<(f64, f64)> {
    use objc2_core_graphics::{CGDisplayCopyAllDisplayModes, CGDisplayMode};
    use objc2_foundation::{ns_string, NSNumber};
    /// kDisplayModeNativeFlag (IOGraphicsTypes.h).
    const NATIVE: u32 = 0x0200_0000;
    let number = screen
        .deviceDescription()
        .objectForKey(ns_string!("NSScreenNumber"))?;
    let id = number.downcast::<NSNumber>().ok()?.unsignedIntValue();
    let modes = unsafe { CGDisplayCopyAllDisplayModes(id, None) }?;
    (0..modes.count())
        .filter_map(|i| unsafe { (modes.value_at_index(i) as *const CGDisplayMode).as_ref() })
        .find(|m| CGDisplayMode::io_flags(Some(m)) & NATIVE != 0)
        .map(|m| {
            (
                CGDisplayMode::pixel_width(Some(m)) as f64,
                CGDisplayMode::pixel_height(Some(m)) as f64,
            )
        })
}

pub struct Window {
    pub window: Retained<NSWindow>,
    pub view: Retained<StreamView>,
    /// The layer the stream starts with: the view gets a new one when it
    /// moves between its windows (`fresh_layer`), the renderer with it.
    pub layer: Retained<CAMetalLayer>,
    _delegate: Retained<WindowDelegate>,
    /// The monitor that hears key-ups AppKit keeps from the view
    /// (`watch_command_key_ups`), until the window closes.
    key_ups: RefCell<Option<Retained<objc2::runtime::AnyObject>>>,
}

/// AppKit never hands a window the key-up of a key pressed while Command is
/// held: -[NSApplication sendEvent:] keeps it (Cmd+V's V). The host then
/// holds that key down for good and repeats it -- Cmd+V typed "vvvv…" into a
/// Windows host's search box. A local monitor sees each event before
/// `sendEvent:` does, so the stream hears those key-ups there, as GLFW's
/// does (`cocoa_init.m`, `keyUpMonitor`). One that reaches the view as well
/// is harmless: a key up is sent only for a key held.
fn watch_command_key_ups(
    view: &Retained<StreamView>,
) -> Option<Retained<objc2::runtime::AnyObject>> {
    let weak = objc2::rc::Weak::from_retained(view);
    let block = block2::RcBlock::new(move |event: std::ptr::NonNull<NSEvent>| -> *mut NSEvent {
        // SAFETY: AppKit hands the monitor a live event for the call.
        let e = unsafe { event.as_ref() };
        if e.modifierFlags().contains(NSEventModifierFlags::Command) {
            if let Some(view) = weak.load() {
                if view.window().is_some_and(|w| w.isKeyWindow()) {
                    view.with(|h| h.key(e, false));
                }
            }
        }
        // On to the app as usual.
        event.as_ptr()
    });
    // SAFETY: the block lives as long as the monitor (AppKit copies it), and
    // only reads the event it is given on the main thread.
    unsafe {
        NSEvent::addLocalMonitorForEventsMatchingMask_handler(
            objc2_app_kit::NSEventMask::KeyUp,
            &block,
        )
    }
}

impl Window {
    pub fn open(
        mtm: MainThreadMarker,
        render: Arc<RenderShared>,
        fullscreen: bool,
        stream: (u32, u32),
        title: &str,
    ) -> Window {
        let info = ScreenInfo::main(mtm).unwrap_or(ScreenInfo {
            frame: NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(1512.0, 982.0)),
            scale: 2.0,
            top_inset: 0.0,
            panel: None,
        });
        // A titled window either way: full screen, the view then moves into a
        // window over the whole screen (below).
        let w = (stream.0 as f64 / info.scale).min(info.frame.size.width * 0.9);
        let h = (stream.1 as f64 / info.scale).min(info.frame.size.height * 0.9);
        let rect = NSRect::new(
            NSPoint::new(
                info.frame.origin.x + (info.frame.size.width - w) / 2.0,
                info.frame.origin.y + (info.frame.size.height - h) / 2.0,
            ),
            NSSize::new(w, h),
        );
        let inset = 0.0;
        let window: Retained<NSWindow> =
            StreamNSWindow::new(mtm, rect, WINDOWED_STYLE).into_super();
        unsafe { window.setReleasedWhenClosed(false) };
        window.setTitle(&objc2_foundation::NSString::from_str(title));
        window.setBackgroundColor(Some(&NSColor::blackColor()));
        window.setOpaque(true);
        window.setAcceptsMouseMovedEvents(true);

        let content = NSRect::new(NSPoint::new(0.0, 0.0), rect.size);
        let view = StreamView::new(mtm, content, render.clone(), inset);
        let layer = CAMetalLayer::new();
        layer.setFrame(CGRect::new(
            CGPoint::new(0.0, 0.0),
            CGSize::new(rect.size.width, rect.size.height),
        ));
        layer.setContentsScale(info.scale);
        // Opaque, and nothing else in the window: with nothing over the window
        // either (full screen, `FULL_SCREEN_LEVEL`), lets the compositor send
        // the layer straight to the display ("direct to display"), which
        // saves a whole refresh of latency against a composited layer.
        layer.setOpaque(true);
        // Layer-hosting: set the layer before wantsLayer.
        view.setLayer(Some(&layer));
        view.setWantsLayer(true);
        window.setContentView(Some(&view));

        let delegate = WindowDelegate::new(mtm, view.clone());
        let _ = delegate.ivars().home.set(window.clone());
        window.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));

        // Keeps the green button a full-screen one (`toggleFullScreen:`).
        window.setCollectionBehavior(NSWindowCollectionBehavior::FullScreenPrimary);

        let app = NSApplication::sharedApplication(mtm);
        if fullscreen {
            delegate.enter_full_screen();
        } else {
            window.makeKeyAndOrderFront(None);
            window.makeFirstResponder(Some(&view));
        }
        #[allow(deprecated)]
        app.activateIgnoringOtherApps(true);
        view.publish_layout();
        let key_ups = RefCell::new(watch_command_key_ups(&view));
        Window {
            window,
            view,
            layer,
            _delegate: delegate,
            key_ups,
        }
    }

    /// The window the stream is in now: the full-screen one, or `window`.
    pub fn current(&self) -> Retained<NSWindow> {
        self._delegate
            .current()
            .unwrap_or_else(|| self.window.clone())
    }

    pub fn close(&self) {
        if let Some(monitor) = self.key_ups.borrow_mut().take() {
            // SAFETY: a monitor `addLocalMonitorForEventsMatchingMask` gave,
            // removed once (taken).
            unsafe { NSEvent::removeMonitor(&monitor) };
        }
        self.view.with(|h| set_capture(h, false));
        self._delegate.close_full_screen();
        // Either way: Ctrl+Alt+Shift+X may have made the stream full screen.
        if let Some(mtm) = MainThreadMarker::new() {
            NSApplication::sharedApplication(mtm)
                .setPresentationOptions(NSApplicationPresentationOptions::Default);
        }
        self.window.orderOut(None);
        self.window.close();
    }
}

/// How long after the stream window takes the keyboard back it takes the
/// pointer: at once, coming back from a screenshot tool, hiding the pointer
/// and cutting it off from the mouse did not take, and the Mac's pointer
/// moved over the stream beside the host's. macOS hands the app the front a
/// moment after its window is key.
const CAPTURE_SETTLE: Duration = Duration::from_millis(150);

/// Take the pointer once the stream window has had the keyboard for
/// `CAPTURE_SETTLE`, if it still has it and Helio is the active app.
fn capture_when_settled() {
    let Ok(when) = dispatch2::DispatchTime::try_from(CAPTURE_SETTLE) else {
        return;
    };
    let _ = dispatch2::DispatchQueue::main().after(when, || {
        for_stream_windows(|window, view, active| {
            if active && window.isKeyWindow() {
                view.with(|h| set_capture(h, true));
            }
        });
    });
}

/// Each stream window, its view, and whether Helio is the active app.
fn for_stream_windows(mut f: impl FnMut(&NSWindow, &StreamView, bool)) {
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let app = NSApplication::sharedApplication(mtm);
    let active = app.isActive();
    for window in app.windows().iter() {
        if let Some(view) = window
            .contentView()
            .and_then(|v| v.downcast::<StreamView>().ok())
        {
            f(&window, &view, active);
        }
    }
}

/// Moonlight's Ctrl+Alt+Shift+X: the key stream window into or out of full
/// screen.
pub fn toggle_fullscreen() {
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let app = NSApplication::sharedApplication(mtm);
    let Some(window) = app.keyWindow() else {
        return;
    };
    if window
        .contentView()
        .and_then(|v| v.downcast::<StreamView>().ok())
        .is_none()
    {
        return;
    }
    window.toggleFullScreen(None);
}

/// Moonlight's Ctrl+Alt+Shift+D: the key window into the Dock -- the stream
/// window through its delegate, which knows whether it is full screen.
fn minimise() {
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let Some(window) = NSApplication::sharedApplication(mtm).keyWindow() else {
        return;
    };
    let delegate = window.delegate();
    let delegate: Option<&AnyObject> = delegate.as_deref().map(|d| d.as_ref());
    match delegate.and_then(|d| d.downcast_ref::<WindowDelegate>()) {
        Some(d) => d.minimise(),
        None => window.miniaturize(None),
    }
}

/// Warp the (captured, hidden) system pointer to the window's centre, so it is
/// somewhere sensible when released.
pub fn centre_pointer(window: &NSWindow) {
    let f = window.frame();
    // Cocoa's screen coordinates rise from the bottom of the primary
    // display; the warp's fall from its top.
    let primary = MainThreadMarker::new()
        .and_then(|mtm| NSScreen::screens(mtm).firstObject())
        .map_or(f.size.height, |s| s.frame().size.height);
    let _ = CGWarpMouseCursorPosition(CGPoint::new(
        f.origin.x + f.size.width / 2.0,
        primary - (f.origin.y + f.size.height / 2.0),
    ));
}

/// Whether the system pointer is over `window` (both in Cocoa's screen
/// coordinates).
fn pointer_inside(window: &NSWindow) -> bool {
    let (p, f) = (NSEvent::mouseLocation(), window.frame());
    p.x >= f.origin.x
        && p.x < f.origin.x + f.size.width
        && p.y >= f.origin.y
        && p.y < f.origin.y + f.size.height
}
