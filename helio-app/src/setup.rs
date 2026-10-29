//! Setup, the whole window, in asides' setup (`window.rs`'s `SetupStep`):
//! the first time the app opens, or from Settings → General. Its steps
//! along the top, the step, and Back, Skip and Continue at the bottom; each
//! step fades in as the others do.
//!
//! The steps: what the app is; what this computer is for (play other
//! computers from it, let devices play it, or both); for hosting, what the
//! system must allow, each said in a line before it is asked (on a Mac,
//! Screen Recording and Accessibility for the host; on Windows, the
//! administrator UAC asks for, and whether there are NVIDIA graphics and
//! SudoVDA); how to pair from the other computer; done.
//!
//! The step reached is saved as it is reached, so setup goes on from there
//! after a restart (a Mac asks for one once Screen Recording is allowed).
//! Skipped, the defaults stay (play only, hosting as it was), and each
//! permission is asked for when it is first needed. Playing only never asks
//! for the screen, the input or an administrator.

use gpui::{div, prelude::*, px, AnyElement, Context, FontWeight, SharedString, Window};
use helio_ui::{
    button, choice_row, footnote, icon, page_header, rows, section, ButtonKind, IconName, Metrics,
    Theme,
};

use crate::app::{HelioApp, Page};
use crate::prefs::Prefs;

/// Setup's column (asides' `SETUP_WIDTH`).
const WIDTH: f32 = 560.0;

/// Setup's steps, in order.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Step {
    Welcome,
    Use,
    Permissions,
    Pairing,
    Done,
}

impl Step {
    const ALL: [Step; 5] = [
        Step::Welcome,
        Step::Use,
        Step::Permissions,
        Step::Pairing,
        Step::Done,
    ];

    /// Its name in the settings (and HELIO_UI_DEMO's `setup=`).
    pub fn name(self) -> &'static str {
        match self {
            Step::Welcome => "welcome",
            Step::Use => "use",
            Step::Permissions => "permissions",
            Step::Pairing => "pairing",
            Step::Done => "done",
        }
    }

    pub fn named(name: &str) -> Option<Step> {
        Step::ALL.into_iter().find(|s| s.name() == name)
    }

    /// What the progress along the top calls it.
    fn title(self) -> &'static str {
        match self {
            Step::Use => "What for",
            Step::Permissions => "Permissions",
            Step::Pairing => "Pairing",
            Step::Welcome | Step::Done => "",
        }
    }
}

/// What this computer is for.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Use {
    /// Play my other computers from here: the client only.
    Play,
    /// Let my devices play this one: hosting.
    Host,
    Both,
}

impl Use {
    const ALL: [Use; 3] = [Use::Play, Use::Host, Use::Both];

    pub fn name(self) -> &'static str {
        match self {
            Use::Play => "play",
            Use::Host => "host",
            Use::Both => "both",
        }
    }

    pub fn named(name: &str) -> Option<Use> {
        Use::ALL.into_iter().find(|u| u.name() == name)
    }

    pub fn hosts(self) -> bool {
        self != Use::Play
    }
}

/// Where setup is.
pub struct Setup {
    pub step: Step,
    /// What was chosen; none yet: playing only (the default).
    pub use_for: Option<Use>,
    /// It opens the window (not done yet).
    showing: bool,
}

impl Setup {
    /// Where setup was left: at the step saved, until it is over.
    /// HELIO_UI_DEMO's `setup[=STEP]` opens it at a step.
    pub fn resume(prefs: &Prefs) -> Setup {
        let demo = std::env::var("HELIO_UI_DEMO").ok().and_then(|spec| {
            spec.split(',').map(str::trim).find_map(|part| match part {
                "setup" => Some(Step::Welcome),
                _ => part.strip_prefix("setup=").and_then(Step::named),
            })
        });
        let saved = prefs.setup_step.as_deref().and_then(Step::named);
        Setup {
            step: demo.or(saved).unwrap_or(Step::Welcome),
            use_for: prefs.use_for.as_deref().and_then(Use::named),
            showing: demo.is_some() || !prefs.onboarded,
        }
    }

    pub fn showing(&self) -> bool {
        self.showing
    }

    fn hosts(&self) -> bool {
        self.use_for.is_some_and(Use::hosts)
    }

    /// The steps this setup goes through: Permissions only for hosting.
    fn steps(&self) -> Vec<Step> {
        Step::ALL
            .into_iter()
            .filter(|s| *s != Step::Permissions || self.hosts())
            .collect()
    }

