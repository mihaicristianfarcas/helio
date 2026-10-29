//! KDE Plasma under Wayland: a virtual output at the client's mode, made
//! and streamed by KWin's own screen cast (`zkde_screencast_unstable_v1`'s
//! `stream_virtual_output`, what krfb's virtual monitor and KDE's portal
//! use), and the client's input through KWin's fake input
//! (`org_kde_kwin_fake_input`, what KDE Connect and the portal use), with no
//! dialog to answer.
//!
//! Both are interfaces KWin keeps from programs that do not ask for them:
//! KWin 6.6 offers them only to a program whose desktop entry lists them in
//! `X-KDE-Wayland-Interfaces` (it finds the entry by the program's path),
//! which is why Helio's install puts `dev.mihaicristian.helio.host.desktop`
//! beside `helio-host` (`tools/linux/install-apps`). Later KWins keep them
//! only from sandboxed programs.
//!
//! The output is a monitor of its own that the desktop extends onto, placed
//! where KWin puts it; the pointer's position on it is turned into KWin's
//! global coordinates, which fake input takes. Text goes by the key that
//! types each character on the host's layout (KWin's keymap, its first
//! layout), with a Shift pressed around it where that is how, as the X11
//! sink types (`helio_input::typing`): KWin's own keysym typing sets the
//! modifiers out of step with the key, so a capital came out small and the
//! letter after it capital (KWin 6.6). A character the layout lacks goes as
//! a keysym (fake input 6), which KWin types on a key of its own with no
//! modifier. The output goes when its stream is closed, or when this
//! connection does (the host exiting).

use std::collections::HashMap;
use std::os::fd::AsRawFd;
use std::os::unix::fs::FileExt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use helio_input::InputError;
use parking_lot::Mutex;
use wayland_client::protocol::{wl_keyboard, wl_output, wl_registry, wl_seat};
use wayland_client::{Connection, Dispatch, EventQueue, Proxy, QueueHandle, WEnum};
use wayland_protocols_plasma::screencast::v1::client::{
    zkde_screencast_stream_unstable_v1::{self, ZkdeScreencastStreamUnstableV1},
    zkde_screencast_unstable_v1::{self, ZkdeScreencastUnstableV1},
};

use crate::remote_input::{Deliver, Op, RemoteSink};
use fake::org_kde_kwin_fake_input::{self, OrgKdeKwinFakeInput};

/// KDE's fake input, version 6 (`keyboard_keysym`): a newer copy of the
/// protocol than `wayland-protocols-plasma` carries, from
/// plasma-wayland-protocols (LGPL-2.1-or-later, as its header says).
#[allow(non_upper_case_globals, clippy::all)]
mod fake {
    use wayland_client;
    pub mod __interfaces {
        use wayland_client::backend as wayland_backend;
        wayland_scanner::generate_interfaces!("src/linux/protocols/fake-input.xml");
    }
    use self::__interfaces::*;
    wayland_scanner::generate_client_code!("src/linux/protocols/fake-input.xml");
}

/// The versions used: `stream_virtual_output_with_description` (4); fake
/// input's absolute pointer (3), keys (4) and keysyms (6).
const SCREENCAST_VERSION: u32 = 4;
const FAKE_INPUT_VERSION: u32 = 6;
const FAKE_INPUT_ABSOLUTE: u32 = 3;
const FAKE_INPUT_KEYS: u32 = 4;
const FAKE_INPUT_KEYSYMS: u32 = 6;
/// `wl_output.name` (4): which output is ours.
const OUTPUT_VERSION: u32 = 4;
/// `wl_seat`'s keyboard, for KWin's keymap.
const SEAT_VERSION: u32 = 5;

/// `pointer` 2: drawn into the picture.
const POINTER_EMBEDDED: u32 = 2;

/// The output's name: KWin calls it `Virtual-Helio`.
const NAME: &str = "Helio";

/// A wheel notch in KWin's scroll units, as libinput reports one (15°).
const NOTCH: f64 = 15.0;

/// How long KWin may take to make the output and its stream.
const MAKE_WAIT: Duration = Duration::from_secs(10);
/// How often the dispatch thread looks whether it should stop.
const POLL_MS: i32 = 200;

