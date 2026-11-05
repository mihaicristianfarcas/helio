//! macOS: ScreenCaptureKit, the capture macOS offers for whole displays.
//!
//! The stream is configured the way the rest of the pipeline wants its
//! frames: already in NV12 at video range with the BT.709 matrix (what
//! VideoToolbox encodes without a conversion, and what Helio's renderer
//! assumes), at the stream's size, never faster than the stream's frame rate.
//! ScreenCaptureKit only delivers a frame when the display changed, so, as
//! with Desktop Duplication, `grab` waits for news with a timeout and the
//! session repeats the last image when there is none.
//!
//! A stream can stop for good under a session: a MacBook's lid closing and
//! the Mac sleeping left the client on the picture of that moment for the
//! rest of the session, input still working. macOS says so to the stream's
//! delegate only sometimes: with `replayd` (which serves every capture)
//! killed, the stream delivered nothing more and the delegate heard
//! nothing (measured on macOS 26.5). A stream that runs is never silent,
//! though: the sound comes without a break (silence too), and the picture
//! hands over a buffer without an image ("idle") many times a second while
//! the display is still (measured: ~38 a second at 60 fps). So a stream
//! that hands over nothing for `QUIET` has stopped, said so or not. It is
//! started again with the same settings until it takes (`RESTART_EVERY`);
//! the picture keeps its last image meanwhile, so the client sees it held
//! rather than lost.

use std::ptr::NonNull;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use block2::RcBlock;
use dispatch2::{DispatchQueue, DispatchRetained};
use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2::{define_class, msg_send, AllocAnyThread, DefinedClass};
use objc2_core_foundation::CFRetained;
use objc2_core_graphics::{kCGDisplayStreamYCbCrMatrix_ITU_R_709_2, CGMainDisplayID};
use objc2_core_media::{
    CMAudioFormatDescriptionGetStreamBasicDescription, CMSampleBuffer, CMTime, CMTimeFlags,
};
use objc2_core_video::{kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange, CVPixelBuffer};
use objc2_foundation::{NSArray, NSError, NSObject, NSObjectProtocol};
use objc2_screen_capture_kit::{
    SCContentFilter, SCDisplay, SCShareableContent, SCStream, SCStreamConfiguration,
    SCStreamConfigurationPreset, SCStreamDelegate, SCStreamOutput, SCStreamOutputType,
};

use crate::{CaptureError, Grab};

/// How often a stream macOS stopped is started again until it takes: the
/// display it captures is often not offered for capture again for a second
/// or two (a lid just closed, a display waking).
const RESTART_EVERY: Duration = Duration::from_secs(1);

/// A stream this long without a buffer has stopped: a picture's hands over
/// ~38 a second on a still display, a sound's 50 (silence too; both
/// measured on macOS 26.5, the picture at 60 fps).
const QUIET: Duration = Duration::from_secs(2);

/// The newest frame ScreenCaptureKit delivered, and how many it has.
#[derive(Default)]
struct Latest {
    slot: Mutex<(u64, Option<CFRetained<CVPixelBuffer>>)>,
    fresh: Condvar,
    /// Every buffer the stream handed over, an image in it or not.
    heard: std::sync::atomic::AtomicU64,
}

define_class!(
    #[unsafe(super(NSObject))]
    #[name = "HelioHostFrameOutput"]
    #[ivars = Arc<Latest>]
    struct FrameOutput;

    unsafe impl NSObjectProtocol for FrameOutput {}

    unsafe impl SCStreamOutput for FrameOutput {
        #[unsafe(method(stream:didOutputSampleBuffer:ofType:))]
        unsafe fn did_output(&self, _stream: &SCStream, sample: &CMSampleBuffer, kind: SCStreamOutputType) {
            if kind != SCStreamOutputType::Screen {
                return;
            }
            let latest = self.ivars();
            latest.heard.fetch_add(1, Ordering::Relaxed);
            // An unchanged display still reports in ("idle"), without an image.
            let Some(image) = (unsafe { sample.image_buffer() }) else { return };
            let mut slot = latest.slot.lock().unwrap_or_else(|e| e.into_inner());
            slot.0 += 1;
            slot.1 = Some(image);
            latest.fresh.notify_all();
        }
    }
);

