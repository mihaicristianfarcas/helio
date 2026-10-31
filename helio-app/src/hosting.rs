//! Hosting in the window: This computer while hosting is off (one page that
//! says what hosting is and holds the switch), the switch's row (in that
//! page, Settings → Hosting and setup), and what the system must allow for
//! it, each with a line on why:
//!
//! - macOS: Screen Recording and Accessibility, granted to the host (Helio Host,
//!   the helper the LaunchAgent runs), which asks for them with macOS's own
//!   prompts as it starts. What macOS allows the host is what the host
//!   reports (`api::Grants`); each not allowed has a button to its pane of
//!   System Settings, and the row changes as soon as the host says so.
//! - Windows: NVIDIA's encoder, used when it is here (without it: software
//!   H.264), and the drivers turning hosting on installs where they are
//!   missing: SudoVDA's virtual display (without it: the main display
//!   scaled) and ViGEmBus's controllers (`docs/platforms/windows.md`).
//! - Linux: nothing to grant ahead (GNOME and KDE make each session a
//!   display of its own; elsewhere the desktop portal asks on Wayland when a
//!   device connects).

use gpui::{div, prelude::*, px, AnyElement, Context, SharedString};
use helio_ui::{
    button, footnote, notice, page_header, rows, section, setting, spinner, switch, IconName, Ink,
    Metrics, Theme,
};

use crate::app::{HelioApp, Page};
use crate::host::switch::{Busy, State};

/// A Mac's Privacy & Security panes, by what they grant.
#[cfg(target_os = "macos")]
const SCREEN_PANE: &str =
    "x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture";
#[cfg(target_os = "macos")]
const ACCESSIBILITY_PANE: &str =
    "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility";

/// What a Windows host has to stream with.
#[cfg_attr(not(windows), allow(dead_code))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Equipment {
    /// NVIDIA's encoder (`nvEncodeAPI64.dll`, installed with the driver).
    pub nvidia: bool,
    /// SudoVDA, the virtual display driver.
    pub sudovda: bool,
    /// ViGEmBus, the virtual controllers' driver.
    pub vigembus: bool,
}

/// What this Windows computer has (looked at as the page is drawn: files
/// and a device list, all quick).
#[cfg(windows)]
pub fn equipment() -> Equipment {
    let system = std::env::var_os("SystemRoot")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from(r"C:\Windows"))
        .join("System32");
    Equipment {
        nvidia: system.join("nvEncodeAPI64.dll").is_file(),
        sudovda: helio_display::sudovda::present(),
        vigembus: system.join("drivers").join("ViGEmBus.sys").is_file(),
    }
}

/// The lines the check says: (it is there, what, why). The drivers are
/// installed by turning hosting on (`helio-host/src/hosting/drivers.rs`),
/// so one missing while hosting is on did not install.
#[cfg_attr(not(windows), allow(dead_code))]
fn equipment_lines(e: Equipment, hosting: bool) -> [(bool, &'static str, &'static str); 3] {
    [
        if e.nvidia {
            (
                true,
                "NVIDIA graphics",
                "Streams in HEVC or AV1, encoded by the graphics card (NVENC).",
            )
        } else {
            (
                false,
                "No NVIDIA graphics",
                "Streams in H.264, encoded in software (Media Foundation): it works, at a \
                 lower quality for the bitrate and with more of the processor.",
            )
        },
        match (e.sudovda, hosting) {
            (true, _) => (
                true,
                "SudoVDA",
                "A virtual display at each device's own resolution and refresh rate.",
            ),
            (false, false) => (
                false,
                "SudoVDA, with hosting",
                "Turning hosting on installs it, for a virtual display at each device's own \
                 resolution (on x64 PCs: it is not built for Arm).",
            ),
            (false, true) => (
                false,
                "No SudoVDA",
                "Streams the main display, scaled to each device: SudoVDA is built for x64 \
                 PCs only, or did not install (turn hosting off and on to try again).",
            ),
        },
        match (e.vigembus, hosting) {
            (true, _) => (
                true,
                "ViGEmBus",
                "Each device's controllers become Xbox controllers on this PC.",
            ),
            (false, false) => (
                false,
                "ViGEmBus, with hosting",
                "Turning hosting on installs it, for each device's controllers.",
            ),
            (false, true) => (
                false,
                "No ViGEmBus",
                "Takes no controllers: ViGEmBus did not install (turn hosting off and on to \
                 try again).",
            ),
        },
    ]
}

