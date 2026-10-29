//! The Linux host's side of a session (see `session`): a virtual display
//! at the client's mode where the desktop makes one (`virtual_display`:
//! GNOME, KDE), else the screen as it is (the picture scaled to the
//! client's size and letterboxed); encoded with FFmpeg (VA-API, NVENC or
//! x264), and the sound through PulseAudio (or PipeWire's server for its
//! clients). Helio Host runs in the user's desktop session, as the user.
//!
//! Under Wayland the screen comes by PipeWire and input goes to the
//! compositor: its own interfaces for a virtual display, the desktop portal
//! (`portal`) for the screen as it is. Under X11 the screen comes through
//! MIT-SHM and input goes in through XTest.

use std::os::fd::OwnedFd;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use helio_capture::pipewire::{self, PipeWireCapture};
use helio_capture::pulse::PulseCapture;
use helio_capture::x11::X11Capture;
use helio_capture::{CaptureError, Frame, Grab, PixelOrder};
use helio_display::DisplayMode;
use helio_encode::ffmpeg::{self, Backend, FfmpegEncoder, Image};
use helio_encode::{Codec, EncodeError, EncodedFrame, EncoderConfig};
use helio_input::x11::XTestSink;
use helio_input::{InputError, InputSink};
use helio_proto::audio::FRAME_SAMPLES;
use helio_proto::control::{self, AckStatus, Control, SessionStart};
use helio_proto::input::InputEvent;
use helio_transport::{Endpoint, Peer};

use crate::audio::AudioHandle;
use crate::config::HostConfig;
use crate::negotiate::Negotiated;
use crate::portal::{KeepAwake, Portal};
use crate::power::Awake;
use crate::remote_input::RemoteSink;
use crate::session::Shared;
use crate::video::VideoParams;
use crate::virtual_display::{self, VirtualDisplay};

pub const HOST_KIND: u8 = control::host::LINUX;

/// How long a session start waits for the desktop to allow sharing the
/// screen: the first time, someone at the host answers a dialog.
const PORTAL_WAIT: Duration = Duration::from_secs(60);

/// What the video pipeline captures.
#[derive(Debug, Clone)]
pub enum VideoSource {
    /// The X screen.
    X11,
    /// A screen cast: the portal's PipeWire connection (else the
    /// session's PipeWire), the stream, and the one mode to ask it for (a
    /// virtual monitor that takes its size from the stream).
    PipeWire {
        fd: Option<Arc<OwnedFd>>,
        node: u32,
        mode: Option<pipewire::Mode>,
    },
}

enum SinkKind {
    X11(Box<XTestSink>),
    Remote(RemoteSink),
    /// Nowhere (no XTEST; a desktop that shares no input).
    None,
}

/// Where the client's input goes, counted so the log shows input arriving.
pub struct Sink {
    inner: SinkKind,
    received: u64,
}

impl InputSink for Sink {
    fn inject(&mut self, events: &[InputEvent]) -> Result<(), InputError> {
        self.received += events.len() as u64;
        match &mut self.inner {
            SinkKind::X11(s) => s.inject(events),
            SinkKind::Remote(s) => s.inject(events),
            SinkKind::None => Ok(()),
        }
    }

    fn release_all(&mut self) -> Result<(), InputError> {
        match &mut self.inner {
            SinkKind::X11(s) => s.release_all(),
            SinkKind::Remote(s) => s.release_all(),
            SinkKind::None => Ok(()),
        }
    }
}

/// A session's display: the screen, and where its picture sits in the
/// stream.
pub struct Display {
    source: VideoSource,
    picture: (u32, u32, u32, u32),
    screen: Screen,
}

/// What a session streams, held for as long as it does.
enum Screen {
    /// The X screen, kept on.
    X11 { _awake: Box<Awake> },
    /// Wayland: the portal session (screen, input, the screen kept on).
    Portal(SharedScreen),
    /// Wayland: a virtual display of the desktop's own, the screen kept on.
    Virtual {
        display: VirtualDisplay,
        _awake: KeepAwake,
    },
}

/// A screen the desktop shares through its portal.
struct SharedScreen {
    portal: Portal,
    /// Its size in the desktop's coordinates.
    size: Option<(u32, u32)>,
    /// Keyboard and pointer shared too.
    input: bool,
}

pub struct Platform {
    /// What this machine encodes, and with what, found at start.
    codecs: Vec<(Codec, Backend)>,
    /// The codecs it also encodes at 4:4:4 (NVENC, x264; not VA-API).
    yuv444: Vec<Codec>,
    data_dir: PathBuf,
    wayland: bool,
}