impl FrameOutput {
    fn new(latest: Arc<Latest>) -> Retained<Self> {
        let this = Self::alloc().set_ivars(latest);
        unsafe { msg_send![super(this), init] }
    }
}

define_class!(
    /// Hears that macOS stopped a stream (the stream holds its delegate
    /// weakly: `Running` keeps it).
    #[unsafe(super(NSObject))]
    #[name = "HelioStreamWatch"]
    #[ivars = Arc<AtomicBool>]
    struct StreamWatch;

    unsafe impl NSObjectProtocol for StreamWatch {}

    unsafe impl SCStreamDelegate for StreamWatch {
        #[unsafe(method(stream:didStopWithError:))]
        unsafe fn did_stop(&self, _stream: &SCStream, error: &NSError) {
            tracing::warn!(
                error = %describe(error),
                code = error.code(),
                "macOS stopped the capture; starting it again"
            );
            self.ivars().store(true, Ordering::Release);
        }
    }
);

impl StreamWatch {
    fn new(stopped: Arc<AtomicBool>) -> Retained<Self> {
        let this = Self::alloc().set_ivars(stopped);
        // SAFETY: NSObject's init on a freshly allocated object.
        unsafe { msg_send![super(this), init] }
    }
}

/// Holds an Objective-C object across the completion handler's thread.
struct Handoff<T>(T);
// SAFETY: the objects handed over are only used again on the receiving
// thread, after the completion handler has let go of them.
unsafe impl<T> Send for Handoff<T> {}

/// A started ScreenCaptureKit stream, stopped when dropped.
struct Running {
    stream: Retained<SCStream>,
    _watch: Retained<StreamWatch>,
    _queue: DispatchRetained<DispatchQueue>,
    /// macOS stopped it, and said so (`StreamWatch`).
    stopped: Arc<AtomicBool>,
    /// It handed over nothing for `QUIET`: stopped without a word.
    quiet: AtomicBool,
}

// SAFETY: ScreenCaptureKit objects are not thread-affine; the stream is
// only stopped (once, on drop) by whoever owns this.
unsafe impl Send for Running {}

impl Running {
    /// Start capturing `display_id` as `config` says, into `output`.
    fn start(
        display_id: u32,
        config: &SCStreamConfiguration,
        output: &ProtocolObject<dyn SCStreamOutput>,
        kind: SCStreamOutputType,
        queue_name: &str,
    ) -> Result<Running, CaptureError> {
        let display = find_display(display_id)?;
        let stopped = Arc::new(AtomicBool::new(false));
        let watch = StreamWatch::new(stopped.clone());
        // SAFETY: ScreenCaptureKit's documented calls on objects made here;
        // the delegate and queue live as long as the stream (`Running`).
        unsafe {
            let filter = SCContentFilter::initWithDisplay_excludingWindows(
                SCContentFilter::alloc(),
                &display,
                &NSArray::new(),
            );
            let stream = SCStream::initWithFilter_configuration_delegate(
                SCStream::alloc(),
                &filter,
                config,
                Some(ProtocolObject::from_ref(&*watch)),
            );
            let queue = DispatchQueue::new(queue_name, None);
            stream
                .addStreamOutput_type_sampleHandlerQueue_error(output, kind, Some(&queue))
                .map_err(|e| {
                    CaptureError::Platform(format!("adding the stream output: {}", describe(&e)))
                })?;
            wait(|done| stream.startCaptureWithCompletionHandler(Some(&done)))
                .map_err(|e| CaptureError::Platform(format!("starting capture: {e}")))?;
            Ok(Running {
                stream,
                _watch: watch,
                _queue: queue,
                stopped,
                quiet: AtomicBool::new(false),
            })
        }
    }

    fn stopped(&self) -> bool {
        self.stopped.load(Ordering::Acquire) || self.quiet.load(Ordering::Acquire)
    }