impl HelioApp {
    /// This computer, hosting off: that it is off, what hosting is, and
    /// the way to the switch (Settings → Hosting, its one place).
    pub fn render_hosting_page(&mut self, t: Theme, cx: &mut Context<Self>) -> AnyElement {
        let device = crate::app::device_word();
        let off = setting(
            "Hosting is off",
            Some("Settings → Hosting turns it on.".into()),
            button("open-hosting-settings", "Hosting Settings…", t).on_click(cx.listener(
                |this, _, _, cx| this.set_page(Page::Settings(crate::settings::Tab::Hosting), cx),
            )),
            t,
        );
        let content = div()
            .flex()
            .flex_col()
            .gap(px(20.0))
            .child(page_header(
                "This computer",
                Some(format!("Let your other devices play this {device}.").into()),
                None,
                t,
            ))
            .child(
                section("", t)
                    .child(rows([off.into_any_element()], t))
                    .child(footnote(
                        format!(
                            "On, this {device} runs the host whenever you are signed in: \
                             devices you pair stream its screen and sound, and use its \
                             keyboard, mouse and controllers, as each one is allowed. Off, \
                             nothing on it listens."
                        ),
                        t,
                    )),
            );
        let scroll = self.page_scroll.clone();
        helio_ui::page("hosting-page", &scroll, Metrics::PAGE, t, content)
    }

    /// Hosting's switch, with what it is doing, or why it did not.
    pub fn hosting_switch_row(&mut self, t: Theme, cx: &mut Context<Self>) -> AnyElement {
        let (state, busy, error, found) = {
            let h = self.hosting.read(cx);
            (
                h.state.clone(),
                h.busy,
                h.error.clone(),
                h.program().is_some(),
            )
        };
        let detail: SharedString = match (&busy, &state) {
            (Some(Busy::Elevating { .. }), _) => {
                "Waiting for Windows: choose Yes to let Helio change the host's service.".into()
            }
            (Some(Busy::Switching { on: true }), _) => "Turning hosting on…".into(),
            (Some(Busy::Switching { on: false }), _) => "Turning hosting off…".into(),
            (None, State::Unknown) => "Looking…".into(),
            (None, State::Off) => "Off: no host runs on this computer.".into(),
            (None, State::Starting) => "On: the host is starting.".into(),
            (None, State::On { api: true }) => {
                "On: the host runs whenever you are signed in.".into()
            }
            (None, State::On { api: false }) => {
                "On: the host runs, but does not answer yet.".into()
            }
            (None, State::Failed(why)) => format!("On, but the host does not run: {why}.").into(),
        };
        let on = match busy {
            Some(Busy::Switching { on } | Busy::Elevating { on }) => on,
            None => state.enabled(),
        };
        let control: AnyElement = if busy.is_some() {
            div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .child(spinner("hosting-busy", 13.0, t.tertiary))
                .child(switch("hosting-switch", on, t).disabled(true))
                .into_any_element()
        } else {
            let hosting = self.hosting.clone();
            switch("hosting-switch", on, t)
                .disabled(!found || state == State::Unknown)
                .on_toggle(move |on, _, cx| hosting.update(cx, |h, cx| h.turn(on, cx)))
                .into_any_element()
        };
        let label = format!("Let my devices play this {}", crate::app::device_word());
        let row = setting(label, Some(detail), control, t);
        match error {
            Some(e) => div()
                .flex()
                .flex_col()
                .child(row)
                .child(div().px(px(12.0)).pb(px(10.0)).child(notice(
                    IconName::Warning,
                    Ink::DANGER,
                    e,
                    t,
                )))
                .into_any_element(),
            None => row.into_any_element(),
        }
    }

