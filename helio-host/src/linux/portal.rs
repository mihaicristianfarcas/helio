//! Wayland: the screen and the input through the desktop portal
//! (xdg-desktop-portal), the way desktop sharing is meant to work there.
//!
//! A RemoteDesktop session with a screen cast in it gives both, on GNOME and
//! KDE; where the desktop has no RemoteDesktop portal (wlroots: sway,
//! Hyprland) a ScreenCast session gives the screen alone. The first session
//! asks whoever sits at the host to allow it (the desktop's own dialog);
//! the portal then hands back a restore token, kept in the data directory,
//! and later sessions start without asking until it is revoked.
//!
//! The portal is a D-Bus service, driven here from one small async runtime
//! for the whole process; input goes to it over a queue, in order.

use std::os::fd::OwnedFd;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Duration;

use ashpd::desktop::inhibit::{InhibitFlags, InhibitOptions, InhibitProxy};
use ashpd::desktop::remote_desktop::{
    Axis, DeviceType, KeyState, RemoteDesktop, SelectDevicesOptions, StartOptions,
};
use ashpd::desktop::screencast::{
    CursorMode, OpenPipeWireRemoteOptions, Screencast, SelectSourcesOptions, SourceType,
    StartCastOptions, Stream,
};
use ashpd::desktop::{CreateSessionOptions, PersistMode, Session};
use helio_input::InputError;
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};

use crate::remote_input::{Deliver, Op, RemoteSink};

enum Cmd {
    Input(Vec<Op>),
    Close,
}

/// What the desktop granted.
pub struct Granted {
    /// The PipeWire connection and the screen's stream on it.
    pub fd: OwnedFd,
    pub node: u32,
    /// The stream's size in the desktop's (logical) coordinates.
    pub size: Option<(u32, u32)>,
    /// Keyboard and pointer: a RemoteDesktop session.
    pub input: bool,
}

pub struct Portal {
    tx: UnboundedSender<Cmd>,
    /// Hung up when the session's task has finished (closed the session).
    done: std::sync::mpsc::Receiver<()>,
}

/// Where every portal call runs. One runtime for the life of the process:
/// ashpd keeps one D-Bus connection for all of them, and that connection
/// lives on the runtime that first made it (with a runtime per session, the
/// second session's calls waited forever on the first's, gone).
fn runtime() -> Result<&'static tokio::runtime::Runtime, String> {
    static RUNTIME: OnceLock<Result<tokio::runtime::Runtime, String>> = OnceLock::new();
    RUNTIME
        .get_or_init(|| {
            tokio::runtime::Builder::new_multi_thread()
                .worker_threads(1)
                .thread_name("portal")
                .enable_all()
                .build()
                .map_err(|e| e.to_string())
        })
        .as_ref()
        .map_err(Clone::clone)
}

impl Portal {
    /// A session on the desktop in `dir`'s name (its restore token). Blocks
    /// until the desktop grants it, or `wait` passes (the first time, someone
    /// has to answer the desktop's dialog).
    pub fn open(dir: &Path, wait: Duration) -> Result<(Portal, Granted), String> {
        let token_path = dir.join("portal-token");
        let (tx, rx) = unbounded_channel();
        let (ready_tx, ready_rx) = std::sync::mpsc::sync_channel(1);
        let (done_tx, done) = std::sync::mpsc::channel::<()>();
        runtime()?.spawn(async move {
            serve(token_path, rx, ready_tx).await;
            drop(done_tx);
        });
        let portal = Portal { tx, done };
        match ready_rx.recv_timeout(wait) {
            Ok(Ok(granted)) => Ok((portal, granted)),
            Ok(Err(e)) => Err(e),
            Err(_) => Err(format!(
                "the desktop did not allow screen sharing within {} s",
                wait.as_secs()
            )),
        }
    }

    /// The client's input, to this session: `picture` is where the
    /// picture sits in the stream, `size` the shared screen's size in the
    /// desktop's coordinates.
    pub fn sink(&self, picture: (u32, u32, u32, u32), size: Option<(u32, u32)>) -> RemoteSink {
        let tx = self.tx.clone();
        let deliver: Deliver = Box::new(move |ops| {
            tx.send(Cmd::Input(ops))
                .map_err(|_| InputError::Unavailable)
        });
        RemoteSink::new(deliver, picture, size)
    }
}