    /// Stopped without a word from macOS: it hands over nothing (`QUIET`).
    fn mark_stopped(&self, what: &str) {
        if !self.quiet.swap(true, Ordering::AcqRel) {
            tracing::warn!(
                what,
                "the capture stopped without a word from macOS; starting it again"
            );
        }
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        if self.stopped.load(Ordering::Acquire) {
            return;
        }
        // One that went quiet may only be stuck: stopped too, but not
        // waited on for long (a dead one never answers).
        let within = if self.quiet.load(Ordering::Acquire) {
            Duration::from_millis(500)
        } else {
            Duration::from_secs(10)
        };
        // SAFETY: stops a stream this owns, once.
        let _ = wait_within(within, |done| unsafe {
            self.stream.stopCaptureWithCompletionHandler(Some(&done))
        });
    }
}

/// A stream macOS stopped, started again once `RESTART_EVERY` has passed
/// since the last try. True once it runs again.
fn restart(
    running: &mut Option<Running>,
    next_try: &mut Instant,
    what: &str,
    start: impl FnOnce() -> Result<Running, CaptureError>,
) -> bool {
    if running.as_ref().is_some_and(|r| !r.stopped()) {
        return true;
    }
    let now = Instant::now();
    if now < *next_try {
        return false;
    }
    *next_try = now + RESTART_EVERY;
    // The old stream first: two on one display can fail to start.
    *running = None;
    match start() {
        Ok(r) => {
            tracing::info!(what, "capture running again");
            *running = Some(r);
            true
        }
        Err(e) => {
            tracing::debug!(what, error = %e, "capture not back yet");
            false
        }
    }
}

/// What a picture capture was opened with, to open it again.
#[derive(Clone, Copy)]
struct VideoSettings {
    display_id: u32,
    width: u32,
    height: u32,
    fps_mhz: u32,
    cursor: bool,
    hdr: bool,
}

impl VideoSettings {
    fn config(&self) -> Retained<SCStreamConfiguration> {
        // SAFETY: setters on a configuration made here, with values in
        // their documented ranges.
        unsafe {
            let config = if self.hdr {
                // ScreenCaptureKit's HDR10 preset: `x420` (10-bit 4:2:0,
                // video range), BT.2100 PQ, the BT.2020 matrix, tone-mapped
                // for a canonical HDR display rather than for the host's:
                // what an HDR10 encoder takes as it is.
                let c = SCStreamConfiguration::streamConfigurationWithPreset(
                    SCStreamConfigurationPreset::CaptureHDRRecordingPreservedSDRHDR10,
                );
                tracing::info!(
                    pixel_format = %fourcc(c.pixelFormat()),
                    colour_space = %c.colorSpaceName(),
                    "capturing in HDR10"
                );
                c
            } else {
                let c = SCStreamConfiguration::new();
                c.setPixelFormat(kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange);
                c.setColorMatrix(kCGDisplayStreamYCbCrMatrix_ITU_R_709_2);
                c
            };
            config.setWidth(self.width as usize);
            config.setHeight(self.height as usize);
            config.setMinimumFrameInterval(CMTime {
                value: 1000,
                timescale: self.fps_mhz.max(1000) as i32,
                flags: CMTimeFlags::Valid,
                epoch: 0,
            });
            config.setShowsCursor(self.cursor);
            // Enough in flight that the encoder holding one never stalls capture.
            config.setQueueDepth(5);
            config
        }
    }
}

/// Captures one display.
pub struct SckCapture {
    running: Option<Running>,
    output: Retained<FrameOutput>,
    settings: VideoSettings,
    next_try: Instant,
    /// The count of buffers last seen (`Latest::heard`), and since when.
    heard: (u64, Instant),
    latest: Arc<Latest>,
    seen: u64,
    size: (u32, u32),
}

// SAFETY: as `Running`; the output object is only handed to new streams by
// whoever owns this.
unsafe impl Send for SckCapture {}

impl SckCapture {
    /// The display macOS calls main (the one with the menu bar).
    pub fn main_display() -> u32 {
        CGMainDisplayID()
    }