/// Whether KWin offers this program what a virtual output needs.
pub fn available() -> Result<(), String> {
    let (_conn, _queue, mut state) = connect()?;
    state.offers().map(|_| ())
}

/// A virtual output of KWin's, streamed and driven for one session.
pub struct VirtualOutput {
    conn: Connection,
    stream: ZkdeScreencastStreamUnstableV1,
    fake: OrgKdeKwinFakeInput,
    shared: Arc<Mutex<State>>,
    /// The output's size in KWin's (logical) coordinates.
    logical: (u32, u32),
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl VirtualOutput {
    /// A virtual output of `width` x `height` pixels at `scale`, and its
    /// stream's PipeWire node on the session's PipeWire.
    pub fn open(width: u32, height: u32, scale: f64) -> Result<(VirtualOutput, u32), String> {
        let (conn, mut queue, mut state) = connect()?;
        let (screencast, fake) = state.offers()?;
        fake.authenticate(
            "Helio Host".into(),
            "streams this desktop to a device paired with it".into(),
        );
        // The size asked for is the output's mode, in pixels, whatever the
        // protocol calls it: KWin makes the output that size and scales it
        // (`DrmVirtualOutput`), so its desktop is that over the scale.
        let logical = (
            (width as f64 / scale).round().max(1.0) as u32,
            (height as f64 / scale).round().max(1.0) as u32,
        );
        let qh = queue.handle();
        let stream = if screencast.version() >= 4 {
            screencast.stream_virtual_output_with_description(
                NAME.into(),
                "Helio Host's display for the device streaming it".into(),
                width as i32,
                height as i32,
                scale,
                POINTER_EMBEDDED,
                &qh,
                (),
            )
        } else {
            screencast.stream_virtual_output(
                NAME.into(),
                width as i32,
                height as i32,
                scale,
                POINTER_EMBEDDED,
                &qh,
                (),
            )
        };
        let deadline = Instant::now() + MAKE_WAIT;
        let node = loop {
            queue
                .roundtrip(&mut state)
                .map_err(|e| format!("KWin: {e}"))?;
            if let Some(e) = state.failed.take() {
                return Err(format!("KWin made no virtual output: {e}"));
            }
            if let Some(node) = state.node {
                if state.ours().is_some_and(|o| o.done) {
                    break node;
                }
            }
            if Instant::now() > deadline {
                return Err(format!(
                    "KWin made no virtual output within {} s",
                    MAKE_WAIT.as_secs()
                ));
            }
            std::thread::sleep(Duration::from_millis(20));
        };
        if let Some(o) = state.ours() {
            tracing::info!(
                node,
                width,
                height,
                scale,
                output = o.name.as_deref().unwrap_or(""),
                at = ?o.position,
                "KWin made a virtual output"
            );
        }
        let shared = Arc::new(Mutex::new(state));
        let stop = Arc::new(AtomicBool::new(false));
        let thread = std::thread::Builder::new()
            .name("kwin".into())
            .spawn({
                let (conn, shared, stop) = (conn.clone(), shared.clone(), stop.clone());
                move || dispatch(conn, queue, shared, stop)
            })
            .map_err(|e| e.to_string())?;
        Ok((
            VirtualOutput {
                conn,
                stream,
                fake,
                shared,
                logical,
                stop,
                thread: Some(thread),
            },
            node,
        ))
    }

    /// The client's input, to the virtual output: `picture` is where the
    /// picture sits in the stream (all of it).
    pub fn sink(&self, picture: (u32, u32, u32, u32)) -> RemoteSink {
        let (conn, fake, shared) = (self.conn.clone(), self.fake.clone(), self.shared.clone());
        let deliver: Deliver = Box::new(move |ops| {
            let state = shared.lock();
            let origin = state.ours().map_or((0, 0), |o| o.position);
            for op in ops {
                send(&fake, &state.keys, origin, op);
            }
            drop(state);
            conn.flush().map_err(|_| InputError::Unavailable)
        });
        RemoteSink::new(deliver, picture, Some(self.logical))
    }
}

impl Drop for VirtualOutput {
    fn drop(&mut self) {
        self.stream.close();
        let _ = self.conn.flush();
        self.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

/// The host's layout as fake input types it: which key types each keysym
/// (an X key code: the Linux key code plus 8), and whether with Shift.
#[derive(Default)]
struct Keys {
    typing: HashMap<u32, (u8, bool)>,
    shift: Option<u8>,
}

impl Keys {
    /// From KWin's keymap: its first layout's first two levels.
    fn from_keymap(text: String) -> Option<Keys> {
        use xkbcommon::xkb;
        let context = xkb::Context::new(xkb::CONTEXT_NO_FLAGS);
        let keymap = xkb::Keymap::new_from_string(
            &context,
            text,
            xkb::KEYMAP_FORMAT_TEXT_V1,
            xkb::KEYMAP_COMPILE_NO_FLAGS,
        )?;
        let min = keymap.min_keycode().raw().clamp(8, 255);
        let max = keymap.max_keycode().raw().clamp(min, 255);
        let mut keysyms = Vec::new();
        for kc in min..=max {
            for level in 0..2 {
                let syms = keymap.key_get_syms_by_level(xkb::Keycode::new(kc), 0, level);
                keysyms.push(syms.first().map_or(0, |s| s.raw()));
            }
        }
        let typing = helio_input::typing::typing_map(min as u8, 2, &keysyms, &[]);
        let shift = typing.get(&helio_input::typing::SHIFT_L).map(|k| k.0);
        Some(Keys { typing, shift })
    }
}

/// One op, as fake input: the pointer at `origin` (the output's place) plus
/// its position on the output; text by `keys` where the layout has it.
fn send(fake: &OrgKdeKwinFakeInput, keys: &Keys, origin: (i32, i32), op: Op) {
    let state = |down: bool| u32::from(down);
    let v = fake.version();
    let code = |kc: u8| kc as u32 - 8;
    match op {
        Op::Key(code, down) if v >= FAKE_INPUT_KEYS => fake.keyboard_key(code as u32, state(down)),
        // Typed whole on the way down; nothing more on the way up.
        Op::Keysym(sym, down)
            if v >= FAKE_INPUT_KEYS && keys.typing.contains_key(&(sym as u32)) =>
        {
            let (kc, shifted) = keys.typing[&(sym as u32)];
            let shift = if shifted { keys.shift } else { None };
            if down {
                if let Some(s) = shift {
                    fake.keyboard_key(code(s), 1);
                }
                fake.keyboard_key(code(kc), 1);
                fake.keyboard_key(code(kc), 0);
                if let Some(s) = shift {
                    fake.keyboard_key(code(s), 0);
                }
            }
        }
        Op::Keysym(sym, down) if v >= FAKE_INPUT_KEYSYMS => {
            fake.keyboard_keysym(sym as u32, state(down))
        }
        Op::Motion(dx, dy) => fake.pointer_motion(dx, dy),
        Op::Absolute(x, y) if v >= FAKE_INPUT_ABSOLUTE => {
            fake.pointer_motion_absolute(origin.0 as f64 + x, origin.1 as f64 + y)
        }
        Op::Button(code, down) => fake.button(code as u32, state(down)),
        // wl_pointer's axes: 0 vertical, 1 horizontal.
        Op::Scroll(vertical, steps) => fake.axis(u32::from(!vertical), steps as f64 * NOTCH),
        _ => tracing::trace!(?op, version = v, "KWin's fake input is too old for this"),
    }
}

/// Read KWin's events until told to stop: where the output is, and
/// whether its stream ends.
fn dispatch(
    conn: Connection,
    mut queue: EventQueue<State>,
    shared: Arc<Mutex<State>>,
    stop: Arc<AtomicBool>,
) {
    while !stop.load(Ordering::Relaxed) {
        let mut state = shared.lock();
        if queue.dispatch_pending(&mut state).is_err() {
            return;
        }
        drop(state);
        let _ = conn.flush();
        let Some(guard) = queue.prepare_read() else {
            continue;
        };
        let mut fds = [libc::pollfd {
            fd: guard.connection_fd().as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        }];
        // SAFETY: one valid pollfd, for the length given.
        let ready = unsafe { libc::poll(fds.as_mut_ptr(), 1, POLL_MS) };
        if ready > 0 {
            if guard.read().is_err() {
                return;
            }
        } else {
            drop(guard);
        }
    }
}

fn connect() -> Result<(Connection, EventQueue<State>, State), String> {
    let conn = Connection::connect_to_env().map_err(|e| format!("no Wayland display: {e}"))?;
    let mut queue = conn.new_event_queue();
    let qh = queue.handle();
    let _registry = conn.display().get_registry(&qh, ());
    let mut state = State::default();
    // The globals, then the outputs' own events.
    for _ in 0..2 {
        queue
            .roundtrip(&mut state)
            .map_err(|e| format!("KWin: {e}"))?;
    }
    Ok((conn, queue, state))
}

#[derive(Default)]
struct State {
    screencast: Option<ZkdeScreencastUnstableV1>,
    fake: Option<OrgKdeKwinFakeInput>,
    keyboard: Option<wl_keyboard::WlKeyboard>,
    keys: Keys,
    outputs: Vec<Output>,
    /// How many outputs there were before ours was asked for.
    before: Option<usize>,
    node: Option<u32>,
    failed: Option<String>,
}

struct Output {
    proxy: wl_output::WlOutput,
    name: Option<String>,
    /// In KWin's global (logical) coordinates.
    position: (i32, i32),
    /// Its first round of events is in.
    done: bool,
}

impl State {
    fn offers(&mut self) -> Result<(ZkdeScreencastUnstableV1, OrgKdeKwinFakeInput), String> {
        match (&self.screencast, &self.fake) {
            (Some(s), Some(f)) => {
                self.before = Some(self.outputs.len());
                Ok((s.clone(), f.clone()))
            }
            _ => Err(
                "KWin offers this program no virtual output (zkde_screencast_unstable_v1) \
                or no fake input: install helio-host with Helio's install, whose desktop entry \
                asks KWin for them"
                    .into(),
            ),
        }
    }

    /// Our output: the one named after us, of those that came after the
    /// stream was asked for.
    fn ours(&self) -> Option<&Output> {
        let new = self.outputs.iter().skip(self.before.unwrap_or(usize::MAX));
        let mut fallback = None;
        for o in new {
            if o.name.as_deref().is_some_and(|n| n.ends_with(NAME)) {
                return Some(o);
            }
            fallback.get_or_insert(o);
        }
        fallback
    }
}

impl Dispatch<wl_registry::WlRegistry, ()> for State {
    fn event(
        state: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        let wl_registry::Event::Global {
            name,
            interface,
            version,
        } = event
        else {
            return;
        };
        match interface.as_str() {
            "zkde_screencast_unstable_v1" if version >= 2 => {
                state.screencast =
                    Some(registry.bind(name, version.min(SCREENCAST_VERSION), qh, ()));
            }
            "org_kde_kwin_fake_input" => {
                state.fake = Some(registry.bind(name, version.min(FAKE_INPUT_VERSION), qh, ()));
            }
            "wl_seat" => {
                let _: wl_seat::WlSeat = registry.bind(name, version.min(SEAT_VERSION), qh, ());
            }
            "wl_output" => {
                let proxy = registry.bind(name, version.min(OUTPUT_VERSION), qh, ());
                state.outputs.push(Output {
                    proxy,
                    name: None,
                    position: (0, 0),
                    done: false,
                });
            }
            _ => {}
        }
    }
}

impl Dispatch<wl_seat::WlSeat, ()> for State {
    fn event(
        state: &mut Self,
        seat: &wl_seat::WlSeat,
        event: wl_seat::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let wl_seat::Event::Capabilities {
            capabilities: WEnum::Value(caps),
        } = event
        {
            if caps.contains(wl_seat::Capability::Keyboard) && state.keyboard.is_none() {
                state.keyboard = Some(seat.get_keyboard(qh, ()));
            }
        }
    }
}

impl Dispatch<wl_keyboard::WlKeyboard, ()> for State {
    fn event(
        state: &mut Self,
        _: &wl_keyboard::WlKeyboard,
        event: wl_keyboard::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let wl_keyboard::Event::Keymap {
            format: WEnum::Value(wl_keyboard::KeymapFormat::XkbV1),
            fd,
            size,
        } = event
        else {
            return;
        };
        // Read where it is, not from the file's position: KWin may hand
        // every client the same open file.
        let mut text = vec![0u8; size as usize];
        let read = std::fs::File::from(fd).read_exact_at(&mut text, 0);
        let end = text.iter().position(|&b| b == 0).unwrap_or(text.len());
        text.truncate(end);
        match read.ok().and_then(|()| String::from_utf8(text).ok()) {
            Some(text) => match Keys::from_keymap(text) {
                Some(keys) => state.keys = keys,
                None => tracing::debug!("KWin's keymap did not compile: text goes as keysyms"),
            },
            None => tracing::debug!("KWin's keymap could not be read: text goes as keysyms"),
        }
    }
}

impl Dispatch<wl_output::WlOutput, ()> for State {
    fn event(
        state: &mut Self,
        output: &wl_output::WlOutput,
        event: wl_output::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let Some(o) = state.outputs.iter_mut().find(|o| &o.proxy == output) else {
            return;
        };
        match event {
            wl_output::Event::Geometry { x, y, .. } => o.position = (x, y),
            wl_output::Event::Name { name } => o.name = Some(name),
            wl_output::Event::Done => o.done = true,
            _ => {}
        }
    }
}

impl Dispatch<ZkdeScreencastStreamUnstableV1, ()> for State {
    fn event(
        state: &mut Self,
        _: &ZkdeScreencastStreamUnstableV1,
        event: zkde_screencast_stream_unstable_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            zkde_screencast_stream_unstable_v1::Event::Created { node } => state.node = Some(node),
            zkde_screencast_stream_unstable_v1::Event::Failed { error } => {
                state.failed = Some(error)
            }
            zkde_screencast_stream_unstable_v1::Event::Closed => {
                tracing::info!("KWin closed the virtual output's stream")
            }
            _ => {}
        }
    }
}

impl Dispatch<ZkdeScreencastUnstableV1, ()> for State {
    fn event(
        _: &mut Self,
        _: &ZkdeScreencastUnstableV1,
        _: zkde_screencast_unstable_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<OrgKdeKwinFakeInput, ()> for State {
    fn event(
        _: &mut Self,
        _: &OrgKdeKwinFakeInput,
        _: org_kde_kwin_fake_input::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

/// These need a KDE Plasma session under Wayland (KWin with OpenGL), and
/// this test program named in a desktop entry that asks KWin for the
/// interfaces, as `install.sh` names `helio-host`; run them from a
/// terminal in the session:
///
/// ```sh
/// cargo test --release -p helio-host --no-run      # prints the test program's path
/// printf '[Desktop Entry]\nType=Application\nName=Helio Host test\nExec=%s\nNoDisplay=true\nX-KDE-Wayland-Interfaces=zkde_screencast_unstable_v1,org_kde_kwin_fake_input\n' \
///     "$(realpath target/release/deps/helio_host-HASH)" > ~/.local/share/applications/helio-host-test.desktop
/// kbuildsycoca6
/// target/release/deps/helio_host-HASH --ignored kwin
/// ```
#[cfg(test)]
mod tests {
    use super::*;
    use helio_capture::pipewire::PipeWireCapture;
    use helio_capture::Grab;
    use helio_input::InputSink;
    use helio_proto::input::InputEvent;

    /// A frame from `capture`, within a few seconds.
    fn frame(capture: &mut PipeWireCapture) -> helio_capture::Frame {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let (Ok(Grab::Frame), Some(f)) = (capture.grab(500), capture.image()) {
                return f;
            }
            assert!(Instant::now() < deadline, "no frame from KWin's stream");
        }
    }

    /// Whether the pixels around (x, y) differ between two frames.
    fn changed_near(a: &helio_capture::Frame, b: &helio_capture::Frame, x: u32, y: u32) -> bool {
        let (ab, bb) = (a.bytes(), b.bytes());
        (y.saturating_sub(16)..(y + 48).min(a.height)).any(|row| {
            let start = row as usize * a.stride + x.saturating_sub(16) as usize * 4;
            let end = row as usize * a.stride + (x + 48).min(a.width) as usize * 4;
            ab[start..end] != bb[start..end]
        })
    }

    #[test]
    #[ignore = "requires a KDE Plasma session that grants this program KWin's interfaces"]
    fn kwin_makes_an_output_of_the_clients_size_and_takes_its_pointer() {
        let (output, node) = VirtualOutput::open(1280, 800, 2.0).expect("a virtual output");
        let mut capture = PipeWireCapture::new(None, node, None).expect("its stream");
        let before = frame(&mut capture);
        assert_eq!(
            (before.width, before.height),
            (1280, 800),
            "the stream is the client's size in pixels, at its density"
        );
        assert!(
            output.shared.lock().ours().is_some(),
            "KWin lists the output"
        );
        // The pointer onto the output, a quarter of the way in: drawn into
        // the picture there.
        let mut sink = output.sink((0, 0, 1280, 800));
        sink.inject(&[InputEvent::MouseMoveAbs { x: 320, y: 200 }])
            .expect("input reaches KWin");
        std::thread::sleep(Duration::from_millis(300));
        sink.inject(&[InputEvent::MouseMoveAbs { x: 321, y: 200 }])
            .expect("input reaches KWin");
        let mut after = frame(&mut capture);
        for _ in 0..10 {
            if changed_near(&before, &after, 320, 200) {
                break;
            }
            after = frame(&mut capture);
        }
        assert!(
            changed_near(&before, &after, 320, 200),
            "the pointer is drawn where it was moved"
        );
        drop(capture);
        drop(output);
        std::thread::sleep(Duration::from_millis(500));
        let (_conn, _queue, state) = connect().expect("KWin");
        assert!(
            !state
                .outputs
                .iter()
                .any(|o| o.name.as_deref().is_some_and(|n| n.ends_with(NAME))),
            "the output is gone with its stream"
        );
    }

    #[test]
    #[ignore = "requires a KDE Plasma session that grants this program KWin's interfaces, and Konsole"]
    fn kwin_types_keys_and_text_into_a_window_on_the_output() {
        let (output, _node) = VirtualOutput::open(1280, 800, 1.0).expect("a virtual output");
        let mut sink = output.sink((0, 0, 1280, 800));
        // The pointer on the output first: KWin opens a window on the
        // output the pointer is on.
        sink.inject(&[InputEvent::MouseMoveAbs { x: 640, y: 400 }])
            .expect("input reaches KWin");
        let typed = std::env::temp_dir().join(format!("helio-kwin-typed-{}", std::process::id()));
        let _ = std::fs::remove_file(&typed);
        let mut konsole = std::process::Command::new("konsole")
            .args(["--separate", "-e", "sh", "-c"])
            .arg(format!(
                "read -r line; printf %s \"$line\" > '{}'",
                typed.display()
            ))
            .spawn()
            .expect("Konsole");
        std::thread::sleep(Duration::from_secs(3));
        // Into the window, then a line: `ls` by keys, the rest as text, a
        // capital and a shifted symbol by the layout's keys. (A letter the
        // layout lacks, sent as a keysym, does not reach Konsole: KWin 6.6
        // types it on a keymap of its own that Konsole does not take.)
        let mut events = vec![
            InputEvent::ButtonDown(helio_proto::input::Button::Left),
            InputEvent::ButtonUp(helio_proto::input::Button::Left),
            InputEvent::KeyDown(0x26),
            InputEvent::KeyUp(0x26),
            InputEvent::KeyDown(0x1F),
            InputEvent::KeyUp(0x1F),
        ];
        events.extend(" from Helio!".chars().map(InputEvent::Text));
        events.extend([InputEvent::KeyDown(0x1C), InputEvent::KeyUp(0x1C)]);
        for e in events {
            sink.inject(&[e]).expect("input reaches KWin");
            std::thread::sleep(Duration::from_millis(30));
        }
        let deadline = Instant::now() + Duration::from_secs(5);
        while !typed.exists() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(100));
        }
        let _ = konsole.kill();
        let _ = konsole.wait();
        let line = std::fs::read_to_string(&typed).unwrap_or_default();
        let _ = std::fs::remove_file(&typed);
        assert_eq!(
            line, "ls from Helio!",
            "what reached the window on the output"
        );
    }
}
