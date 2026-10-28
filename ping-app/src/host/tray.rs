//! The icon in the menu bar (a Mac) or the taskbar's notification area
//! (Windows) while hosting is on: its menu says what the host is doing and
//! opens the window; a device asking to pair is announced with a
//! notification. With the icon there, closing the window leaves the app
//! running behind it; hosting off, the icon goes, and closing the window
//! quits the app as before. Where there is no tray (Linux), none of this
//! exists.
//!
//! Quitting from the menu quits this app, not the host: the host is a
//! service (or a LaunchAgent, or a user unit) of its own and keeps running.

use std::collections::HashSet;
use std::rc::Rc;

use gpui::{App, Entity, Global, SystemNotification};
use pingpong_ui::tray::{Tray, TrayEvent, TrayImage, TrayItem};
use pingpong_update::{Checker, Status as UpdateStatus};

use super::switch::{Hosting, State};
use super::worker::{Conn, Snapshot};
use crate::app::Show;

/// The app's name to the system: the status item's saved place, the tray
/// window's class, the notifications' sender.
pub const APP_ID: &str = "dev.pingpong.Ping";

/// The menu's items.
mod item {
    pub const OPEN: u32 = 1;
    pub const PAIR: u32 = 2;
    pub const UPDATES: u32 = 3;
    pub const QUIT: u32 = 4;
}

pub struct HostIcon {
    hosting: Entity<Hosting>,
    updates: Rc<Checker>,
    tray: Option<Tray>,
    /// The pairing requests a notification has gone out for.
    announced: HashSet<u32>,
}

impl Global for HostIcon {}

/// A notification's tag for a pairing request, and back.
fn pairing_tag(id: u32) -> String {
    format!("pong-pair-{id}")
}

/// The menu's first line, and what hovering over the icon says.
pub fn state_line(state: &State, snap: &Snapshot) -> String {
    match (state, &snap.conn) {
        (State::Failed(_), _) => "The host stopped".into(),
        (_, Conn::Connecting) | (_, Conn::NotRunning) => "The host is starting…".into(),
        (_, Conn::SignIn { .. }) => "Signed out of the host".into(),
        (_, Conn::Ready) => match snap.status.as_ref().and_then(|s| s.session.as_ref()) {
            Some(session) if session.agent.is_some() => {
                format!("An AI agent is at work ({})", session.client)
            }
            Some(session) => format!("Streaming to {}", session.client),
            None => "Hosting: ready for your devices".into(),
        },
    }
}

/// The icon's menu for the host's state and the update check's.
pub fn menu(state: &State, snap: &Snapshot, updates: &UpdateStatus) -> Vec<TrayItem> {
    let mut items = vec![
        TrayItem::Label(state_line(state, snap)),
        TrayItem::Separator,
        TrayItem::action(item::OPEN, "Open Ping"),
    ];
    let pending = snap
        .status
        .as_ref()
        .map(|s| s.pending.as_slice())
        .unwrap_or_default();
    match pending {
        [] => {}
        [one] => items.push(TrayItem::action(
            item::PAIR,
            format!("{} Wants to Pair…", one.client_name),
        )),
        many => items.push(TrayItem::action(
            item::PAIR,
            format!("{} Devices Want to Pair…", many.len()),
        )),
    }
    items.push(TrayItem::Separator);
    items.push(TrayItem::action(
        item::UPDATES,
        match &updates.update {
            Some(update) => format!("{}…", title_case(&update.headline("Ping"))),
            None => "Check for Updates…".into(),
        },
    ));
    items.push(TrayItem::Separator);
    // The app quits; the host stays.
    items.push(TrayItem::action(item::QUIT, "Quit Ping"));
    items
}