    /// Capture `display_id` scaled to `width`x`height`, at most `fps_mhz`
    /// thousandths of a frame a second. `cursor`: draw the pointer into the
    /// image. `hdr`: HDR10, 10-bit BT.2020 PQ (macOS 15 and later).
    pub fn new(
        display_id: u32,
        width: u32,
        height: u32,
        fps_mhz: u32,
        cursor: bool,
        hdr: bool,
    ) -> Result<SckCapture, CaptureError> {
        let settings = VideoSettings {
            display_id,
            width,
            height,
            fps_mhz,
            cursor,
            hdr,
        };
        // Shared with the capture queue's callback. The pixel buffers in it
        // are CoreFoundation objects, which are safe to retain and release
        // from any thread; the mutex orders the rest.
        #[allow(clippy::arc_with_non_send_sync)]
        let latest = Arc::new(Latest::default());
        let output = FrameOutput::new(latest.clone());
        let mut capture = SckCapture {
            running: None,
            output,
            settings,
            next_try: Instant::now(),
            heard: (0, Instant::now()),
            latest,
            seen: 0,
            size: (width, height),
        };
        capture.running = Some(capture.start()?);
        Ok(capture)
    }

    fn start(&self) -> Result<Running, CaptureError> {
        Running::start(
            self.settings.display_id,
            &self.settings.config(),
            ProtocolObject::from_ref(&*self.output),
            SCStreamOutputType::Screen,
            "helio-host.capture",
        )
    }

    /// Wait up to `timeout_ms` for a frame newer than the last one grabbed.
    /// A stream macOS stopped is started again first, when it is time to
    /// try; until it runs, this times out.
    pub fn grab(&mut self, timeout_ms: u32) -> Result<Grab, CaptureError> {
        let deadline = Instant::now() + Duration::from_millis(timeout_ms as u64);
        if self.running.as_ref().is_none_or(|r| r.stopped()) {
            let (mut running, mut next_try) = (self.running.take(), self.next_try);
            let started = restart(&mut running, &mut next_try, "picture", || self.start());
            (self.running, self.next_try) = (running, next_try);
            if !started {
                std::thread::sleep(deadline.saturating_duration_since(Instant::now()));
                return Ok(Grab::Timeout);
            }
            self.heard.1 = Instant::now();
        }
        let heard = self.latest.heard.load(Ordering::Relaxed);
        if heard != self.heard.0 {
            self.heard = (heard, Instant::now());
        } else if self.heard.1.elapsed() >= QUIET {
            if let Some(r) = &self.running {
                r.mark_stopped("picture");
            }
        }
        let mut slot = self.latest.slot.lock().unwrap_or_else(|e| e.into_inner());
        while slot.0 == self.seen {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Ok(Grab::Timeout);
            }
            slot = self
                .latest
                .fresh
                .wait_timeout(slot, left)
                .unwrap_or_else(|e| e.into_inner())
                .0;
        }
        self.seen = slot.0;
        Ok(Grab::Frame)
    }

    /// The newest image, if any has arrived.
    pub fn image(&self) -> Option<CFRetained<CVPixelBuffer>> {
        self.latest
            .slot
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .1
            .clone()
    }

    pub fn size(&self) -> (u32, u32) {
        self.size
    }

    /// Buffers the stream has handed over, with an image or without.
    pub fn heard(&self) -> u64 {
        self.latest.heard.load(Ordering::Relaxed)
    }
}

/// The ScreenCaptureKit display with this CoreGraphics id.
fn find_display(display_id: u32) -> Result<Retained<SCDisplay>, CaptureError> {
    let (tx, rx) = std::sync::mpsc::sync_channel(1);
    let handler = RcBlock::new(
        move |content: *mut SCShareableContent, error: *mut NSError| {
            let found = match unsafe { content.as_ref() } {
                Some(content) => {
                    let displays = unsafe { content.displays() };
                    let ids: Vec<u32> = displays.iter().map(|d| unsafe { d.displayID() }).collect();
                    let display = displays
                        .iter()
                        .find(|d| unsafe { d.displayID() } == display_id);
                    Ok((display.map(Handoff), ids))
                }
                None => Err(unsafe { error.as_ref() }
                    .map(describe)
                    .unwrap_or_else(|| "no shareable content".into())),
            };
            let _ = tx.send(found);
        },
    );
    unsafe { SCShareableContent::getShareableContentWithCompletionHandler(&handler) };
    match rx.recv_timeout(Duration::from_secs(10)) {
        Ok(Ok((Some(Handoff(display)), _))) => Ok(display),
        // Asleep displays (and a locked screen's) are not offered for capture.
        Ok(Ok((None, ids))) => Err(CaptureError::NoSuchOutput(format!(
            "display {display_id} (capturable: {ids:?})"
        ))),
        // Most often: this process (or the terminal it runs in) lacks the
        // Screen Recording permission.
        Ok(Err(e)) => Err(CaptureError::Platform(format!("listing displays: {e}"))),
        Err(_) => Err(CaptureError::Platform("listing displays timed out".into())),
    }
}

