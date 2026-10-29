//! GNOME under Wayland: a virtual monitor at the client's mode, recorded
//! with mutter's own screen cast, and the client's input through mutter's
//! own remote desktop: what gnome-remote-desktop does for its clients
//! (`grd-session.c`, `grd-rdp-pipewire-stream.c`), with no dialog to answer.
//!
//! `org.gnome.Mutter.RemoteDesktop` and `org.gnome.Mutter.ScreenCast` are
//! mutter's private D-Bus API, open to any process of the session (the
//! desktop portal is one): a remote-desktop session, a screen-cast session
//! tied to it, and in that a `RecordVirtual` stream, which is a monitor of
//! its own that the desktop extends onto. mutter 50 takes the mode as the
//! stream's `modes`; older ones ignore that and make the monitor the size
//! the PipeWire consumer asks for, so the capture asks for exactly that
//! size too (`helio_capture::pipewire::Mode`). Once it is among mutter's
//! monitors, `mutter_layout` arranges it (alone, or beside the host's
//! own). The monitor goes when the session stops.

use std::collections::HashMap;
use std::time::Duration;

use ashpd::zbus::{self, Connection, Proxy};
use ashpd::zvariant::{OwnedObjectPath, Value};
use futures_util::StreamExt;
use helio_display::DisplayMode;
use helio_input::InputError;
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};

use crate::remote_input::{Deliver, Op, RemoteSink};

const SCREEN_CAST: &str = "org.gnome.Mutter.ScreenCast";
const REMOTE_DESKTOP: &str = "org.gnome.Mutter.RemoteDesktop";

/// `cursor-mode` 1: the pointer drawn into the picture.
const CURSOR_EMBEDDED: u32 = 1;

enum Cmd {
    Input(Vec<Op>),
    Close,
}

/// A virtual monitor of mutter's, streamed and driven for one session.
pub struct VirtualMonitor {
    tx: UnboundedSender<Cmd>,
    /// Hung up when the session's task has stopped the session.
    done: std::sync::mpsc::Receiver<()>,
}

/// Whether mutter's screen cast is on the session bus: a GNOME session.
pub fn available() -> bool {
    let Ok(rt) = crate::portal::runtime() else {
        return false;
    };
    rt.block_on(async {
        let Ok(conn) = Connection::session().await else {
            return false;
        };
        let Ok(dbus) = zbus::fdo::DBusProxy::new(&conn).await else {
            return false;
        };
        let Ok(name) = SCREEN_CAST.try_into() else {
            return false;
        };
        dbus.name_has_owner(name).await.unwrap_or(false)
    })
}

impl VirtualMonitor {
    /// A virtual monitor in `mode`, and its stream's PipeWire node on the
    /// session's PipeWire; arranged, once mutter lists it, as the whole
    /// desktop (`isolate`) or beside the host's monitors, at `scale`.
    /// Blocks until mutter has made it, or `wait` passes.
    pub fn open(
        mode: DisplayMode,
        isolate: bool,
        scale: f64,
        wait: Duration,
    ) -> Result<(VirtualMonitor, u32), String> {
        let (tx, rx) = unbounded_channel();
        let (ready_tx, ready_rx) = std::sync::mpsc::sync_channel(1);
        let (done_tx, done) = std::sync::mpsc::channel::<()>();
        crate::portal::runtime()?.spawn(async move {
            serve(mode, isolate, scale, rx, ready_tx).await;
            drop(done_tx);
        });
        let monitor = VirtualMonitor { tx, done };
        match ready_rx.recv_timeout(wait) {
            Ok(Ok(node)) => Ok((monitor, node)),
            Ok(Err(e)) => Err(e),
            Err(_) => Err(format!(
                "mutter made no virtual monitor within {} s",
                wait.as_secs()
            )),
        }
    }