    pub fn next(&self) -> Step {
        let steps = self.steps();
        let i = steps.iter().position(|s| *s == self.step).unwrap_or(0);
        steps[(i + 1).min(steps.len() - 1)]
    }

    pub fn back(&self) -> Step {
        let steps = self.steps();
        let i = steps.iter().position(|s| *s == self.step).unwrap_or(0);
        steps[i.saturating_sub(1)]
    }
}

impl HelioApp {
    /// Go to setup's `step`, saved as reached (at Done, setup is over only
    /// when Done is pressed).
    pub fn setup_go(&mut self, step: Step, cx: &mut Context<Self>) {
        tracing::info!(step = step.name(), "setup");
        self.setup.step = step;
        self.prefs.setup_step = Some(step.name().to_string());
        self.page_scroll.set_offset(gpui::point(px(0.0), px(0.0)));
        self.save_prefs(cx);
    }

    /// Setup again, from Settings → General: from what this computer is for.
    pub fn setup_again(&mut self, cx: &mut Context<Self>) {
        self.setup.showing = true;
        self.set_page(Page::Setup, cx);
        self.setup_go(Step::Use, cx);
    }

    /// Setup is over (done, or skipped with the defaults): the Hosts page,
    /// or This computer when it was set up to host.
    pub fn finish_setup(&mut self, cx: &mut Context<Self>) {
        tracing::info!(use_for = ?self.setup.use_for, "setup over");
        self.setup.showing = false;
        self.setup.step = Step::Welcome;
        self.prefs.onboarded = true;
        self.prefs.setup_step = None;
        self.save_prefs(cx);
        let page = match self.setup.use_for {
            Some(Use::Host) => Page::Host(crate::host::Page::Overview),
            _ => Page::Hosts,
        };
        self.set_page(page, cx);
    }

    fn choose_use(&mut self, use_for: Use, cx: &mut Context<Self>) {
        self.setup.use_for = Some(use_for);
        self.prefs.use_for = Some(use_for.name().to_string());
        self.save_prefs(cx);
    }

    /// Continue: on to the next step; from the last, setup is over.
    fn setup_continue(&mut self, cx: &mut Context<Self>) {
        match self.setup.step {
            Step::Done => self.finish_setup(cx),
            _ => {
                let next = self.setup.next();
                self.setup_go(next, cx);
            }
        }
    }

    /// Setup, the whole window: its steps along the top, the step, and Back,
    /// Skip and Continue at the bottom (the welcome has its own way in).
    pub fn render_setup(
        &mut self,
        t: Theme,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let step = self.setup.step;
        if step == Step::Welcome {
            return self.setup_welcome(t, cx);
        }
        let body = match step {
            Step::Use => self.setup_use(t, cx),
            Step::Permissions => self.setup_permissions(t, window, cx),
            Step::Pairing => self.setup_pairing(t, cx),
            Step::Welcome | Step::Done => self.setup_done(t, cx),
        };
        let steps = self.setup.steps();
        let index = |s: Step| steps.iter().position(|x| *x == s).unwrap_or(0);
        // The steps between the welcome and done: those behind checked, this
        // one lit. A click on another goes to it.
        let between = &steps[1..steps.len() - 1];
        let progress = div()
            .flex()
            .items_center()
            .gap(px(20.0))
            .children(between.iter().map(|&s| {
                let (behind, on) = (index(s) < index(step), s == step);
                let mark = if behind {
                    icon(IconName::Check, 12.0, t.fresh).into_any_element()
                } else {
                    div()
                        .size(px(7.0))
                        .rounded_full()
                        .bg(if on { t.accent } else { t.quaternary })
                        .into_any_element()
                };
                div()
                    .id(SharedString::from(format!("setup-step-{}", s.name())))
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .text_size(px(12.0))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(if on { t.primary } else { t.tertiary })
                    .when(!on, |d| {
                        d.cursor_pointer()
                            .on_click(cx.listener(move |this, _, _, cx| this.setup_go(s, cx)))
                    })
                    .child(div().w(px(12.0)).flex().justify_center().child(mark))
                    .child(s.title())
            }));
        let back = self.setup.back();
        let footer = div()
            .flex_none()
            .h(px(60.0))
            .px(px(24.0))
            .flex()
            .items_center()
            .justify_between()
            .border_t_1()
            .border_color(t.hairline)
            .child(
                button("setup-back", "Back", t)
                    .on_click(cx.listener(move |this, _, _, cx| this.setup_go(back, cx))),
            )
            .child(match step {
                Step::Done => div()
                    .child(
                        button("setup-finish", "Done", t)
                            .accent()
                            .on_click(cx.listener(|this, _, _, cx| this.finish_setup(cx))),
                    )
                    .into_any_element(),
                _ => div()
                    .flex()
                    .gap(px(8.0))
                    .child(
                        button("setup-skip", "Skip Setup", t)
                            .kind(ButtonKind::Ghost)
                            .on_click(cx.listener(|this, _, _, cx| this.finish_setup(cx))),
                    )
                    .child(
                        button("setup-next", "Continue", t)
                            .accent()
                            .on_click(cx.listener(|this, _, _, cx| this.setup_continue(cx))),
                    )
                    .into_any_element(),
            });
        let width = (f32::from(window.viewport_size().width) - 56.0).clamp(320.0, WIDTH);
        div()
            .size_full()
            .flex()
            .flex_col()
            .child(
                // The steps between two halves that move the window.
                div()
                    .flex_none()
                    .h(px(Metrics::TITLE_BAR))
                    .flex()
                    .items_center()
                    .child(helio_ui::drag_area().flex_1().h_full())
                    .child(progress)
                    .child(helio_ui::drag_area().flex_1().h_full()),
            )
            .child(helio_ui::scrolled(
                div()
                    .id("setup")
                    .size_full()
                    .overflow_y_scroll()
                    .track_scroll(&self.page_scroll)
                    .child(
                        div()
                            .w_full()
                            .flex()
                            .justify_center()
                            .px(px(28.0))
                            .pt(px(28.0))
                            .pb(px(32.0))
                            .child(div().w_full().min_w_0().max_w(px(width)).child(body)),
                    ),
                "setup-bar",
                &self.page_scroll,
                t,
            ))
            .child(footer)
            .into_any_element()
    }