impl Platform {
    pub fn new(data_dir: &Path) -> Platform {
        let wayland = crate::portal::wayland_session();
        let codecs = FfmpegEncoder::probe();
        let yuv444: Vec<Codec> = codecs
            .iter()
            .map(|c| c.0)
            .filter(|c| FfmpegEncoder::probe_yuv444(*c))
            .collect();
        for (codec, backend) in &codecs {
            tracing::info!(
                codec = codec.name(),
                ?backend,
                yuv444 = yuv444.contains(codec),
                "can encode"
            );
        }
        if codecs.is_empty() {
            tracing::error!(
                "no video encoder: FFmpeg has none of VA-API, NVENC or libx264 working here"
            );
        }
        let virtual_display = wayland.then(virtual_display::offered).flatten();
        if let Some(desktop) = virtual_display {
            tracing::info!(
                ?desktop,
                "a Wayland session: sessions get a virtual display of the desktop's own"
            );
        } else if wayland {
            tracing::info!("a Wayland session: the screen and input go through the desktop portal");
            // Ask now, while someone is likely at the machine, rather than
            // when a client first connects from afar.
            if !data_dir.join("portal-token").exists() {
                let dir = data_dir.to_path_buf();
                std::thread::spawn(move || {
                    tracing::info!(
                        "allow Helio Host to share the screen in the desktop's dialog; it is asked once"
                    );
                    match Portal::open(&dir, Duration::from_secs(600)) {
                        Ok(_) => {
                            tracing::info!("screen sharing allowed; sessions will not ask again")
                        }
                        Err(e) => {
                            tracing::warn!(error = %e, "screen sharing not allowed yet; \
                                the first session will ask")
                        }
                    }
                });
            }
        }
        Platform {
            codecs,
            yuv444,
            data_dir: data_dir.to_path_buf(),
            wayland,
        }
    }

    pub fn supported_codecs(&self) -> Vec<Codec> {
        self.codecs.iter().map(|c| c.0).collect()
    }

    /// Anything that stops a session before it starts.
    pub fn preflight(&self) -> Result<(), AckStatus> {
        if self.wayland {
            return Ok(());
        }
        if let Err(e) = x11rb::connect(None) {
            tracing::error!(error = %e, "no X display to stream (Helio Host runs in the user's \
                desktop session); refusing the session");
            return Err(AckStatus::Failed);
        }
        Ok(())
    }

    pub fn refusal_note(&self, _status: AckStatus) -> Option<Control> {
        None
    }

    /// A client is starting a session.
    pub fn claim(&mut self) {}

    /// The display to stream: the screen, woken and kept on.
    /// What of `asked` (`control::video`) the encoder can stream with
    /// `codec`: 4:4:4 where NVENC or x264 encodes it; never HDR (the X
    /// server and the portal share SDR pictures).
    pub fn video_caps(&self, codec: Codec, asked: u8) -> u8 {
        if self.yuv444.contains(&codec) {
            asked & control::video::YUV444
        } else {
            0
        }
    }

    /// The HDR stream's metadata (never asked: no HDR here).
    pub fn hdr_metadata(&self, _d: &Display) -> control::HdrMetadata {
        control::HdrMetadata::bt2020(1000, 203)
    }

