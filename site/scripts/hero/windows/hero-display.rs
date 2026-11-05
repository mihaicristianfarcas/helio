//! The Windows hero's screen (capture-hero.sh): a 2880x1800 virtual display
//! as the whole desktop, the host's monitors off, as a stream has it
//! (helio_display::windows::WindowsDisplay), until C:\pp\hero\done exists,
//! at most 180 s; then the desktop as it was. Copy it into
//! helio-display/examples/ and `cargo build --release -p helio-display
//! --example hero-display` on the PC. It prints ACTIVE once the display is
//! the desktop.
#[cfg(windows)]
fn main() {
    use std::time::{Duration, Instant};

    use helio_display::windows::WindowsDisplay;
    use helio_display::{DisplayControl, DisplayMode};

    let state = std::env::temp_dir().join("helio-hero-display-state.toml");
    WindowsDisplay::restore_stale(&state);
    let mode = DisplayMode {
        width: 2880,
        height: 1800,
        refresh_mhz: 120_000,
    };
    // A display added right after another was removed can list no modes
    // for a moment: put everything back and try again, a few times.
    let mut display = WindowsDisplay::new(state.clone());
    let mut tries = 0;
    loop {
        tries += 1;
        let active = display.activate(mode);
        println!("activate {tries}: {active:?}");
        if active.is_ok() {
            println!("ACTIVE");
            break;
        }
        println!("restore: {:?}", display.restore());
        if tries == 5 {
            println!("GAVE UP");
            return;
        }
        std::thread::sleep(Duration::from_secs(3));
        display = WindowsDisplay::new(state.clone());
    }
    let done = std::path::Path::new(r"C:\pp\hero\done");
    let start = Instant::now();
    while !done.exists() && start.elapsed() < Duration::from_secs(180) {
        std::thread::sleep(Duration::from_millis(250));
    }
    println!("restore: {:?}", display.restore());
}

#[cfg(not(windows))]
fn main() {}