    /// What the app does, and the way in: set up, or the defaults.
    fn setup_welcome(&mut self, t: Theme, cx: &mut Context<Self>) -> AnyElement {
        let line = |text: &'static str| {
            div()
                .text_size(px(14.0))
                .line_height(px(21.0))
                .text_color(t.secondary)
                .text_center()
                .child(text)
        };
        div()
            .size_full()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(12.0))
            .px(px(40.0))
            .child(crate::app::brand_mark(56.0, t.primary))
            .child(
                div()
                    .pt(px(16.0))
                    .text_size(px(28.0))
                    .font_weight(FontWeight::BOLD)
                    .child("Welcome to Helio"),
            )
            .child(div().max_w(px(470.0)).flex().flex_col().gap(px(8.0)).children([
                line("Play your other computers from this one, with their games, apps and sound, as if you sat at them."),
                line("Or let your devices play this one: hosting is off until you turn it on."),
            ]))
            .child(
                div()
                    .pt(px(16.0))
                    .flex()
                    .gap(px(10.0))
                    .child(
                        button("setup-start", "Set Up Helio", t)
                            .accent()
                            .on_click(cx.listener(|this, _, _, cx| this.setup_go(Step::Use, cx))),
                    )
                    .child(
                        button("setup-defaults", "Skip, Use the Defaults", t)
                            .on_click(cx.listener(|this, _, _, cx| this.finish_setup(cx))),
                    ),
            )
            .child(
                div().pt(px(6.0)).max_w(px(420.0)).child(
                    footnote(
                        "A minute: what this computer is for, what it must allow to be played, \
                         and how to pair. Skipped, this computer plays others only, and Helio asks \
                         for each permission when it first needs it; everything is in Settings \
                         later.",
                        t,
                    )
                    .text_center(),
                ),
            )
            .into_any_element()
    }

    /// What this computer is for: three answers.
    fn setup_use(&mut self, t: Theme, cx: &mut Context<Self>) -> AnyElement {
        let chosen = self.setup.use_for.unwrap_or(Use::Play);
        let device = crate::app::device_word();
        let answers = [
            (
                Use::Play,
                IconName::Devices,
                "Play my other computers from here".to_string(),
                format!("This {device} streams from your others. Nothing on it is shared."),
            ),
            (
                Use::Host,
                IconName::Display,
                "Let my devices play this one".to_string(),
                format!("Hosting: your other devices stream this {device}'s screen, with its keyboard, mouse and sound."),
            ),
            (
                Use::Both,
                IconName::Network,
                "Both".to_string(),
                format!("Play your others from here, and let them play this {device}."),
            ),
        ];
        let choices: Vec<AnyElement> = answers
            .into_iter()
            .map(|(u, glyph, label, detail)| {
                choice_row(
                    SharedString::from(format!("setup-use-{}", u.name())),
                    t,
                    glyph,
                    label,
                    detail,
                    u == chosen,
                )
                .on_click(cx.listener(move |this, _, _, cx| this.choose_use(u, cx)))
                .into_any_element()
            })
            .collect();
        div()
            .flex()
            .flex_col()
            .gap(px(20.0))
            .child(page_header(
                "What do you want to do on this computer?",
                Some("You can change it any time in Settings → Hosting.".into()),
                None,
                t,
            ))
            .child(rows(choices, t))
            .into_any_element()
    }

    /// For hosting: what the system must allow, each said before it is
    /// asked, and hosting turned on.
    fn setup_permissions(
        &mut self,
        t: Theme,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let _ = window;
        let switch = self.hosting_switch_row(t, cx);
        let needs = self.hosting_needs_rows(t, cx);
        let (subtitle, foot) = if cfg!(target_os = "macos") {
            (
                "macOS asks for each of these once, for the host (Helio Host), when hosting starts.",
                "Allowed later in System Settings → Privacy & Security, macOS asks to quit and \
                 reopen the host: Helio notices when it is back.",
            )
        } else if cfg!(windows) {
            (
                "Hosting runs as a service, so Windows asks for an administrator once, to turn \
                 it on.",
                "The host uses NVIDIA's encoder and SudoVDA's virtual display when they are \
                 here; without them it still streams.",
            )
        } else {
            (
                "The host runs as your user, so turning it on asks for nothing.",
                "On Wayland, your desktop asks you to share the screen when a device first \
                 connects.",
            )
        };
        div()
            .flex()
            .flex_col()
            .gap(px(20.0))
            .child(page_header(
                "What this computer must allow",
                Some(subtitle.into()),
                None,
                t,
            ))
            .child(section("Hosting", t).child(rows([switch], t)))
            .when(!needs.is_empty(), |d| {
                d.child(section("Allowed", t).child(rows(needs, t)))
            })
            .child(footnote(foot, t))
            .into_any_element()
    }

    /// Pairing: hosting, the devices asking to pair as they come, each with
    /// the field for the PIN it shows; else how to pair from here, in a
    /// line.
    fn setup_pairing(&mut self, t: Theme, cx: &mut Context<Self>) -> AnyElement {
        let device = crate::app::device_word();
        let line = |text: String| {
            rows(
                [div()
                    .min_h(px(Metrics::SETTINGS_ROW))
                    .px(px(12.0))
                    .py(px(10.0))
                    .flex()
                    .items_center()
                    .gap(px(12.0))
                    .child(helio_ui::tile(IconName::Link, t))
                    .child(div().flex_1().min_w_0().line_height(px(19.0)).child(text))
                    .into_any_element()],
                t,
            )
            .into_any_element()
        };
        let (subtitle, body): (&str, Vec<AnyElement>) = match self.setup.use_for {
            Some(Use::Host | Use::Both) => (
                "Pair once with each device that will play this one.",
                match self.host.clone() {
                    Some(host) => host.update(cx, |h, cx| h.pairing(t, cx)),
                    None => vec![line(
                        "Hosting is off, so no device can ask to pair yet. Turn it on in \
                         Permissions (Back), or later in Settings → Hosting; then pair from \
                         This computer → Devices."
                            .to_string(),
                    )],
                },
            ),
            _ => (
                "Pair once with each computer you want to play.",
                vec![line(format!(
                    "On the other computer, turn hosting on in Helio. Then pick it in Hosts on \
                     this {device}: it shows a PIN, which you type into Helio on the other one."
                ))],
            ),
        };
        div()
            .flex()
            .flex_col()
            .gap(px(20.0))
            .child(page_header("Pairing", Some(subtitle.into()), None, t))
            .children(body)
            .child(footnote(
                "Pairing uses a PIN, never a password: nothing else is copied between them.",
                t,
            ))
            .into_any_element()
    }

    /// What is ready, and what is left (asked for when first needed). A
    /// click on one goes to its step.
    fn setup_done(&mut self, t: Theme, cx: &mut Context<Self>) -> AnyElement {
        let hosts = self.setup.hosts();
        let state = self.hosting.read(cx).state.clone();
        let item = |id: &'static str,
                    step: Step,
                    ready: bool,
                    label: String,
                    detail: String,
                    cx: &mut Context<Self>| {
            div()
                .id(id)
                .min_h(px(44.0))
                .px(px(12.0))
                .py(px(8.0))
                .flex()
                .items_center()
                .gap(px(12.0))
                .cursor_pointer()
                .hover(|s| s.bg(t.hover))
                .on_click(cx.listener(move |this, _, _, cx| this.setup_go(step, cx)))
                .child(div().w(px(28.0)).flex().justify_center().child(icon(
                    if ready {
                        IconName::Check
                    } else {
                        IconName::Clock
                    },
                    16.0,
                    if ready { t.fresh } else { t.tertiary },
                )))
                .child(helio_ui::label_stack(label, Some(detail.into()), t))
                .into_any_element()
        };
        let device = crate::app::device_word();
        let mut list = vec![item(
            "setup-done-use",
            Step::Use,
            true,
            match self.setup.use_for.unwrap_or(Use::Play) {
                Use::Play => "Plays your other computers".into(),
                Use::Host => format!("Lets your devices play this {device}"),
                Use::Both => format!("Plays your others, and lets them play this {device}"),
            },
            "Settings → Hosting changes it.".into(),
            cx,
        )];
        if hosts {
            let on = state.enabled();
            list.push(item(
                "setup-done-hosting",
                Step::Permissions,
                on,
                if on {
                    "Hosting is on".into()
                } else {
                    "Hosting is off".into()
                },
                if on {
                    "The host runs whenever you are signed in.".into()
                } else {
                    "Turn it on in Permissions, or later in Settings → Hosting.".into()
                },
                cx,
            ));
        }
        // Paired: hosting, the devices that may play this one; playing
        // only, the computers this one may play.
        let paired = if hosts {
            self.host
                .as_ref()
                .map_or(0, |h| h.read(cx).snapshot().clients.len())
        } else {
            self.model.items.iter().filter(|i| i.is_paired()).count()
        };
        let (label, detail) = match (hosts, paired) {
            (true, 0) => (
                "Pair your devices".into(),
                "Their PINs go in This computer → Devices.".into(),
            ),
            (true, n) => (
                format!("{n} {} paired", if n == 1 { "device" } else { "devices" }),
                "This computer → Devices has them, and pairs more.".into(),
            ),
            (false, 0) => (
                "Pair your computers".into(),
                "Pick a computer in Hosts to pair with it.".into(),
            ),
            (false, n) => (
                format!(
                    "Paired with {n} {}",
                    if n == 1 { "computer" } else { "computers" }
                ),
                "Hosts lists them: a click plays one.".into(),
            ),
        };
        list.push(item(
            "setup-done-pairing",
            Step::Pairing,
            paired > 0,
            label,
            detail,
            cx,
        ));
        div()
            .flex()
            .flex_col()
            .gap(px(20.0))
            .child(page_header(
                "You're set",
                Some(if hosts {
                    format!("This {device} is ready to be played, and Helio is ready to play.")
                        .into()
                } else {
                    "Pick a computer in Hosts to play it.".into()
                }),
                None,
                t,
            ))
            .child(rows(list, t))
            .child(footnote(
                "Everything here is in Settings; General → Set Up Again goes through it again.",
                t,
            ))
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup(step: Step, use_for: Option<Use>) -> Setup {
        Setup {
            step,
            use_for,
            showing: true,
        }
    }

    #[test]
    fn playing_only_goes_past_the_permissions() {
        let s = setup(Step::Use, Some(Use::Play));
        assert_eq!(s.next(), Step::Pairing);
        assert_eq!(setup(Step::Pairing, None).back(), Step::Use);
        let s = setup(Step::Use, Some(Use::Both));
        assert_eq!(s.next(), Step::Permissions);
        assert_eq!(
            setup(Step::Pairing, Some(Use::Host)).back(),
            Step::Permissions
        );
        assert_eq!(setup(Step::Done, None).next(), Step::Done);
        assert_eq!(setup(Step::Welcome, None).back(), Step::Welcome);
    }

    #[test]
    fn setup_goes_on_where_it_was_left_until_it_is_over() {
        let fresh = Setup::resume(&Prefs::default());
        assert!(fresh.showing() && fresh.step == Step::Welcome && fresh.use_for.is_none());
        let left = Prefs {
            setup_step: Some("permissions".into()),
            use_for: Some("both".into()),
            ..Prefs::default()
        };
        let resumed = Setup::resume(&left);
        assert_eq!(
            (resumed.step, resumed.use_for),
            (Step::Permissions, Some(Use::Both))
        );
        let over = Prefs {
            onboarded: true,
            ..Prefs::default()
        };
        assert!(!Setup::resume(&over).showing());
        assert_eq!(Step::named("pairing"), Some(Step::Pairing));
        assert_eq!(Use::named("host").map(Use::hosts), Some(true));
    }
}