    /// The client's input, to the virtual monitor: `picture` is where the
    /// picture sits in the stream (all of it), whose coordinates mutter's
    /// pointer takes.
    pub fn sink(&self, picture: (u32, u32, u32, u32)) -> RemoteSink {
        let tx = self.tx.clone();
        let deliver: Deliver = Box::new(move |ops| {
            tx.send(Cmd::Input(ops))
                .map_err(|_| InputError::Unavailable)
        });
        RemoteSink::new(deliver, picture, None)
    }
}

impl Drop for VirtualMonitor {
    fn drop(&mut self) {
        let _ = self.tx.send(Cmd::Close);
        // The monitor gone before the next session makes one (a moment;
        // not forever, if mutter stopped answering).
        let _ = self.done.recv_timeout(Duration::from_secs(5));
    }
}

/// Stops a task the session started when the session ends first.
struct AbortOnDrop(Option<tokio::task::JoinHandle<()>>);

impl Drop for AbortOnDrop {
    fn drop(&mut self) {
        if let Some(t) = &self.0 {
            t.abort();
        }
    }
}

/// The remote-desktop session, and the stream's object path, which
/// mutter's absolute pointer names.
struct Session {
    remote: Proxy<'static>,
    stream: String,
}

async fn serve(
    mode: DisplayMode,
    isolate: bool,
    scale: f64,
    mut rx: UnboundedReceiver<Cmd>,
    ready: std::sync::mpsc::SyncSender<Result<u32, String>>,
) {
    let conn = match Connection::session().await {
        Ok(c) => c,
        Err(e) => {
            let _ = ready.send(Err(format!("no session bus: {e}")));
            return;
        }
    };
    // The monitors there are, to tell the virtual one by.
    let before = match crate::mutter_layout::connectors(&conn).await {
        Ok(b) => Some(b),
        Err(e) => {
            tracing::warn!(error = %e, "cannot read mutter's monitors: the virtual monitor \
                stays where mutter puts it");
            None
        }
    };
    let started = tokio::select! {
        r = start(&conn, mode) => r,
        _ = rx.recv() => return,
    };
    let (session, node, mut closed) = match started {
        Ok(v) => v,
        Err(e) => {
            let _ = ready.send(Err(e));
            return;
        }
    };
    tracing::info!(
        node,
        width = mode.width,
        height = mode.height,
        fps = mode.refresh_mhz as f64 / 1000.0,
        "GNOME made a virtual monitor"
    );
    let _ = ready.send(Ok(node));
    let arranging = before.map(|before| {
        let conn = conn.clone();
        tokio::spawn(async move {
            if let Err(e) = crate::mutter_layout::arrange(conn, before, isolate, scale).await {
                tracing::warn!(error = %e, "the virtual monitor stays where mutter put it");
            }
        })
    });
    let _arranging = AbortOnDrop(arranging);
    loop {
        tokio::select! {
            cmd = rx.recv() => match cmd {
                Some(Cmd::Input(ops)) => {
                    for op in ops {
                        if let Err(e) = apply(&session, op).await {
                            tracing::debug!(error = %e, ?op, "mutter input");
                        }
                    }
                }
                Some(Cmd::Close) | None => break,
            },
            _ = closed.next() => {
                tracing::info!("GNOME ended the remote desktop session (stopped from its menu)");
                // Its stream is gone with it: the video stops, and the
                // session with it.
                while let Some(cmd) = rx.recv().await {
                    if matches!(cmd, Cmd::Close) {
                        break;
                    }
                }
                return;
            }
        }
    }
    let _ = session.remote.call::<_, _, ()>("Stop", &()).await;
}

