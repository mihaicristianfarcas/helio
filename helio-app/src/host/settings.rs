//! The host's own settings, kept by the host, each where the app's settings
//! of its kind are: the host's name, sessions and network in Settings →
//! Hosting under the switch; its encoder and limits in Video; its clipboard
//! in Input; what agents may do on it in Agent setup. Each saves as it
//! changes. They exist only while hosting is on: a computer that only plays
//! has no host to set.

use gpui::{div, prelude::*, px, AnyElement, Context, ElementId};
use helio_ui::{
    field, footnote, notice, rows, section, select, setting, stepper, switch, IconName, Ink, Theme,
};
use serde_json::json;

use super::worker::Conn;
use super::HostPages;
use crate::settings::Tab;

impl HostPages {
    /// The host's settings that go on the settings page `tab`, after the
    /// app's own; none where it has none. On Hosting, a line instead while
    /// the host does not answer.
    pub fn settings_for(&mut self, tab: Tab, t: Theme, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let waiting = match (&self.snap.conn, self.config()) {
            (Conn::Ready, Some(_)) => None,
            (Conn::Ready, None) => Some("Reading the host's settings…"),
            _ => Some(
                "The host's settings show here once it answers: This computer → Overview \
                 says why it does not.",
            ),
        };
        if let Some(line) = waiting {
            return match tab {
                Tab::Hosting => vec![footnote(line, t).into_any_element()],
                _ => Vec::new(),
            };
        }
        let mut out = Vec::new();
        let pending = self.snap.status.as_ref().and_then(|s| s.restart.as_deref());
        if let Some(line) = restart_line(pending, self.restarting.is_some()) {
            out.push(notice(IconName::Refresh, Ink::ATTENTION, line, t).into_any_element());
        }
        if let Some((glyph, tint, text, _)) = self.note.clone() {
            out.push(notice(glyph, tint, text, t).into_any_element());
        }
        let sections = match tab {
            Tab::Hosting => self.hosting_sections(t, cx),
            Tab::Video => self.video_sections(t, cx),
            Tab::Input => self.input_sections(t, cx),
            Tab::Agents => self.agent_sections(t, cx),
            Tab::General | Tab::Audio | Tab::Updates => return Vec::new(),
        };
        out.extend(sections);
        out
    }

