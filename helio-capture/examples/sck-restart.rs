//! A capture macOS stops starts again: capture the main display and say each
//! second how many frames came. Stop ScreenCaptureKit meanwhile from another
//! terminal (`kill -9 $(pgrep replayd)`; launchd starts it again when next
//! asked): the log says the capture stopped, then that it runs again, and
//! frames come once more (the first image of the new stream at least).
//!   sck-restart [SECS] [--virtual]   the picture (of a display with nothing on it)
//!   sck-restart [SECS] --sound       the sound

#[cfg(target_os = "macos")]
fn main() {
    use std::time::{Duration, Instant};

    use helio_capture::sck::SckCapture;
    use helio_capture::Grab;

    tracing_subscriber::fmt()
        .with_env_filter("debug")
        .with_writer(std::io::stderr)
        .init();
    let secs: u64 = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(15);
    if std::env::args().any(|a| a == "--sound") {
        sound(secs);
        return;
    }
    // `--virtual`: a display of its own, with nothing on it, rather than
    // the main one: what a still desktop sends.
    let virtual_display = std::env::args().any(|a| a == "--virtual").then(|| {
        helio_display::macos::VirtualDisplay::new(
            helio_display::DisplayMode {
                width: 1280,
                height: 800,
                refresh_mhz: 60_000,
            },
            "Helio Host",
            false,
            false,
        )
        .expect("virtual display")
    });
    let display = virtual_display
        .as_ref()
        .map_or_else(SckCapture::main_display, |d| d.id);
    let mut cap = SckCapture::new(display, 1280, 800, 60_000, false, false).expect("capture");
    let run = Instant::now();
    let mut second = Instant::now();
    let mut frames = 0;
    let mut heard = cap.heard();
    while run.elapsed() < Duration::from_secs(secs) {
        if let Ok(Grab::Frame) = cap.grab(100) {
            frames += 1;
        }
        if second.elapsed() >= Duration::from_secs(1) {
            println!(
                "{:>3} s: {frames} frames, {} buffers",
                run.elapsed().as_secs(),
                cap.heard() - heard
            );
            frames = 0;
            heard = cap.heard();
            second = Instant::now();
        }
    }
}

/// The system's sound: buffers a second.
#[cfg(target_os = "macos")]
fn sound(secs: u64) {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    use helio_capture::sck::{SckAudio, SckCapture};

    let heard = Arc::new(AtomicU64::new(0));
    let count = heard.clone();
    let _audio = SckAudio::new(
        SckCapture::main_display(),
        Box::new(move |_, _| {
            count.fetch_add(1, Ordering::Relaxed);
        }),
    )
    .expect("sound capture");
    let run = Instant::now();
    let mut last = 0;
    while run.elapsed() < Duration::from_secs(secs) {
        std::thread::sleep(Duration::from_secs(1));
        let now = heard.load(Ordering::Relaxed);
        println!("{:>3} s: {} buffers", run.elapsed().as_secs(), now - last);
        last = now;
    }
}

#[cfg(not(target_os = "macos"))]
fn main() {}