/// Call `start` with a completion handler and wait for it.
fn wait(start: impl FnOnce(RcBlock<dyn Fn(*mut NSError)>)) -> Result<(), String> {
    wait_within(Duration::from_secs(10), start)
}

/// `wait`, for at most `within`.
fn wait_within(
    within: Duration,
    start: impl FnOnce(RcBlock<dyn Fn(*mut NSError)>),
) -> Result<(), String> {
    let (tx, rx) = std::sync::mpsc::sync_channel(1);
    let done = RcBlock::new(move |error: *mut NSError| {
        let _ = tx.send(unsafe { error.as_ref() }.map(describe));
    });
    start(done);
    match rx.recv_timeout(within) {
        Ok(None) => Ok(()),
        Ok(Some(e)) => Err(e),
        Err(_) => Err("timed out".into()),
    }
}

fn describe(e: &NSError) -> String {
    e.localizedDescription().to_string()
}

/// Where captured sound goes: interleaved 32-bit float frames, and how many
/// channels they have. Called on ScreenCaptureKit's queue.
pub type AudioSink = Box<dyn Fn(&[f32], usize) + Send + Sync>;

/// The sound capture's sink, and how many buffers it has had (the watch
/// tells a stopped stream by a gap).
struct AudioIvars {
    sink: AudioSink,
    heard: Arc<std::sync::atomic::AtomicU64>,
}

define_class!(
    #[unsafe(super(NSObject))]
    #[name = "HelioHostAudioOutput"]
    #[ivars = AudioIvars]
    struct AudioOutput;

    unsafe impl NSObjectProtocol for AudioOutput {}

    unsafe impl SCStreamOutput for AudioOutput {
        #[unsafe(method(stream:didOutputSampleBuffer:ofType:))]
        unsafe fn did_output(
            &self,
            _stream: &SCStream,
            sample: &CMSampleBuffer,
            kind: SCStreamOutputType,
        ) {
            if kind != SCStreamOutputType::Audio {
                return;
            }
            self.ivars().heard.fetch_add(1, Ordering::Relaxed);
            if let Some((pcm, channels)) = unsafe { interleaved(sample) } {
                (self.ivars().sink)(&pcm, channels);
            }
        }
    }
);

impl AudioOutput {
    fn new(sink: AudioSink, heard: Arc<std::sync::atomic::AtomicU64>) -> Retained<Self> {
        let this = Self::alloc().set_ivars(AudioIvars { sink, heard });
        unsafe { msg_send![super(this), init] }
    }
}

/// The sample's float PCM, interleaved (ScreenCaptureKit delivers one plane
/// per channel), and its channel count.
unsafe fn interleaved(sample: &CMSampleBuffer) -> Option<(Vec<f32>, usize)> {
    const FLOAT: u32 = 1; // kAudioFormatFlagIsFloat
    const NON_INTERLEAVED: u32 = 1 << 5; // kAudioFormatFlagIsNonInterleaved
    let format = unsafe { sample.format_description() }?;
    let asbd = unsafe { CMAudioFormatDescriptionGetStreamBasicDescription(&format).as_ref() }?;
    if asbd.mFormatFlags & FLOAT == 0 || asbd.mBitsPerChannel != 32 {
        return None;
    }
    let channels = asbd.mChannelsPerFrame.max(1) as usize;
    let frames = unsafe { sample.num_samples() }.max(0) as usize;
    let block = unsafe { sample.data_buffer() }?;
    let len = unsafe { block.data_length() };
    let mut raw = vec![0f32; len / 4];
    let status =
        unsafe { block.copy_data_bytes(0, raw.len() * 4, NonNull::new(raw.as_mut_ptr().cast())?) };
    if status != 0 || raw.len() < frames * channels {
        return None;
    }
    if asbd.mFormatFlags & NON_INTERLEAVED == 0 {
        raw.truncate(frames * channels);
        return Some((raw, channels));
    }
    let mut out = vec![0f32; frames * channels];
    for c in 0..channels {
        let plane = &raw[c * frames..(c + 1) * frames];
        for (i, &v) in plane.iter().enumerate() {
            out[i * channels + c] = v;
        }
    }
    Some((out, channels))
}