    pub fn display(
        &mut self,
        n: &Negotiated,
        req: &SessionStart,
        keep_host_displays: bool,
        _cfg: &HostConfig,
    ) -> Result<Display, AckStatus> {
        let (width, height) = (n.width, n.height);
        let (w, h) = (width as u32, height as u32);
        // Looked for at each session: the desktop may have started after
        // the host.
        if let Some(desktop) = self.wayland.then(virtual_display::offered).flatten() {
            let mode = DisplayMode {
                width,
                height,
                refresh_mhz: n.fps_mhz,
            };
            match virtual_display::make(desktop, mode, req.scale_pct, keep_host_displays) {
                Ok(made) => {
                    tracing::info!(
                        ?desktop,
                        width,
                        height,
                        fps = n.fps_mhz as f64 / 1000.0,
                        client_scale_pct = req.scale_pct,
                        keep_host_displays,
                        "streaming a virtual display"
                    );
                    return Ok(Display {
                        source: VideoSource::PipeWire {
                            fd: None,
                            node: made.node,
                            mode: made.mode,
                        },
                        picture: (0, 0, w, h),
                        screen: Screen::Virtual {
                            display: made.display,
                            _awake: crate::portal::keep_awake(),
                        },
                    });
                }
                Err(e) => {
                    tracing::warn!(error = %e, "no virtual display; streaming the desktop's \
                        shared screen")
                }
            }
        }
        if self.wayland {
            let (portal, granted) = Portal::open(&self.data_dir, PORTAL_WAIT).map_err(|e| {
                tracing::error!(error = %e, "the desktop did not share its screen");
                AckStatus::Failed
            })?;
            let picture = match granted.size {
                Some((sw, sh)) => ffmpeg::fit(sw, sh, w, h),
                None => (0, 0, w, h),
            };
            tracing::info!(stream = ?(width, height), ?picture, input = granted.input, "streaming the desktop's shared screen");
            return Ok(Display {
                source: VideoSource::PipeWire {
                    fd: Some(Arc::new(granted.fd)),
                    node: granted.node,
                    mode: None,
                },
                picture,
                screen: Screen::Portal(SharedScreen {
                    portal,
                    size: granted.size,
                    input: granted.input,
                }),
            });
        }
        let (conn, screen) = x11rb::connect(None).map_err(|e| {
            tracing::error!(error = %e, "no X display");
            AckStatus::Failed
        })?;
        let s = &x11rb::connection::Connection::setup(&conn).roots[screen];
        let (sw, sh) = (s.width_in_pixels as u32, s.height_in_pixels as u32);
        let picture = ffmpeg::fit(sw, sh, w, h);
        tracing::info!(screen = ?(sw, sh), stream = ?(width, height), ?picture, "streaming the X screen");
        Ok(Display {
            source: VideoSource::X11,
            picture,
            screen: Screen::X11 {
                _awake: Box::new(Awake::hold()),
            },
        })
    }

    pub fn video_params(
        &self,
        d: &Display,
        encoder: EncoderConfig,
        cfg: &HostConfig,
        _req: &SessionStart,
    ) -> VideoParams {
        VideoParams {
            source: d.source.clone(),
            encoder: EncoderConfig {
                two_pass: false,
                ..encoder
            },
            pace_mbps: cfg.pace_mbps.max(10),
        }
    }

    /// The video could not start on `d`.
    pub fn abandon(&mut self, d: Display) {
        drop(d);
    }

    pub fn input_sink(&self, d: &Display, _width: u16, _height: u16) -> Sink {
        let inner = match &d.screen {
            Screen::Virtual { display, .. } => SinkKind::Remote(display.sink(d.picture)),
            Screen::Portal(SharedScreen {
                portal,
                size,
                input: true,
            }) => SinkKind::Remote(portal.sink(d.picture, *size)),
            Screen::Portal(_) => SinkKind::None,
            Screen::X11 { .. } => XTestSink::new(None, d.picture)
                .map_or(SinkKind::None, |s| SinkKind::X11(Box::new(s))),
        };
        Sink { inner, received: 0 }
    }

    /// Input for the session is over.
    pub fn input_done(&self, sink: &Sink) {
        if sink.received > 0 {
            tracing::info!(events = sink.received, "input received");
        }
    }

    /// What the machine plays (the default output's monitor), in stereo.
    pub fn audio(
        &self,
        _d: &Display,
        req: &SessionStart,
        bitrate_kbps: u32,
        endpoint: Arc<Endpoint>,
        peer: Arc<Peer>,
    ) -> Option<(AudioHandle, u8)> {
        if req.audio_channels == 0 {
            return None;
        }
        let bitrate = helio_audio::bitrate_bps(crate::audio::CHANNELS, bitrate_kbps);
        let open = |chunks: crate::audio::Chunks| {
            PulseCapture::new(
                FRAME_SAMPLES,
                Box::new(move |pcm| chunks.send(|buf| buf.extend_from_slice(pcm))),
            )
            .map_err(|e| e.to_string())
        };
        match AudioHandle::start(open, crate::audio::CHANNELS, bitrate, endpoint, peer) {
            Ok(a) => Some((a, crate::audio::CHANNELS)),
            Err(e) => {
                tracing::warn!(error = %e, "audio unavailable; streaming video only");
                None
            }
        }
    }

    /// The session's pipeline is up, just before the client hears so.
    pub fn starting(&mut self, _peer: &Arc<Peer>, _shared: &Shared, _endpoint: &Arc<Endpoint>) {}

    /// The session runs.
    pub fn started(&mut self, _req: &SessionStart) {}

    /// Every tick while streaming: nothing more to tell the client.
    pub fn tick(&mut self, _d: &mut Display, _now: Instant, _second: bool) -> Vec<Control> {
        Vec::new()
    }