impl Drop for Portal {
    fn drop(&mut self) {
        let _ = self.tx.send(Cmd::Close);
        // The session closed before the next one opens (a moment; not
        // forever, if the portal stopped answering).
        let _ = self.done.recv_timeout(Duration::from_secs(5));
    }
}

enum Kind {
    Remote(RemoteDesktop, Session<RemoteDesktop>),
    Cast(Session<Screencast>),
}

async fn serve(
    token_path: PathBuf,
    mut rx: UnboundedReceiver<Cmd>,
    ready: std::sync::mpsc::SyncSender<Result<Granted, String>>,
) {
    let saved = std::fs::read_to_string(&token_path).unwrap_or_default();
    // Until the desktop answers, which may be never (a dialog nobody sees):
    // a close ends the wait.
    let started = tokio::select! {
        r = start(&saved) => r,
        _ = rx.recv() => return,
    };
    let (kind, stream, token) = match started {
        Ok(v) => v,
        Err(e) => {
            let _ = ready.send(Err(e));
            return;
        }
    };
    // Each token works once; the new one is for next time.
    if let Some(t) = token {
        if let Err(e) = std::fs::write(&token_path, t) {
            tracing::warn!(error = %e, "could not keep the portal's restore token");
        }
    }
    let node = stream.pipe_wire_node_id();
    let fd = match Screencast::new().await {
        Ok(sc) => match &kind {
            Kind::Remote(_, session) => {
                sc.open_pipe_wire_remote(session, OpenPipeWireRemoteOptions::default())
                    .await
            }
            Kind::Cast(session) => {
                sc.open_pipe_wire_remote(session, OpenPipeWireRemoteOptions::default())
                    .await
            }
        },
        Err(e) => Err(e),
    };
    let fd = match fd {
        Ok(fd) => fd,
        Err(e) => {
            let _ = ready.send(Err(format!("no PipeWire connection from the portal: {e}")));
            return;
        }
    };
    let size = stream
        .size()
        .map(|(w, h)| (w.max(1) as u32, h.max(1) as u32));
    let input = matches!(kind, Kind::Remote(..));
    tracing::info!(node, ?size, input, "the desktop is shared");

    // Keep the screen on while it streams.
    let inhibit = match InhibitProxy::new().await {
        Ok(p) => p
            .inhibit(
                None,
                enumflags2::BitFlags::from(InhibitFlags::Idle),
                InhibitOptions::default().set_reason("Helio Host is streaming this screen"),
            )
            .await
            .map_err(|e| tracing::info!(error = %e, "the screen may blank during the session"))
            .ok(),
        Err(e) => {
            tracing::info!(error = %e, "no Inhibit portal: the screen may blank during the \
                session");
            None
        }
    };

    let _ = ready.send(Ok(Granted {
        fd,
        node,
        size,
        input,
    }));
    while let Some(cmd) = rx.recv().await {
        match cmd {
            Cmd::Input(ops) => {
                if let Kind::Remote(rd, session) = &kind {
                    for op in ops {
                        if let Err(e) = apply(rd, session, node, op).await {
                            tracing::debug!(error = %e, ?op, "portal input");
                        }
                    }
                }
            }
            Cmd::Close => break,
        }
    }
    if let Some(r) = inhibit {
        let _ = r.close().await;
    }
    let _ = match &kind {
        Kind::Remote(_, s) => s.close().await,
        Kind::Cast(s) => s.close().await,
    };
}