/// The sound capture's stream, and its output to start a new one with.
struct AudioStream {
    running: Option<Running>,
    output: Retained<AudioOutput>,
    /// Buffers the output has had.
    heard: Arc<std::sync::atomic::AtomicU64>,
    display_id: u32,
}

// SAFETY: as `Running`; the output's sink is Send + Sync.
unsafe impl Send for AudioStream {}

impl AudioStream {
    fn start(&self) -> Result<Running, CaptureError> {
        // SAFETY: as `VideoSettings::config`.
        unsafe {
            let config = SCStreamConfiguration::new();
            // The picture is not wanted: as small and as rare as allowed.
            config.setWidth(2);
            config.setHeight(2);
            config.setMinimumFrameInterval(CMTime {
                value: 1,
                timescale: 1,
                flags: CMTimeFlags::Valid,
                epoch: 0,
            });
            config.setCapturesAudio(true);
            config.setSampleRate(48_000);
            config.setChannelCount(2);
            config.setExcludesCurrentProcessAudio(true);
            Running::start(
                self.display_id,
                &config,
                ProtocolObject::from_ref(&*self.output),
                SCStreamOutputType::Audio,
                "helio-host.audio-capture",
            )
        }
    }
}

/// Captures what the Mac plays (every app but this one), 48 kHz stereo.
/// Nothing pulls from it as `grab` does from the picture, so a thread of
/// its own starts its stream again when macOS stops it.
pub struct SckAudio {
    stop: Arc<AtomicBool>,
    watcher: Option<std::thread::JoinHandle<()>>,
}

impl SckAudio {
    /// ScreenCaptureKit captures sound alongside a display; any capturable
    /// one will do (`display_id`).
    pub fn new(display_id: u32, sink: AudioSink) -> Result<SckAudio, CaptureError> {
        let heard = Arc::new(std::sync::atomic::AtomicU64::new(0));
        let mut audio = AudioStream {
            running: None,
            output: AudioOutput::new(sink, heard.clone()),
            heard,
            display_id,
        };
        audio.running = Some(audio.start()?);
        let stop = Arc::new(AtomicBool::new(false));
        let watcher = {
            let stop = stop.clone();
            std::thread::Builder::new()
                .name("audio-capture-watch".into())
                .spawn(move || {
                    let mut next_try = Instant::now();
                    // The count of buffers last seen, and since when.
                    let mut heard = (0, Instant::now());
                    while !stop.load(Ordering::Relaxed) {
                        std::thread::sleep(Duration::from_millis(100));
                        let now = audio.heard.load(Ordering::Relaxed);
                        if now != heard.0 {
                            heard = (now, Instant::now());
                        } else if heard.1.elapsed() >= QUIET {
                            if let Some(r) = &audio.running {
                                r.mark_stopped("sound");
                            }
                        }
                        if audio.running.as_ref().is_none_or(|r| r.stopped()) {
                            let mut running = audio.running.take();
                            if restart(&mut running, &mut next_try, "sound", || audio.start()) {
                                heard.1 = Instant::now();
                            }
                            audio.running = running;
                        }
                    }
                    // Dropping `audio` stops its stream.
                })
                .map_err(|e| CaptureError::Platform(format!("audio capture watch: {e}")))?
        };
        Ok(SckAudio {
            stop,
            watcher: Some(watcher),
        })
    }
}

impl Drop for SckAudio {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(w) = self.watcher.take() {
            let _ = w.join();
        }
    }
}

/// A pixel format's four characters, for the log.
fn fourcc(format: u32) -> String {
    String::from_utf8_lossy(&format.to_be_bytes()).into_owned()
}