    /// The session is over (its portal session closes with it).
    pub fn ended(
        &mut self,
        display: Option<Display>,
        _ran: bool,
        _shutdown: bool,
        _shared: &Shared,
    ) {
        drop(display);
    }

    /// Between sessions.
    pub fn idle(&mut self) {}
}

/// How long since the X server last had input of any kind (XTest's
/// included): the session tells someone at the host from its agent by when
/// it last injected. None under Wayland, where no client may know.
pub fn host_input_idle() -> Option<Duration> {
    use x11rb::protocol::screensaver::ConnectionExt as _;
    if crate::portal::wayland_session() {
        return None;
    }
    thread_local! {
        static CONN: std::cell::RefCell<Option<(x11rb::rust_connection::RustConnection, u32)>> = const { std::cell::RefCell::new(None) };
    }
    CONN.with(|c| {
        let mut c = c.borrow_mut();
        if c.is_none() {
            let (conn, screen) = x11rb::connect(None).ok()?;
            let root = x11rb::connection::Connection::setup(&conn).roots[screen].root;
            *c = Some((conn, root));
        }
        let (conn, root) = c.as_ref()?;
        match conn
            .screensaver_query_info(*root)
            .ok()
            .and_then(|r| r.reply().ok())
        {
            Some(info) => Some(Duration::from_millis(info.ms_since_user_input as u64)),
            None => {
                *c = None;
                None
            }
        }
    })
}

/// The host's monitors stay as they are (a virtual display extends the
/// desktop): someone at the host sees them.
pub fn isolates_host_displays(_req: &SessionStart, _cfg: &HostConfig) -> bool {
    false
}

/// Linux cannot tell a locked screen generically (each desktop locks its
/// own way); the lock screen is streamed like anything else.
pub fn secure_screen() -> bool {
    false
}

/// The screen, as the video loop reads it.
pub enum Capture {
    X11(Box<X11Capture>),
    PipeWire(PipeWireCapture),
}

impl Capture {
    pub fn grab(&mut self, timeout_ms: u32) -> Result<Grab, CaptureError> {
        match self {
            Capture::X11(c) => c.grab(timeout_ms),
            Capture::PipeWire(c) => c.grab(timeout_ms),
        }
    }

    pub fn image(&self) -> Option<Frame> {
        match self {
            Capture::X11(c) => c.image(),
            Capture::PipeWire(c) => c.image(),
        }
    }
}

/// The encoder, taking the capture's images.
pub struct Encoder(FfmpegEncoder);

impl Encoder {
    pub fn submit(
        &mut self,
        image: &Frame,
        index: u64,
        force_idr: bool,
    ) -> Result<(), EncodeError> {
        let image = Image {
            data: image.bytes(),
            width: image.width,
            height: image.height,
            stride: image.stride,
            rgb: image.order == PixelOrder::Rgbx,
        };
        self.0.submit(&image, index, force_idr)
    }

    pub fn next(&mut self, timeout: Duration) -> Option<Result<EncodedFrame, EncodeError>> {
        self.0.next(timeout)
    }

    pub fn in_flight(&self) -> usize {
        self.0.in_flight()
    }

    pub fn acknowledge_through(&mut self, index: u64) {
        self.0.acknowledge_through(index)
    }

    pub fn invalidate(&mut self, first: u64, last: u64) -> bool {
        self.0.invalidate(first, last)
    }

    pub fn set_bitrate(&mut self, bitrate_bps: u32) -> Result<(), EncodeError> {
        self.0.set_bitrate(bitrate_bps)
    }
}

/// The screen's capture, and an encoder at the stream's size.
pub fn open_video(params: &VideoParams) -> Result<(Capture, Encoder), String> {
    let cap = match &params.source {
        // The pointer drawn in (`features::POINTER_IN_PICTURE`); the portal's
        // stream has it embedded.
        VideoSource::X11 => Capture::X11(Box::new(
            X11Capture::new(None, true).map_err(|e| e.to_string())?,
        )),
        VideoSource::PipeWire { fd, node, mode } => {
            let fd = fd
                .as_ref()
                .map(|fd| fd.try_clone())
                .transpose()
                .map_err(|e| format!("the PipeWire connection: {e}"))?;
            Capture::PipeWire(PipeWireCapture::new(fd, *node, *mode).map_err(|e| e.to_string())?)
        }
    };
    let enc = FfmpegEncoder::new(params.encoder).map_err(|e| e.to_string())?;
    Ok((cap, Encoder(enc)))
}