/// A remote-desktop session with a screen cast of a virtual monitor in
/// `mode`, started; its PipeWire node, and the signal that mutter closed
/// it.
async fn start(
    conn: &Connection,
    mode: DisplayMode,
) -> Result<(Session, u32, zbus::proxy::SignalStream<'static>), String> {
    let call = |what: &'static str| move |e: zbus::Error| format!("mutter's {what}: {e}");
    let remote_desktop = Proxy::new(
        conn,
        REMOTE_DESKTOP,
        "/org/gnome/Mutter/RemoteDesktop",
        REMOTE_DESKTOP,
    )
    .await
    .map_err(call("remote desktop"))?;
    let path: OwnedObjectPath = remote_desktop
        .call("CreateSession", &())
        .await
        .map_err(call("remote desktop session"))?;
    let remote = Proxy::new_owned(
        conn.clone(),
        REMOTE_DESKTOP.to_string(),
        path,
        format!("{REMOTE_DESKTOP}.Session"),
    )
    .await
    .map_err(call("remote desktop session"))?;
    let session_id: String = remote
        .get_property("SessionId")
        .await
        .map_err(call("remote desktop session"))?;

    let screen_cast = Proxy::new(
        conn,
        SCREEN_CAST,
        "/org/gnome/Mutter/ScreenCast",
        SCREEN_CAST,
    )
    .await
    .map_err(call("screen cast"))?;
    let props: HashMap<&str, Value> = HashMap::from([(
        "remote-desktop-session-id",
        Value::from(session_id.as_str()),
    )]);
    let path: OwnedObjectPath = screen_cast
        .call("CreateSession", &(props,))
        .await
        .map_err(call("screen cast session"))?;
    let cast = Proxy::new(conn, SCREEN_CAST, path, format!("{SCREEN_CAST}.Session"))
        .await
        .map_err(call("screen cast session"))?;
    let stream_path: OwnedObjectPath = cast
        .call("RecordVirtual", &(record_virtual(mode),))
        .await
        .map_err(call("virtual monitor"))?;
    let stream = Proxy::new(
        conn,
        SCREEN_CAST,
        stream_path.clone(),
        format!("{SCREEN_CAST}.Stream"),
    )
    .await
    .map_err(call("virtual monitor"))?;
    // Listening before the start, which is when mutter says the node.
    let mut added = stream
        .receive_signal("PipeWireStreamAdded")
        .await
        .map_err(call("virtual monitor"))?;
    let closed = remote
        .receive_signal("Closed")
        .await
        .map_err(call("remote desktop session"))?;
    remote
        .call::<_, _, ()>("Start", &())
        .await
        .map_err(call("remote desktop session's start"))?;
    let node = match added.next().await {
        Some(msg) => msg
            .body()
            .deserialize::<u32>()
            .map_err(|e| format!("mutter's stream node: {e}"))?,
        None => return Err("mutter closed the virtual monitor's stream".into()),
    };
    Ok((
        Session {
            remote,
            stream: stream_path.as_str().to_string(),
        },
        node,
        closed,
    ))
}

/// `RecordVirtual`'s properties: the pointer drawn in, and the one mode
/// (mutter 50; ignored before).
fn record_virtual(mode: DisplayMode) -> HashMap<&'static str, Value<'static>> {
    let one: HashMap<&str, Value> = HashMap::from([
        ("size", Value::from((mode.width as u32, mode.height as u32))),
        (
            "refresh-rate",
            Value::from(mode.refresh_mhz as f64 / 1000.0),
        ),
        ("is-preferred", Value::from(true)),
    ]);
    HashMap::from([
        ("cursor-mode", Value::from(CURSOR_EMBEDDED)),
        ("modes", Value::from(vec![one])),
    ])
}

async fn apply(s: &Session, op: Op) -> zbus::Result<()> {
    let r = &s.remote;
    match op {
        Op::Key(code, down) => r.call("NotifyKeyboardKeycode", &(code as u32, down)).await,
        Op::Keysym(sym, down) => r.call("NotifyKeyboardKeysym", &(sym as u32, down)).await,
        Op::Motion(dx, dy) => r.call("NotifyPointerMotionRelative", &(dx, dy)).await,
        Op::Absolute(x, y) => {
            r.call("NotifyPointerMotionAbsolute", &(s.stream.as_str(), x, y))
                .await
        }
        Op::Button(code, down) => r.call("NotifyPointerButton", &(code, down)).await,
        // Axis 0 is the vertical wheel, 1 the horizontal.
        Op::Scroll(vertical, steps) => {
            r.call("NotifyPointerAxisDiscrete", &(u32::from(!vertical), steps))
                .await
        }
    }
}