/// A sentence as a menu item writes it: "Ping 0.7.0 Is Available".
fn title_case(text: &str) -> String {
    text.split(' ')
        .map(|word| match word {
            // The small words menus leave alone.
            "on" | "to" | "of" | "a" | "the" => word.to_string(),
            _ => {
                let mut chars = word.chars();
                chars
                    .next()
                    .map(|c| c.to_uppercase().chain(chars).collect())
                    .unwrap_or_default()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

impl HostIcon {
    /// Whether the app outlives its window (there is an icon to come back
    /// by).
    pub fn shown(cx: &App) -> bool {
        cx.try_global::<HostIcon>()
            .is_some_and(|b| b.tray.is_some())
    }

    /// The icon's menu as it is now, a line each (checks).
    pub fn menu_lines(cx: &App) -> Vec<String> {
        let Some(this) = cx.try_global::<HostIcon>() else {
            return Vec::new();
        };
        if this.tray.is_none() {
            return Vec::new();
        }
        let hosting = this.hosting.read(cx);
        let snap = hosting
            .link
            .as_ref()
            .map(|l| l.read(cx).snap.clone())
            .unwrap_or_default();
        menu(&hosting.state, &snap, &this.updates.status())
            .into_iter()
            .map(|item| match item {
                TrayItem::Separator => "-".to_string(),
                TrayItem::Label(text) => format!("({text})"),
                TrayItem::Action { title, .. } => title,
            })
            .collect()
    }

    /// Hosting or the host changed, or the update check said something
    /// new: the icon comes or goes, its menu and the notifications follow.
    pub fn refresh(cx: &mut App) {
        let Some(this) = cx.try_global::<HostIcon>() else {
            return;
        };
        let hosting = this.hosting.read(cx);
        let state = hosting.state.clone();
        let snap = hosting
            .link
            .as_ref()
            .map(|l| l.read(cx).snap.clone())
            .unwrap_or_default();
        let updates = this.updates.status();
        let wanted = state.enabled() && pingpong_ui::tray::supported();
        let has = this.tray.is_some();
        if wanted && !has {
            let tray = Tray::new(
                APP_ID,
                "Ping",
                TrayImage {
                    template_png: include_bytes!("../../assets/tray.png"),
                    template_png_2x: include_bytes!("../../assets/tray@2x.png"),
                },
                cx,
                HostIcon::on_tray,
            );
            tracing::info!(shown = tray.is_some(), "the host's icon");
            cx.global_mut::<HostIcon>().tray = tray;
        } else if !wanted && has {
            tracing::info!("the host's icon goes: hosting is off");
            cx.global_mut::<HostIcon>().tray = None;
        }
        let window_in_front = crate::app::window_in_front(cx);
        let pending: Vec<(u32, String)> = snap
            .status
            .as_ref()
            .map(|s| {
                s.pending
                    .iter()
                    .map(|p| (p.id, p.client_name.clone()))
                    .collect()
            })
            .unwrap_or_default();
        // Requests that went (paired, declined, given up) take their
        // notifications along.
        let gone: Vec<u32> = cx
            .global::<HostIcon>()
            .announced
            .iter()
            .copied()
            .filter(|id| !pending.iter().any(|(p, _)| p == id))
            .collect();
        for id in gone {
            cx.dismiss_system_notification(&pairing_tag(id));
            cx.global_mut::<HostIcon>().announced.remove(&id);
        }
        for (id, name) in pending {
            if !cx.global_mut::<HostIcon>().announced.insert(id) {
                continue;
            }
            // The window in front shows the request itself.
            if window_in_front {
                continue;
            }
            tracing::info!(request = id, "announcing a pairing request");
            cx.show_system_notification(SystemNotification {
                tag: pairing_tag(id).into(),
                title: format!("{name} wants to pair").into(),
                body: "Open Ping and enter the PIN it shows to let it connect.".into(),
                actions: Vec::new(),
            });
        }
        let this = cx.global_mut::<HostIcon>();
        if let Some(tray) = &mut this.tray {
            tray.set_tooltip(&format!("Ping: {}", state_line(&state, &snap)));
            tray.set_menu(menu(&state, &snap, &updates));
        }
    }

    fn on_tray(event: TrayEvent, cx: &mut App) {
        tracing::debug!(?event, "tray");
        match event {
            TrayEvent::Activate | TrayEvent::Item(item::OPEN) => crate::app::open(Show::Window, cx),
            TrayEvent::Item(item::PAIR) => crate::app::open(Show::Devices, cx),
            TrayEvent::Item(item::UPDATES) => crate::app::open(Show::Updates, cx),
            TrayEvent::Item(item::QUIT) => cx.quit(),
            TrayEvent::Item(_) => {}
        }
    }
}

/// Start the icon's half of the app: it follows hosting (and the host,
/// through it) and the update check, and a click on a pairing request's
/// notification opens the window at it.
pub fn start(hosting: Entity<Hosting>, updates: Rc<Checker>, cx: &mut App) {
    cx.set_global(HostIcon {
        hosting: hosting.clone(),
        updates,
        tray: None,
        announced: HashSet::new(),
    });
    cx.observe(&hosting, |_, cx| HostIcon::refresh(cx)).detach();
    cx.on_system_notification_response(|response, cx| {
        if response.tag.starts_with("pong-pair-") {
            crate::app::open(Show::Devices, cx);
        }
    });
    HostIcon::refresh(cx);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::api::{AgentStatus, Pending, Session, Status};
    use pingpong_update::Update;

    fn ready(status: Status) -> Snapshot {
        Snapshot {
            conn: Conn::Ready,
            status: Some(status),
            ..Snapshot::default()
        }
    }

    fn titles(items: &[TrayItem]) -> Vec<&str> {
        items
            .iter()
            .map(|item| match item {
                TrayItem::Separator => "-",
                TrayItem::Label(text) | TrayItem::Action { title: text, .. } => text,
            })
            .collect()
    }

    const ON: State = State::On { api: true };

    #[test]
    fn the_menu_says_what_the_host_is_doing() {
        let idle = ready(Status::default());
        assert_eq!(state_line(&ON, &idle), "Hosting: ready for your devices");
        let streaming = ready(Status {
            session: Some(Session {
                client: "macbook".into(),
                ..Session::default()
            }),
            ..Status::default()
        });
        assert_eq!(state_line(&ON, &streaming), "Streaming to macbook");
        let agent = ready(Status {
            session: Some(Session {
                client: "macbook agent".into(),
                agent: Some(AgentStatus::default()),
                ..Session::default()
            }),
            ..Status::default()
        });
        assert_eq!(
            state_line(&ON, &agent),
            "An AI agent is at work (macbook agent)"
        );
        let starting = Snapshot {
            conn: Conn::NotRunning,
            ..Snapshot::default()
        };
        assert_eq!(
            state_line(&State::Starting, &starting),
            "The host is starting…"
        );
        assert_eq!(
            state_line(&State::Failed("exit 3".into()), &starting),
            "The host stopped"
        );
        assert_eq!(
            titles(&menu(&ON, &idle, &UpdateStatus::default())),
            [
                "Hosting: ready for your devices",
                "-",
                "Open Ping",
                "-",
                "Check for Updates…",
                "-",
                "Quit Ping"
            ]
        );
    }

    #[test]
    fn a_device_asking_to_pair_is_in_the_menu_by_name() {
        let request = |id: u32, name: &str| Pending {
            id,
            client_name: name.into(),
            ..Pending::default()
        };
        let one = ready(Status {
            pending: vec![request(7, "MacBook Air")],
            ..Status::default()
        });
        assert!(titles(&menu(&ON, &one, &UpdateStatus::default()))
            .contains(&"MacBook Air Wants to Pair…"));
        let two = ready(Status {
            pending: vec![request(7, "MacBook Air"), request(8, "living-room")],
            ..Status::default()
        });
        assert!(
            titles(&menu(&ON, &two, &UpdateStatus::default())).contains(&"2 Devices Want to Pair…")
        );
    }

    #[test]
    fn an_update_takes_the_place_of_the_check_in_the_menu() {
        let updates = UpdateStatus {
            update: Some(Update::Release {
                version: "0.7.0".into(),
                url: String::new(),
            }),
            ..UpdateStatus::default()
        };
        let items = menu(&ON, &ready(Status::default()), &updates);
        assert!(titles(&items).contains(&"Ping 0.7.0 Is Available…"));
        assert!(!titles(&items).contains(&"Check for Updates…"));
        assert_eq!(
            title_case("4 newer commits on main"),
            "4 Newer Commits on Main"
        );
    }
}
