//! This computer: the host on it, as Helio Host's window showed it, now pages of
//! the one window (This computer in the sidebar): the host at a glance (the
//! session, with its numbers; an agent's, with its controls), the devices
//! (pairing requests first), its log; and the host's settings, in Settings
//! → Hosting under the switch (each setting in one place). What the web UI
//! does, native, for this computer.
//!
//! The pages are a client of the host's web API on localhost, with the
//! certificate the host made pinned (`api`), through one worker thread
//! (`worker`) whose word the app keeps (`link`) while hosting is on
//! (`switch`); the tray icon reads the same (`tray`).

pub mod api;
mod demo;
mod devices;
pub mod link;
mod logs;
mod overview;
mod permissions;
mod settings;
mod signin;
pub mod switch;
pub mod tray;
pub mod worker;

use std::collections::HashMap;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crossbeam_channel::Sender;
use gpui::{
    div, prelude::*, px, AnyElement, Context, Entity, FontWeight, ScrollHandle, SharedString,
    Window,
};
use helio_ui::{button, sheet, FieldEvent, IconName, Ink, Metrics, TextField, Theme, Type};
use serde_json::{json, Value};

use api::Client;
use link::Link;
use switch::Hosting;
use worker::{Cmd, Conn, Done, Snapshot};

pub use demo::pretend;

/// This computer's pages, under it in the sidebar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Overview,
    Devices,
    Logs,
}

impl Page {
    pub fn parse(s: &str) -> Option<Page> {
        Some(match s {
            "overview" => Page::Overview,
            "devices" => Page::Devices,
            "logs" => Page::Logs,
            _ => return None,
        })
    }
}

enum Confirm {
    Unpair(Client),
    EndSession(String),
    StopAgent,
    SignOut,
}

pub struct HostPages {
    page: Page,
    snap: Snapshot,
    cmds: Sender<Cmd>,
    /// Hosting on or off: the not-running page turns it on again.
    hosting: Entity<Hosting>,
    /// A config just changed here, ahead of the host's word on it.
    config_ahead: Option<(Value, Instant)>,
    pins: HashMap<u32, Entity<TextField>>,
    pin_errors: HashMap<u32, String>,
    /// What a device asking to pair may do, where it was chosen.
    pair_choice: HashMap<u32, helio_proto::permission::Permissions>,
    /// The device whose permissions sheet is open (its key).
    editing: Option<String>,
    /// A permissions change made here, ahead of the host's word on it.
    permissions_ahead: Option<(String, helio_proto::permission::Permissions, Instant)>,
    note: Option<(IconName, gpui::Rgba, String, Instant)>,
    /// A change made here restarts the host: the start time of the host
    /// that is to make way, until another answers.
    restarting: Option<u64>,
    confirm: Option<Confirm>,
    user: Entity<TextField>,
    password: Entity<TextField>,
    sign_in_error: Option<String>,
    signing_in: bool,
    name: Entity<TextField>,
    ports: [Entity<TextField>; 3],
    logs_scroll: ScrollHandle,
    logs_len: usize,
    /// Where the page shown is scrolled to (its bar).
    pub page_scroll: ScrollHandle,
}

const PORT_KEYS: [&str; 3] = ["port", "pairing_port", "web_port"];

