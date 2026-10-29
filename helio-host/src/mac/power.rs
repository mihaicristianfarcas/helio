//! Keep a Mac streaming: its display must be awake to be captured at all
//! (ScreenCaptureKit offers no display while it sleeps), and a remote user
//! resets no idle timer. As `caffeinate -u` does: declare user activity to
//! wake the display, and hold an assertion against display sleep for the
//! session.
//!
//! A MacBook also sleeps when its lid closes, a few seconds after, and no
//! assertion stops that: only an external display with the charger plugged
//! in does (measured on battery, a session's virtual display up: asleep 4 s
//! after the lid closed, "Clamshell Sleep" in `pmset -g log`). `LidOpen`
//! asks the kernel not to sleep on the lid for the session (IOPMrootDomain's
//! `kPMSetClamshellSleepState`, the unprivileged call Amphetamine's
//! Closed-Display Mode makes), so the stream goes on with the lid closed,
//! on battery too.

use std::ffi::c_void;
use std::path::{Path, PathBuf};

use objc2_core_foundation::CFString;

type IOPMAssertionID = u32;
const K_IOPM_USER_ACTIVE_LOCAL: u32 = 0;
const K_IOPM_ASSERTION_LEVEL_ON: u32 = 255;

#[link(name = "IOKit", kind = "framework")]
extern "C" {
    fn IOPMAssertionDeclareUserActivity(
        name: *const c_void,
        user_type: u32,
        id: *mut IOPMAssertionID,
    ) -> i32;
    fn IOPMAssertionCreateWithName(
        kind: *const c_void,
        level: u32,
        name: *const c_void,
        id: *mut IOPMAssertionID,
    ) -> i32;
    fn IOPMAssertionRelease(id: IOPMAssertionID) -> i32;
}

/// Held for a session: the display stays on until it is dropped.
pub struct Awake {
    activity: IOPMAssertionID,
    no_sleep: IOPMAssertionID,
}

impl Awake {
    /// Wake the display (as a key press would) and keep it awake.
    pub fn hold() -> Awake {
        let name = CFString::from_str("Helio Host is streaming this Mac");
        let kind = CFString::from_str("PreventUserIdleDisplaySleep");
        let (mut activity, mut no_sleep) = (0, 0);
        unsafe {
            let rc = IOPMAssertionDeclareUserActivity(
                cf(&name),
                K_IOPM_USER_ACTIVE_LOCAL,
                &mut activity,
            );
            if rc != 0 {
                tracing::warn!(rc, "could not wake the display");
            }
            let rc = IOPMAssertionCreateWithName(
                cf(&kind),
                K_IOPM_ASSERTION_LEVEL_ON,
                cf(&name),
                &mut no_sleep,
            );
            if rc != 0 {
                tracing::warn!(rc, "could not keep the display awake");
            }
        }
        Awake { activity, no_sleep }
    }
}

impl Drop for Awake {
    fn drop(&mut self) {
        unsafe {
            for id in [self.no_sleep, self.activity] {
                if id != 0 {
                    IOPMAssertionRelease(id);
                }
            }
        }
    }
}

/// IOPMrootDomain's user-client method that turns sleeping on a closed lid
/// off (1) and on (0): `kPMSetClamshellSleepState` in the SDK's
/// `IOKit/pwr_mgt/IOPMLibDefs.h`.
const K_PM_SET_CLAMSHELL_SLEEP_STATE: u32 = 12;

#[link(name = "IOKit", kind = "framework")]
extern "C" {
    fn IOServiceMatching(name: *const std::ffi::c_char) -> *mut c_void;
    fn IOServiceGetMatchingService(main_port: u32, matching: *mut c_void) -> u32;
    fn IOServiceOpen(service: u32, task: u32, kind: u32, connect: *mut u32) -> i32;
    fn IOServiceClose(connect: u32) -> i32;
    fn IOObjectRelease(object: u32) -> i32;
    fn IOConnectCallScalarMethod(
        connect: u32,
        selector: u32,
        input: *const u64,
        input_count: u32,
        output: *mut u64,
        output_count: *mut u32,
    ) -> i32;
}

extern "C" {
    /// This task's port: what the `mach_task_self()` macro reads.
    static mach_task_self_: u32;
}

/// Tell the kernel whether a closed lid sleeps the Mac. The setting outlives
/// this process (until a reboot), so it is always put back (`LidOpen`).
fn lid_sleeps(sleeps: bool) -> Result<(), i32> {
    // SAFETY: IOKit calls on objects made and released here; the method
    // takes one scalar and returns none.
    unsafe {
        let service = IOServiceGetMatchingService(0, IOServiceMatching(c"IOPMrootDomain".as_ptr()));
        if service == 0 {
            return Err(-1);
        }
        let mut connect = 0u32;
        let rc = IOServiceOpen(service, mach_task_self_, 0, &mut connect);
        IOObjectRelease(service);
        if rc != 0 {
            return Err(rc);
        }
        let disabled = u64::from(!sleeps);
        let rc = IOConnectCallScalarMethod(
            connect,
            K_PM_SET_CLAMSHELL_SLEEP_STATE,
            &disabled,
            1,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        );
        IOServiceClose(connect);
        if rc != 0 {
            return Err(rc);
        }
    }
    Ok(())
}

/// Held for a session: the Mac does not sleep when its lid closes.
///
/// powerd turns the setting back on when a charger or a display comes or
/// goes (the session's own virtual display among them), so it is said again
/// each second (`renew`). The kernel holds it, not this process, so a
/// marker file says it is held: a host that crashed puts it back at its
/// next start (`put_back_left_over`). One setting for the whole system: a
/// tool with a closed-lid mode of its own (Amphetamine) finds it off after a
/// session, until it says it again.
pub struct LidOpen {
    marker: PathBuf,
}

impl LidOpen {
    /// The name of the marker file in the host's data folder.
    const MARKER: &str = "lid-open";

    pub fn hold(data_dir: &Path) -> LidOpen {
        let marker = data_dir.join(Self::MARKER);
        let _ = std::fs::write(&marker, b"");
        match lid_sleeps(false) {
            Ok(()) => tracing::info!("the stream goes on with the lid closed"),
            Err(rc) => tracing::warn!(rc, "closing the lid will sleep the Mac and stop the stream"),
        }
        LidOpen { marker }
    }

    /// Say it again (once a second).
    pub fn renew(&self) {
        let _ = lid_sleeps(false);
    }

    /// A host that ended without putting the lid back (a crash, a kill):
    /// put it back now.
    pub fn put_back_left_over(data_dir: &Path) {
        let marker = data_dir.join(Self::MARKER);
        if marker.exists() {
            tracing::info!("a closed lid sleeps the Mac again (left off by the last run)");
            let _ = lid_sleeps(true);
            let _ = std::fs::remove_file(marker);
        }
    }
}

impl Drop for LidOpen {
    fn drop(&mut self) {
        if let Err(rc) = lid_sleeps(true) {
            tracing::warn!(rc, "could not let a closed lid sleep the Mac again");
        }
        let _ = std::fs::remove_file(&self.marker);
    }
}

fn cf(s: &CFString) -> *const c_void {
    s as *const CFString as *const c_void
}