/// A RemoteDesktop session where the desktop has the portal, else a
/// ScreenCast one; the stream, and the token for next time.
async fn start(saved: &str) -> Result<(Kind, Stream, Option<String>), String> {
    let restore = |kind: &str| saved.strip_prefix(kind).map(str::to_string);
    let cast_sources = || {
        SelectSourcesOptions::default()
            .set_cursor_mode(CursorMode::Embedded)
            .set_sources(enumflags2::BitFlags::from(SourceType::Monitor))
            .set_multiple(false)
    };
    match RemoteDesktop::new().await {
        Ok(rd) => {
            let token = restore("rd:");
            let session = rd
                .create_session(CreateSessionOptions::default())
                .await
                .map_err(|e| e.to_string())?;
            rd.select_devices(
                &session,
                SelectDevicesOptions::default()
                    .set_devices(DeviceType::Keyboard | DeviceType::Pointer)
                    .set_persist_mode(PersistMode::ExplicitlyRevoked)
                    .set_restore_token(token.as_deref()),
            )
            .await
            .and_then(|r| r.response())
            .map_err(|e| format!("choosing the devices: {e}"))?;
            let sc = Screencast::new().await.map_err(|e| e.to_string())?;
            sc.select_sources(&session, cast_sources())
                .await
                .and_then(|r| r.response())
                .map_err(|e| format!("choosing the screen: {e}"))?;
            let started = rd
                .start(&session, None, StartOptions::default())
                .await
                .and_then(|r| r.response())
                .map_err(|e| format!("the desktop did not allow sharing the screen: {e}"))?;
            let stream = started
                .streams()
                .first()
                .cloned()
                .ok_or("the desktop shared no screen")?;
            let token = started.restore_token().map(|t| format!("rd:{t}"));
            if !started.devices().contains(DeviceType::Pointer) {
                tracing::warn!("the desktop did not share the pointer");
            }
            Ok((Kind::Remote(rd, session), stream, token))
        }
        Err(e) => {
            tracing::info!(error = %e, "no RemoteDesktop portal: the screen alone, and no \
                input from the client");
            let token = restore("sc:");
            let sc = Screencast::new()
                .await
                .map_err(|e| format!("no ScreenCast portal either: {e}"))?;
            let session = sc
                .create_session(CreateSessionOptions::default())
                .await
                .map_err(|e| e.to_string())?;
            sc.select_sources(
                &session,
                cast_sources()
                    .set_persist_mode(PersistMode::ExplicitlyRevoked)
                    .set_restore_token(token.as_deref()),
            )
            .await
            .and_then(|r| r.response())
            .map_err(|e| format!("choosing the screen: {e}"))?;
            let started = sc
                .start(&session, None, StartCastOptions::default())
                .await
                .and_then(|r| r.response())
                .map_err(|e| format!("the desktop did not allow sharing the screen: {e}"))?;
            let stream = started
                .streams()
                .first()
                .cloned()
                .ok_or("the desktop shared no screen")?;
            let token = started.restore_token().map(|t| format!("sc:{t}"));
            Ok((Kind::Cast(session), stream, token))
        }
    }
}

async fn apply(
    rd: &RemoteDesktop,
    s: &Session<RemoteDesktop>,
    node: u32,
    op: Op,
) -> Result<(), ashpd::Error> {
    let state = |down: bool| {
        if down {
            KeyState::Pressed
        } else {
            KeyState::Released
        }
    };
    match op {
        Op::Key(code, down) => {
            rd.notify_keyboard_keycode(s, code, state(down), Default::default())
                .await
        }
        Op::Keysym(sym, down) => {
            rd.notify_keyboard_keysym(s, sym, state(down), Default::default())
                .await
        }
        Op::Motion(dx, dy) => {
            rd.notify_pointer_motion(s, dx, dy, Default::default())
                .await
        }
        Op::Absolute(x, y) => {
            rd.notify_pointer_motion_absolute(s, node, x, y, Default::default())
                .await
        }
        Op::Button(code, down) => {
            rd.notify_pointer_button(s, code, state(down), Default::default())
                .await
        }
        Op::Scroll(vertical, steps) => {
            let axis = if vertical {
                Axis::Vertical
            } else {
                Axis::Horizontal
            };
            rd.notify_pointer_axis_discrete(s, axis, steps, Default::default())
                .await
        }
    }
}

/// Whether this session is a Wayland one: the portal, not X11. `HELIO_HOST_CAPTURE`
/// (`x11` or `portal`) decides it where the environment does not tell.
pub fn wayland_session() -> bool {
    match std::env::var("HELIO_HOST_CAPTURE").as_deref() {
        Ok("x11") => false,
        Ok("portal") => true,
        _ => {
            std::env::var("XDG_SESSION_TYPE").as_deref() == Ok("wayland")
                || std::env::var_os("WAYLAND_DISPLAY").is_some()
        }
    }
}