    /// The device word, for "this PC" and "this Mac".
    fn here(&self) -> &'static str {
        if self.os() == "macos" {
            "Mac"
        } else {
            "PC"
        }
    }

    /// Settings → Hosting: the host's name and sessions, and how devices
    /// reach it.
    fn hosting_sections(&mut self, t: Theme, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let mut out = Vec::new();
        out.push(
            section("Host", t)
                .child(rows(
                    [
                        setting(
                            "Name",
                            Some(
                                "Shown to your devices. Press Return to save: Helio Host \
                                 restarts to apply it, once the stream ends if one runs."
                                    .into(),
                            ),
                            div().w(px(200.0)).child(field(&self.name, t)),
                            t,
                        )
                        .into_any_element(),
                        self.toggle(
                            "allow_takeover",
                            "Let a device take over a session",
                            "Otherwise a second device is refused while one streams. A \
                             device also needs Take over in its permissions (Devices).",
                            t,
                            cx,
                        ),
                    ],
                    t,
                ))
                .into_any_element(),
        );
        out.push(
            section("Network", t)
                .child(rows(
                    [
                        self.toggle(
                            "internet_access",
                            "Allow streaming over the internet",
                            "Paired devices connect from anywhere, with no port forwarding: \
                             this host's public address is published sealed, readable only \
                             by them.",
                            t,
                            cx,
                        ),
                        self.toggle(
                            "port_mapping",
                            "Ask the router to forward the port",
                            "UPnP or NAT-PMP, as Sunshine's UPnP option does. Helps where hole \
                             punching cannot; a router behind another NAT (the ISP's box) \
                             cannot help, and is left alone.",
                            t,
                            cx,
                        ),
                    ],
                    t,
                ))
                .into_any_element(),
        );
        out.push(
            section("Ports", t)
                .child(rows(
                    [
                        ("Tunnel (UDP)", 0usize),
                        ("Pairing (TCP)", 1),
                        ("API, this computer only (TCP)", 2),
                    ]
                    .into_iter()
                    .map(|(label, i)| {
                        setting(
                            label,
                            None,
                            div().w(px(90.0)).child(field(&self.ports[i], t)),
                            t,
                        )
                        .into_any_element()
                    }),
                    t,
                ))
                .child(footnote(
                    "Press Return in a field to save it: Helio Host restarts to apply it, once \
                     the stream ends if one runs.",
                    t,
                ))
                .into_any_element(),
        );
        out
    }

    /// Settings → Video: how this computer encodes for the devices that
    /// play it, and what they may ask of it.
    fn video_sections(&mut self, t: Theme, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let mac = self.os() == "macos";
        let here = self.here();
        let mut encoder = vec![self.toggle(
            "keep_host_displays",
            if mac {
                "Keep this Mac's displays on while streaming"
            } else {
                "Keep this PC's monitors on while streaming"
            },
            "Off: the virtual display becomes the whole desktop, so windows open where the \
             device sees them.",
            t,
            cx,
        )];
        if !mac {
            let presets: Vec<String> = (1..=7)
                .map(|p| match p {
                    1 => "P1 (fastest)".to_string(),
                    7 => "P7 (best quality)".to_string(),
                    p => format!("P{p}"),
                })
                .collect();
            let preset = self.cfg_u64("nvenc_preset").clamp(1, 7) as usize - 1;
            let this = cx.weak_entity();
            encoder.push(
                setting(
                    "NVENC preset",
                    Some("Lower is faster. Sunshine uses P1.".into()),
                    select("nvenc-preset", presets, Some(preset), t)
                        .width(170.0)
                        .on_select(move |i, _, cx| {
                            let _ = this.update(cx, |app, cx| {
                                app.set_config("nvenc_preset", json!(i + 1), cx)
                            });
                        }),
                    t,
                )
                .into_any_element(),
            );
            encoder.push(self.toggle(
                "nvenc_two_pass",
                "Two-pass encoding",
                "A quarter-resolution first pass for better bit allocation, at no extra \
                 latency.",
                t,
                cx,
            ));
            encoder.push(self.toggle(
                "nvidia_max_power",
                "Full GPU power for Helio Host",
                "The NVIDIA driver keeps its clocks up for Helio Host instead of lowering them \
                 between frames, which slows the encoder. Helio Host restarts to apply it.",
                t,
                cx,
            ));
            encoder.push(self.toggle(
                "nvidia_dxgi_present",
                "OpenGL and Vulkan through DXGI",
                "Full-screen OpenGL and Vulkan games are captured at their full frame rate. \
                 Changes the driver's setting for every program while Helio Host runs, and puts it \
                 back when it stops. Helio Host restarts to apply it.",
                t,
                cx,
            ));
        }
        encoder.push(self.toggle(
            "allow_hevc",
            "Allow HEVC",
            "Better quality per bit than H.264, when the device decodes it.",
            t,
            cx,
        ));
        if !mac {
            encoder.push(self.toggle(
                "allow_av1",
                "Allow AV1",
                "When both this GPU and the device support it.",
                t,
                cx,
            ));
        }
        encoder.push(self.toggle(
            "allow_hdr",
            "Allow HDR",
            "Stream HDR to a device that asks and shows it: HEVC Main10, from an NVIDIA GPU \
             or a Mac on macOS 15 or later.",
            t,
            cx,
        ));
        if !mac {
            encoder.push(self.toggle(
                "allow_yuv444",
                "Allow YUV 4:4:4",
                "Full-resolution colour for a device that asks: sharper text, for about a \
                 fifth more bitrate.",
                t,
                cx,
            ));
        }
        let limits = vec![
            self.choice(
                "max_bitrate_kbps",
                "Maximum bitrate",
                "What a device may ask for.",
                &[0, 20_000, 50_000, 80_000, 100_000, 150_000],
                |v| {
                    if v == 0 {
                        "The device's choice".into()
                    } else {
                        format!("{} Mbps", v / 1000)
                    }
                },
                t,
                cx,
            ),
            self.choice(
                "max_fps",
                "Maximum frame rate",
                "What a device may ask for.",
                &[0, 30, 60, 90, 120, 144],
                |v| {
                    if v == 0 {
                        "The device's choice".into()
                    } else {
                        format!("{v} FPS")
                    }
                },
                t,
                cx,
            ),
            self.choice(
                "pace_mbps",
                "Send pacing",
                "Frames leave spread at this rate instead of in one burst. Lower it for Wi-Fi \
                 or slow uplinks.",
                &[100, 200, 400, 800, 1000],
                |v| format!("{v} Mbit/s"),
                t,
                cx,
            ),
            self.toggle(
                "adaptive_bitrate",
                "Adapt the bitrate to the network",
                "Lower it when the network congests, climb back when it clears. Random loss is \
                 left to FEC.",
                t,
                cx,
            ),
        ];
        vec![
            section(format!("Streaming from this {here}"), t)
                .child(rows(encoder, t))
                .child(footnote("Changes apply to the next session.", t))
                .into_any_element(),
            section(format!("What devices may ask of this {here}"), t)
                .child(rows(limits, t))
                .into_any_element(),
        ]
    }

    /// Settings → Input: what this computer shares with the devices that
    /// play it.
    fn input_sections(&mut self, t: Theme, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let here = self.here();
        vec![section(format!("Devices playing this {here}"), t)
            .child(rows(
                [self.toggle(
                    "clipboard",
                    "Share this computer's clipboard",
                    "Text, images and files copied on either side can be pasted on the other, \
                     when the device asks for it too. Password managers' copies are never \
                     shared.",
                    t,
                    cx,
                )],
                t,
            ))
            .into_any_element()]
    }

    /// Settings → Agent setup: what AI agents paired with this computer may
    /// do on it (each one's own permissions are on Devices).
    fn agent_sections(&mut self, t: Theme, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let here = self.here();
        let hold = self.cfg_u64("agent_local_input_hold_secs") as u32;
        let this = cx.weak_entity();
        vec![section(format!("Agents using this {here}"), t)
            .child(rows(
                [
                    self.toggle(
                        "agents",
                        "Allow paired AI agents",
                        "Agents pair like devices and are marked as agents. They never take \
                         over a person, and a person always takes over from them. Each one's \
                         permissions are on Devices.",
                        t,
                        cx,
                    ),
                    setting(
                        "Hold after local input",
                        Some(
                            "Using this computer's own keyboard or mouse holds an agent's for \
                             this many seconds. 0: never hold."
                                .into(),
                        ),
                        stepper("hold", hold, 0..=120, 5, t, move |v, _, cx| {
                            let _ = this.update(cx, |app, cx| {
                                app.set_config("agent_local_input_hold_secs", json!(v), cx)
                            });
                        }),
                        t,
                    )
                    .into_any_element(),
                ],
                t,
            ))
            .into_any_element()]
    }

    pub(super) fn toggle(
        &self,
        key: &'static str,
        label: &str,
        detail: &str,
        t: Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let this = cx.weak_entity();
        setting(
            label.to_string(),
            Some(detail.to_string().into()),
            switch(ElementId::Name(key.into()), self.cfg_bool(key), t).on_toggle(
                move |on, _, cx| {
                    let _ = this.update(cx, |app, cx| app.set_config(key, json!(on), cx));
                },
            ),
            t,
        )
        .into_any_element()
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn choice(
        &self,
        key: &'static str,
        label: &str,
        detail: &str,
        values: &[u64],
        name: impl Fn(u64) -> String,
        t: Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let current = self.cfg_u64(key);
        let mut values = values.to_vec();
        if !values.contains(&current) {
            values.push(current);
            values.sort_unstable();
        }
        let names: Vec<String> = values.iter().map(|v| name(*v)).collect();
        let index = values.iter().position(|v| *v == current);
        let this = cx.weak_entity();
        setting(
            label.to_string(),
            Some(detail.to_string().into()),
            select(key, names, index, t)
                .width(170.0)
                .on_select(move |i, _, cx| {
                    let v = values[i];
                    let _ = this.update(cx, |app, cx| app.set_config(key, json!(v), cx));
                }),
            t,
        )
        .into_any_element()
    }
}

/// The line over the host's settings while a restart is ahead: `pending`,
/// the host's word on it ("after_session", "now"); `asked_here`, a change
/// made in this window restarts it and the host has not answered again.
fn restart_line(pending: Option<&str>, asked_here: bool) -> Option<&'static str> {
    match (pending, asked_here) {
        (Some("after_session"), _) => {
            Some("Helio Host restarts when the stream ends, to apply a change.")
        }
        (Some(_), _) | (None, true) => Some("Helio Host is restarting to apply a change…"),
        (None, false) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::restart_line;

    #[test]
    fn a_restart_waiting_for_the_stream_says_so() {
        assert_eq!(
            restart_line(Some("after_session"), true),
            Some("Helio Host restarts when the stream ends, to apply a change.")
        );
    }

    #[test]
    fn a_restart_under_way_says_so_until_the_host_is_back() {
        let line = Some("Helio Host is restarting to apply a change…");
        assert_eq!(restart_line(Some("now"), false), line);
        // Asked here, and the host is gone meanwhile: still under way.
        assert_eq!(restart_line(None, true), line);
    }

    #[test]
    fn no_restart_ahead_says_nothing() {
        assert_eq!(restart_line(None, false), None);
    }
}