    /// What the system must allow, or has, for hosting: a row each, with
    /// why; none where nothing is asked ahead (Linux).
    pub fn hosting_needs_rows(&mut self, t: Theme, cx: &mut Context<Self>) -> Vec<AnyElement> {
        #[cfg(target_os = "macos")]
        {
            let enabled = self.hosting.read(cx).state.enabled();
            let grants = self
                .host
                .as_ref()
                .and_then(|h| h.read(cx).snapshot().status.as_ref().and_then(|s| s.grants));
            let row = |id: &'static str,
                       glyph: IconName,
                       name: &'static str,
                       why: &'static str,
                       granted: Option<bool>,
                       pane: &'static str| {
                let control: AnyElement = match (enabled, granted) {
                    (false, _) => helio_ui::chip("Asked when hosting starts", t.tertiary, t)
                        .into_any_element(),
                    (true, None) => helio_ui::chip("Looking…", t.tertiary, t).into_any_element(),
                    (true, Some(true)) => helio_ui::chip("Allowed", t.fresh, t).into_any_element(),
                    (true, Some(false)) => button(id, "Open System Settings…", t)
                        .on_click(move |_, _, cx| cx.open_url(pane))
                        .into_any_element(),
                };
                div()
                    .min_h(px(Metrics::SETTINGS_ROW))
                    .px(px(12.0))
                    .py(px(8.0))
                    .flex()
                    .items_center()
                    .gap(px(12.0))
                    .child(helio_ui::tile(glyph, t))
                    .child(helio_ui::label_stack(name, Some(why.into()), t))
                    .child(div().flex_none().child(control))
                    .into_any_element()
            };
            vec![
                row(
                    "grant-screen",
                    IconName::Display,
                    "Screen Recording",
                    "For your devices to see this Mac's screen. Once it is allowed, the host \
                     restarts when you come back to Helio.",
                    grants.map(|g| g.screen),
                    SCREEN_PANE,
                ),
                row(
                    "grant-accessibility",
                    IconName::Pointer,
                    "Accessibility",
                    "For their keyboard, mouse and controllers to act on this Mac.",
                    grants.map(|g| g.accessibility),
                    ACCESSIBILITY_PANE,
                ),
            ]
        }
        #[cfg(windows)]
        {
            let hosting = self.hosting.read(cx).state.enabled();
            equipment_lines(equipment(), hosting)
                .into_iter()
                .map(|(here, name, why)| {
                    div()
                        .min_h(px(Metrics::SETTINGS_ROW))
                        .px(px(12.0))
                        .py(px(8.0))
                        .flex()
                        .items_center()
                        .gap(px(12.0))
                        .child(helio_ui::tile(
                            if here {
                                IconName::CheckCircle
                            } else {
                                IconName::Warning
                            },
                            t,
                        ))
                        .child(helio_ui::label_stack(name, Some(why.into()), t))
                        .into_any_element()
                })
                .collect()
        }
        #[cfg(not(any(target_os = "macos", windows)))]
        {
            let _ = (t, cx);
            Vec::new()
        }
    }

    /// Settings → Hosting: the switch and what it needs (the host's own
    /// settings follow, while hosting is on: `HostPages::settings_for`).
    pub fn hosting_settings(&mut self, t: Theme, cx: &mut Context<Self>) -> AnyElement {
        let switch = self.hosting_switch_row(t, cx);
        let needs = self.hosting_needs_rows(t, cx);
        div()
            .flex()
            .flex_col()
            .gap(px(20.0))
            .child(section("", t).child(rows([switch], t)).child(footnote(
                "Quitting Helio never stops the host: only this switch turns it off.",
                t,
            )))
            .when(!needs.is_empty(), |d| {
                d.child(section("What it needs", t).child(rows(needs, t)))
            })
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn without_nvidia_the_check_says_it_streams_in_software_h264() {
        let [nvenc, display, _] = equipment_lines(
            Equipment {
                nvidia: false,
                sudovda: true,
                vigembus: true,
            },
            true,
        );
        assert!(!nvenc.0 && nvenc.2.contains("H.264, encoded in software"));
        assert!(display.0);
        let [nvenc, display, _] = equipment_lines(
            Equipment {
                nvidia: true,
                sudovda: false,
                vigembus: true,
            },
            true,
        );
        assert!(nvenc.0 && !display.0 && display.2.contains("scaled"));
    }

    #[test]
    fn a_missing_driver_is_one_hosting_installs_until_hosting_is_on() {
        let none = Equipment {
            nvidia: true,
            sudovda: false,
            vigembus: false,
        };
        let [_, display, pads] = equipment_lines(none, false);
        assert!(display.2.starts_with("Turning hosting on installs it"));
        assert!(pads.2.starts_with("Turning hosting on installs it"));
        let [_, display, pads] = equipment_lines(none, true);
        assert!(display.2.contains("did not install") && pads.2.contains("did not install"));
    }
}