impl HostPages {
    pub fn new(
        link: Entity<Link>,
        hosting: Entity<Hosting>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> HostPages {
        // What the host says, and what it answers to what is asked of it.
        cx.observe_in(&link, window, |this, link, window, cx| {
            let snap = link.read(cx).snap.clone();
            this.take_snapshot(snap, window, cx);
            cx.notify();
        })
        .detach();
        cx.subscribe(&link, |this, _, done: &Done, cx| {
            this.done(done.clone());
            cx.notify();
        })
        .detach();
        let (cmds, first) = {
            let link = link.read(cx);
            (link.sender(), link.snap.clone())
        };
        // A window shows the host: the worker asks it every second.
        let _ = cmds.send(Cmd::Watched(true));
        cx.spawn_in(window, async move |this, cx| loop {
            cx.background_executor()
                .timer(Duration::from_millis(300))
                .await;
            if this.update_in(cx, |this, _, cx| this.tick(cx)).is_err() {
                break;
            }
        })
        .detach();

        let user = cx.new(|cx| TextField::new(cx).placeholder("User name"));
        let password = cx.new(|cx| TextField::new(cx).password().placeholder("Password"));
        for f in [&user, &password] {
            cx.subscribe_in(f, window, |this, _, e, _, cx| match e {
                FieldEvent::Submit => this.sign_in(cx),
                _ => cx.notify(),
            })
            .detach();
        }
        let name = cx.new(|cx| TextField::new(cx).placeholder("This computer's name"));
        cx.subscribe_in(&name, window, |this, f, e, _, cx| {
            if *e == FieldEvent::Submit {
                let v = f.read(cx).text().trim().to_string();
                if !v.is_empty() {
                    this.set_config("name", json!(v), cx);
                }
            }
        })
        .detach();
        let ports = [(); 3].map(|_| cx.new(|cx| TextField::new(cx).placeholder("port")));
        for (i, f) in ports.iter().enumerate() {
            cx.subscribe_in(f, window, move |this, f, e, _, cx| {
                if *e == FieldEvent::Submit {
                    if let Ok(port) = f.read(cx).text().trim().parse::<u16>() {
                        this.set_config(PORT_KEYS[i], json!(port), cx);
                    }
                }
            })
            .detach();
        }

        let mut app = HostPages {
            page: Page::Overview,
            snap: Snapshot::default(),
            cmds,
            hosting,
            config_ahead: None,
            pins: HashMap::new(),
            pin_errors: HashMap::new(),
            pair_choice: HashMap::new(),
            editing: None,
            permissions_ahead: None,
            note: None,
            restarting: None,
            confirm: None,
            user,
            password,
            sign_in_error: None,
            signing_in: false,
            name,
            ports,
            logs_scroll: ScrollHandle::new(),
            logs_len: 0,
            page_scroll: ScrollHandle::new(),
        };
        // A window opened over a host the app already knows shows it at
        // once, not at the worker's next word.
        app.take_snapshot(first, window, cx);
        app
    }

    fn send(&self, cmd: Cmd) {
        let _ = self.cmds.send(cmd);
    }

    /// Pair request `id` with `pin`, and what was chosen for it.
    fn submit_pin(&mut self, id: u32, pin: String) {
        self.pin_errors.remove(&id);
        let chosen = self.pair_choice.get(&id).copied();
        self.send(Cmd::SubmitPin(id, pin, chosen));
    }

    fn set_page(&mut self, page: Page, cx: &mut Context<Self>) {
        if (page == Page::Logs) != (self.page == Page::Logs) {
            self.send(Cmd::WantLogs(page == Page::Logs));
        }
        if page != self.page {
            self.page_scroll.set_offset(gpui::point(px(0.0), px(0.0)));
        }
        self.page = page;
        cx.notify();
    }

    /// Show `page` (from the sidebar, the tray icon's menu, a
    /// notification, the menu bar).
    pub fn go(&mut self, page: Page, cx: &mut Context<Self>) {
        self.confirm = None;
        self.editing = None;
        self.set_page(page, cx);
    }

    /// The host as last heard from.
    pub fn snapshot(&self) -> &Snapshot {
        &self.snap
    }

    /// Escape: the sheet open closes; whether there was one.
    pub fn dismiss(&mut self, cx: &mut Context<Self>) -> bool {
        let open = self.confirm.take().is_some() | self.editing.take().is_some();
        cx.notify();
        open
    }

    /// Sign out of the host here (asked first).
    pub fn ask_sign_out(&mut self, cx: &mut Context<Self>) {
        self.confirm = Some(Confirm::SignOut);
        cx.notify();
    }

    /// Ask the host how things are now (the window came forward: from
    /// System Settings, say). Screen Recording missing, the host is asked
    /// to look whether it was allowed meanwhile, which only a process it
    /// starts can tell it (`helio-host/src/mac/permissions.rs`): allowed,
    /// it restarts.
    pub fn refresh(&self) {
        self.send(Cmd::Refresh);
        let screen = self
            .snap
            .status
            .as_ref()
            .and_then(|s| s.grants)
            .map(|g| g.screen);
        if screen == Some(false) {
            self.send(Cmd::CheckGrants);
        }
    }

    fn take_snapshot(&mut self, mut s: Snapshot, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some((config, at)) = &self.config_ahead {
            if at.elapsed() < Duration::from_secs(3) {
                s.config = Some(config.clone());
            } else {
                self.config_ahead = None;
            }
        }
        let first_config = self.snap.config.is_none() && s.config.is_some();
        // A PIN field for each request; gone requests take theirs along.
        let pending: Vec<u32> = s
            .status
            .as_ref()
            .map(|st| st.pending.iter().map(|p| p.id).collect())
            .unwrap_or_default();
        self.pins.retain(|id, _| pending.contains(id));
        self.pair_choice.retain(|id, _| pending.contains(id));
        self.keep_permissions_ahead(&mut s.clients);
        for id in pending {
            if let std::collections::hash_map::Entry::Vacant(slot) = self.pins.entry(id) {
                let f = cx.new(|cx| TextField::new(cx).placeholder("PIN"));
                cx.subscribe(&f, move |this, f, e, cx| match e {
                    FieldEvent::Submit => {
                        let pin = f.read(cx).text().trim().to_string();
                        if !pin.is_empty() {
                            this.submit_pin(id, pin);
                        }
                    }
                    _ => cx.notify(),
                })
                .detach();
                slot.insert(f);
            }
        }
        if let Some(logs) = &s.logs {
            let n = logs.lines().count();
            if n != self.logs_len {
                self.logs_len = n;
                self.logs_scroll.scroll_to_bottom();
            }
        }
        let back = restarted(self.restarting, &s);
        self.snap = s;
        if back {
            self.restarting = None;
            self.note = Some((
                IconName::CheckCircle,
                Ink::FRESH,
                "Helio Host restarted: the change applies.".into(),
                Instant::now(),
            ));
        }
        if first_config {
            self.fill_fields(cx);
        }
    }

    /// The fields that edit the config show what it says.
    fn fill_fields(&mut self, cx: &mut Context<Self>) {
        let Some(c) = self.snap.config.clone() else {
            return;
        };
        let name = c
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        self.name.update(cx, |f, cx| f.set_text(name, cx));
        for (i, key) in PORT_KEYS.iter().enumerate() {
            let v = c
                .get(*key)
                .and_then(Value::as_u64)
                .map(|p| p.to_string())
                .unwrap_or_default();
            self.ports[i].update(cx, |f, cx| f.set_text(v, cx));
        }
    }

    fn done(&mut self, d: Done) {
        match d {
            Done::Paired(name) => {
                self.note = Some((
                    IconName::CheckCircle,
                    Ink::FRESH,
                    format!("Paired with {name}. It can connect now."),
                    Instant::now(),
                ))
            }
            Done::PinFailed(id, e) => {
                self.pin_errors.insert(id, e);
            }
            Done::Saved { restart } => {
                if restart {
                    self.restarting = Some(self.snap.status.as_ref().map_or(0, |s| s.started_unix));
                }
            }
            Done::Failed(e) => {
                self.note = Some((IconName::Warning, Ink::DANGER, e, Instant::now()))
            }
            Done::SignedIn => {
                self.signing_in = false;
                self.sign_in_error = None;
            }
            Done::SignInFailed(e) => {
                self.signing_in = false;
                self.sign_in_error = Some(e);
            }
        }
    }

    fn tick(&mut self, cx: &mut Context<Self>) {
        if self
            .note
            .as_ref()
            .is_some_and(|n| n.3.elapsed() > Duration::from_secs(8))
        {
            self.note = None;
            cx.notify();
        }
        if self
            .snap
            .status
            .as_ref()
            .is_some_and(|s| s.session.is_some())
        {
            cx.notify();
        }
    }

    fn config(&self) -> Option<&Value> {
        self.snap.config.as_ref()
    }

    fn cfg_bool(&self, key: &str) -> bool {
        self.config()
            .and_then(|c| c.get(key))
            .and_then(Value::as_bool)
            .unwrap_or(false)
    }

    fn cfg_u64(&self, key: &str) -> u64 {
        self.config()
            .and_then(|c| c.get(key))
            .and_then(Value::as_u64)
            .unwrap_or(0)
    }

    fn set_config(&mut self, key: &str, value: Value, cx: &mut Context<Self>) {
        let Some(mut c) = self.snap.config.clone() else {
            return;
        };
        if c.get(key) == Some(&value) {
            return;
        }
        c[key] = value;
        self.snap.config = Some(c.clone());
        self.config_ahead = Some((c.clone(), Instant::now()));
        self.send(Cmd::PutConfig(c));
        cx.notify();
    }

    fn sign_in(&mut self, cx: &mut Context<Self>) {
        let user = self.user.read(cx).text().trim().to_string();
        let password = self.password.read(cx).text().to_string();
        if user.is_empty() || password.is_empty() || self.signing_in {
            return;
        }
        let setup = matches!(self.snap.conn, Conn::SignIn { setup: true });
        if setup && password.len() < 8 {
            self.sign_in_error = Some("Choose a password of at least 8 characters.".into());
            cx.notify();
            return;
        }
        self.signing_in = true;
        self.sign_in_error = None;
        self.password.update(cx, |f, cx| f.set_text("", cx));
        self.send(Cmd::SignIn {
            user,
            password,
            setup,
        });
        cx.notify();
    }

    fn os(&self) -> &str {
        if self.snap.state.os.is_empty() {
            std::env::consts::OS
        } else {
            &self.snap.state.os
        }
    }

    // -----------------------------------------------------------------------
    // Drawing

    fn body(&mut self, t: Theme, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        match self.snap.conn.clone() {
            Conn::Connecting => signin::waiting("connecting-page", "Connecting to Helio Host…", t),
            Conn::NotRunning => self.not_running(t, cx),
            Conn::SignIn { setup } => self.sign_in_page(setup, t, window, cx),
            Conn::Ready => match self.page {
                Page::Overview => self.overview(t, cx),
                Page::Devices => self.devices(t, cx),
                Page::Logs => self.logs(t, cx),
            },
        }
    }

    fn confirm_sheet(&mut self, t: Theme, cx: &mut Context<Self>) -> Option<AnyElement> {
        let c = self.confirm.as_ref()?;
        let (title, text, action): (String, String, &str) = match c {
            Confirm::Unpair(c) => (
                format!("Unpair {}?", c.name),
                "It will need to pair again to connect.".into(),
                "Unpair",
            ),
            Confirm::EndSession(who) => (
                format!("End {who}'s session?"),
                "The stream closes on the client.".into(),
                "End Session",
            ),
            Confirm::StopAgent => (
                "Stop the agent?".into(),
                "Its session ends.".into(),
                "Stop Agent",
            ),
            Confirm::SignOut => (
                "Sign out?".into(),
                "This window forgets its token; the host stops taking it.".into(),
                "Sign Out",
            ),
        };
        let body = div()
            .flex()
            .flex_col()
            .gap(px(10.0))
            .child(
                div()
                    .text_size(px(17.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(title),
            )
            .child(
                div()
                    .text_size(px(Type::BODY))
                    .line_height(px(18.0))
                    .text_color(t.secondary)
                    .child(text),
            )
            .child(
                div()
                    .pt(px(8.0))
                    .flex()
                    .justify_end()
                    .gap(px(8.0))
                    .child(button("confirm-cancel", "Cancel", t).on_click(cx.listener(
                        |this, _, _, cx| {
                            this.confirm = None;
                            cx.notify();
                        },
                    )))
                    .child(
                        button("confirm-yes", action, t)
                            .danger()
                            .on_click(cx.listener(|this, _, _, cx| {
                                match this.confirm.take() {
                                    Some(Confirm::Unpair(c)) => this.send(Cmd::Unpair(c.key)),
                                    Some(Confirm::EndSession(_)) => this.send(Cmd::EndSession),
                                    Some(Confirm::StopAgent) => this.send(Cmd::AgentOp("stop")),
                                    Some(Confirm::SignOut) => this.send(Cmd::SignOut),
                                    None => {}
                                }
                                cx.notify();
                            })),
                    ),
            );
        Some(sheet("confirm", 360.0, t, body).into_any_element())
    }
}

/// The window gone, only the tray icon looks: the worker asks less often.
impl Drop for HostPages {
    fn drop(&mut self) {
        self.send(Cmd::Watched(false));
    }
}

impl HostPages {
    /// The page shown, for the window's page column.
    pub fn render_page(
        &mut self,
        t: Theme,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.body(t, window, cx)
    }

    /// The sheet open over the window (a confirmation, a device's
    /// permissions), if one is.
    pub fn render_sheet(
        &mut self,
        t: Theme,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let window_h = f32::from(window.viewport_size().height);
        self.confirm_sheet(t, cx)
            .or_else(|| self.permissions_sheet(window_h, t, cx))
    }

    /// The pages' scroll areas, for what the keyboard moves to to be
    /// scrolled into view.
    pub fn scroll_handles(&self) -> [ScrollHandle; 2] {
        [self.page_scroll.clone(), self.logs_scroll.clone()]
    }
}

// ---------------------------------------------------------------------------

/// A page: the strip along the title bar, then its content in a column,
/// scrolling with its bar (asides' pages).
fn page(
    scroll: &ScrollHandle,
    t: Theme,
    id: &'static str,
    content: impl IntoElement,
) -> AnyElement {
    helio_ui::page(id, scroll, Metrics::PAGE, t, content)
}

fn info_row(label: &'static str, value: String, mono: bool, t: Theme) -> AnyElement {
    div()
        .min_h(px(38.0))
        .px(px(12.0))
        .flex()
        .items_center()
        .justify_between()
        .gap(px(16.0))
        .child(
            div()
                .flex_none()
                .text_size(px(Type::BODY))
                .text_color(t.secondary)
                .child(label),
        )
        .child(
            div()
                .min_w_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .text_size(px(if mono { 12.0 } else { Type::BODY }))
                .when(mono, |d| d.font_family(helio_ui::mono_font()))
                .child(value),
        )
        .into_any_element()
}

fn device_word(os: &str) -> &'static str {
    if os == "macos" {
        "Mac"
    } else {
        "PC"
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn duration(secs: u64) -> String {
    if secs < 60 {
        format!("{secs} s")
    } else if secs < 3600 {
        format!("{} min", secs / 60)
    } else {
        format!("{} h {} min", secs / 3600, secs % 3600 / 60)
    }
}

fn ago(secs: u64) -> SharedString {
    if secs < 5 {
        "now".into()
    } else if secs < 60 {
        format!("{secs}s").into()
    } else if secs < 3600 {
        format!("{}m", secs / 60).into()
    } else {
        format!("{}h", secs / 3600).into()
    }
}

/// "Sep 28" (or "Sep 28, 2025" in another year), from Unix seconds (UTC).
fn date(unix: u64) -> String {
    if unix == 0 {
        return "some time ago".into();
    }
    let civil = |days: i64| {
        // Howard Hinnant's days-from-civil, inverted.
        let z = days + 719_468;
        let era = z.div_euclid(146_097);
        let doe = z.rem_euclid(146_097);
        let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let d = doy - (153 * mp + 2) / 5 + 1;
        let m = if mp < 10 { mp + 3 } else { mp - 9 };
        (yoe + era * 400 + i64::from(m <= 2), m, d)
    };
    let (y, m, d) = civil(unix as i64 / 86_400);
    let (now_y, _, _) = civil(now_ms() as i64 / 86_400_000);
    let month = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ][(m - 1) as usize];
    if y == now_y {
        format!("{month} {d}")
    } else {
        format!("{month} {d}, {y}")
    }
}

/// Whether the host that was to make way for a restart (the one that
/// started at `from`) has: another one answers.
fn restarted(from: Option<u64>, now: &Snapshot) -> bool {
    match (from, &now.conn, &now.status) {
        (Some(from), Conn::Ready, Some(s)) => s.started_unix != from,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::api::Status;

    fn ready(started_unix: u64) -> Snapshot {
        Snapshot {
            conn: Conn::Ready,
            status: Some(Status {
                started_unix,
                ..Status::default()
            }),
            ..Snapshot::default()
        }
    }

    #[test]
    fn the_host_has_restarted_once_another_one_answers() {
        assert!(!restarted(Some(100), &ready(100)));
        assert!(restarted(Some(100), &ready(160)));
    }

    #[test]
    fn a_host_that_does_not_answer_has_not_restarted_yet() {
        let gone = Snapshot {
            conn: Conn::NotRunning,
            ..Snapshot::default()
        };
        assert!(!restarted(Some(100), &gone));
    }

    #[test]
    fn with_no_restart_asked_nothing_has_restarted() {
        assert!(!restarted(None, &ready(160)));
    }
}
