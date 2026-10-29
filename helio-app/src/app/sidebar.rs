//! The sidebar, as asides' (`window.rs`): Hosts; This computer (one row
//! while hosting is off, the host's pages while it is on); Agents (a new
//! session) and the open sessions; the settings pages; and a foot that says
//! the version, or offers an update. The pages are one stop for Tab, the arrows going up
//! and down them, each shown as the keyboard comes to it; the glass pill
//! slides to the page shown.

use gpui::{
    div, prelude::*, px, AnyElement, Context, ElementId, FocusHandle, FontWeight, KeyDownEvent,
};
use helio_ui::{dot, icon, spinner, IconName, Ink, Radius, Theme, Type};

use crate::host::Page as HostPage;
use crate::settings::Tab;

use super::{HelioApp, Page};

/// Helio's ball, as in its icon.
pub fn brand_mark(size: f32) -> impl IntoElement {
    div()
        .flex_none()
        .size(px(size))
        .rounded(px(size / 2.0))
        .bg(Ink::BALL)
        .shadow(vec![gpui::BoxShadow {
            color: Ink::BALL.alpha(0.45).into(),
            offset: gpui::point(px(0.0), px(0.0)),
            blur_radius: px(size * 0.6),
            spread_radius: px(0.0),
            inset: false,
        }])
}

/// What a sidebar row shows: the page it goes to, its id, name and icon.
struct Nav {
    page: Page,
    id: &'static str,
    label: &'static str,
    glyph: IconName,
}

const fn nav(page: Page, id: &'static str, label: &'static str, glyph: IconName) -> Nav {
    Nav {
        page,
        id,
        label,
        glyph,
    }
}

/// The settings pages, in order.
const SETTINGS: [Nav; 7] = [
    nav(
        Page::Settings(Tab::General),
        "nav-general",
        "General",
        IconName::Settings,
    ),
    nav(
        Page::Settings(Tab::Video),
        "nav-video",
        "Video",
        IconName::Display,
    ),
    nav(
        Page::Settings(Tab::Audio),
        "nav-audio",
        "Audio",
        IconName::Speaker,
    ),
    nav(
        Page::Settings(Tab::Input),
        "nav-input",
        "Input",
        IconName::Keyboard,
    ),
    nav(
        Page::Settings(Tab::Agents),
        "nav-agent-setup",
        "Agent setup",
        IconName::Sparkle,
    ),
    nav(
        Page::Settings(Tab::Hosting),
        "nav-hosting",
        "Hosting",
        IconName::Server,
    ),
    nav(
        Page::Settings(Tab::Updates),
        "nav-updates",
        "Updates",
        IconName::Download,
    ),
];

/// This computer's pages while hosting is on; its one page while it is off.
/// Its settings are the host's, in Settings → Hosting.
const HOST_PAGES: [Nav; 3] = [
    nav(
        Page::Host(HostPage::Overview),
        "nav-host-overview",
        "Overview",
        IconName::Activity,
    ),
    nav(
        Page::Host(HostPage::Devices),
        "nav-host-devices",
        "Devices",
        IconName::Devices,
    ),
    nav(
        Page::Host(HostPage::Logs),
        "nav-host-logs",
        "Logs",
        IconName::List,
    ),
];
const HOSTING_OFF: Nav = nav(
    Page::Hosting,
    "nav-hosting-off",
    "Overview",
    IconName::Activity,
);

impl HelioApp {
    /// The pages the sidebar lists now, in order (sessions after Agents).
    fn nav_pages(&self) -> Vec<Page> {
        let mut pages = vec![Page::Hosts];
        if self.host.is_some() {
            pages.extend(HOST_PAGES.iter().map(|n| n.page));
        } else {
            pages.push(HOSTING_OFF.page);
        }
        pages.push(Page::Agents);
        pages.extend(self.session_pages());
        pages.extend(SETTINGS.iter().map(|n| n.page));
        pages
    }

    /// A sidebar row's focus handle (one stop for Tab: the page shown's).
    fn nav_focus(&mut self, page: Page, cx: &mut Context<Self>) -> (FocusHandle, bool) {
        let handle = self
            .nav_focus
            .entry(nav_index(page))
            .or_insert_with(|| cx.focus_handle())
            .clone();
        let ring = helio_ui::keyboard_focus() && Some(&handle) == self.focused.as_ref();
        (handle, ring)
    }

    pub(super) fn sidebar(&mut self, t: Theme, cx: &mut Context<Self>) -> impl IntoElement {
        let page = self.page;
        let shown = page;
        let paired = self.model.items.iter().filter(|i| i.is_paired()).count();
        let glide = self.nav_glide.clone();
        let order = self.nav_pages();
        let focus: std::collections::HashMap<usize, (FocusHandle, bool)> = order
            .iter()
            .map(|p| (nav_index(*p), self.nav_focus(*p, cx)))
            .collect();
        // The arrows go up and down the pages, each shown as they come to it.
        let handles: Vec<(Page, FocusHandle)> = order
            .iter()
            .map(|p| (*p, focus[&nav_index(*p)].0.clone()))
            .collect();
        let host = self.host.clone();
        let agent_badge = if self.agents.is_running() {
            Some(helio_ui::live_dot(t.fresh).into_any_element())
        } else if self.agents.awaiting_answer() {
            Some(dot(t.attention, 7.0).into_any_element())
        } else {
            None
        };
        let sessions = self.session_rows(t, cx);
        let arrows = cx.listener(move |this, e: &KeyDownEvent, window, cx| {
            let at = handles.iter().position(|(_, h)| h.is_focused(window));
            let Some(to) = helio_ui::arrow_step(e, at, handles.len(), false) else {
                return;
            };
            cx.stop_propagation();
            helio_ui::keyboard_moved();
            window.focus(&handles[to].1, cx);
            this.set_page(handles[to].0, cx);
        });
        let row = |n: &Nav, trailing: Option<AnyElement>, cx: &mut Context<Self>| {
            let target = n.page;
            let on = shown == target;
            let (handle, ring) = focus
                .get(&nav_index(target))
                .cloned()
                .unwrap_or_else(|| (cx.focus_handle(), false));
            helio_ui::nav_row(
                n.id,
                n.label,
                n.glyph,
                on,
                nav_index(target),
                &glide,
                ring,
                t,
            )
            .track_focus(&handle.tab_stop(on))
            .when(!on, |d| {
                d.on_click(cx.listener(move |this, _, _, cx| this.set_page(target, cx)))
            })
            .children(trailing)
            .into_any_element()
        };
        let mut list = helio_ui::nav_list(&glide, Some(nav_index(shown)), t).on_key_down(arrows);
        let count = (paired > 0).then(|| helio_ui::nav_count(paired, t).into_any_element());
        list = list.child(row(
            &nav(Page::Hosts, "nav-hosts", "Hosts", IconName::Devices),
            count,
            cx,
        ));
        list = list.child(helio_ui::sidebar_header("This computer", t));
        if let Some(host) = host {
            let snap = host.read(cx).snapshot().clone();
            let streaming = snap.status.as_ref().is_some_and(|s| s.session.is_some());
            let pending = snap.status.as_ref().map_or(0, |s| s.pending.len());
            for n in &HOST_PAGES {
                let badge = match n.page {
                    Page::Host(HostPage::Overview) if streaming => {
                        Some(helio_ui::live_dot(t.fresh).into_any_element())
                    }
                    Page::Host(HostPage::Devices) if pending > 0 => Some(
                        div()
                            .px(px(6.0))
                            .rounded(px(8.0))
                            .bg(t.accent)
                            .text_size(px(Type::META))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(t.on_accent)
                            .child(pending.to_string())
                            .into_any_element(),
                    ),
                    _ => None,
                };
                list = list.child(row(n, badge, cx));
            }
        } else {
            list = list.child(row(&HOSTING_OFF, None, cx));
        }
        // A group of its own, as This computer's: a row under This computer's
        // pages read as the host's.
        list = list.child(helio_ui::sidebar_header("Agents", t));
        list = list.child(row(
            &nav(Page::Agents, "nav-agents", "New Session", IconName::Agent),
            agent_badge,
            cx,
        ));
        list = list.children(sessions);
        list = list.child(helio_ui::sidebar_header("Settings", t));
        for n in &SETTINGS {
            list = list.child(row(n, None, cx));
        }
        let footer: AnyElement = if self.model.searching {
            div()
                .flex()
                .items_center()
                .gap(px(6.0))
                .child(spinner("sidebar-searching", 11.0, t.tertiary))
                .child("Looking for hosts…")
                .into_any_element()
        } else {
            format!("Helio {}", env!("CARGO_PKG_VERSION")).into_any_element()
        };
        helio_ui::sidebar(t)
            .child(
                // Scrolls when the window is short.
                div()
                    .id("sidebar-pages")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .child(list),
            )
            .children(
                helio_ui::updates::notice(&super::UPDATE_APP, &self.update_status, t).map(
                    |notice| {
                        notice.on_click(cx.listener(|this, _, _, cx| this.show_updates(false, cx)))
                    },
                ),
            )
            .child(helio_ui::sidebar_foot(t).child(footer))
    }

    /// The open sessions' pages: agents' and your desktops.
    fn session_pages(&self) -> Vec<Page> {
        let chats = self.agents.chats.iter().map(|c| Page::Chat(c.id));
        let desktops = self
            .streams
            .iter()
            .filter(|s| s.chat.is_none())
            .map(|s| Page::Desktop(s.id));
        chats.chain(desktops).collect()
    }

    /// The sidebar's open sessions: agents' and your desktops.
    pub(super) fn session_rows(&mut self, t: Theme, cx: &mut Context<Self>) -> Vec<AnyElement> {
        use crate::chat::ChatState;
        let page = self.page;
        let mut out = Vec::new();
        let desktops: Vec<(u64, String, u64)> = self
            .streams
            .iter()
            .filter(|s| s.chat.is_none())
            .map(|s| (s.id, s.host.clone(), s.since.elapsed().as_secs()))
            .collect();
        if self.agents.chats.is_empty() && desktops.is_empty() {
            return out;
        }
        out.push(
            div()
                .pt(px(14.0))
                .pb(px(4.0))
                .px(px(9.0))
                .text_size(px(Type::META))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(t.tertiary)
                .child("Sessions")
                .into_any_element(),
        );
        let glide = self.nav_glide.clone();
        let row = |id: ElementId,
                   on: bool,
                   index: usize,
                   glyph: IconName,
                   title: String,
                   sub: String,
                   badge: AnyElement| {
            div()
                .id(id)
                .min_h(px(40.0))
                .px(px(8.0))
                .py(px(4.0))
                .flex()
                .items_center()
                .gap(px(8.0))
                .rounded(px(Radius::ROW))
                .relative()
                .border_1()
                .border_color(helio_ui::rgba(0.0, 0.0, 0.0, 0.0))
                .child(glide.item(index))
                .hover(|s| s.bg(t.hover))
                .when(!on, |d| d.cursor_pointer())
                .child(icon(glyph, 15.0, if on { t.primary } else { t.secondary }))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .child(
                            div()
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .text_ellipsis()
                                .text_size(px(Type::BODY))
                                .text_color(if on { t.primary } else { t.primary.alpha(0.85) })
                                .child(title),
                        )
                        .child(
                            div()
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .text_ellipsis()
                                .text_size(px(Type::META))
                                .text_color(t.tertiary)
                                .child(sub),
                        ),
                )
                .child(badge)
        };
        let watched: Vec<u64> = self.streams.iter().filter_map(|s| s.chat).collect();
        let chats: Vec<(u64, String, String, ChatState)> = self
            .agents
            .chats
            .iter()
            .map(|c| (c.id, c.title.clone(), c.host.clone(), c.state()))
            .collect();
        for (id, title, host, state) in chats {
            let (label, badge): (&str, AnyElement) = match state {
                ChatState::Working => {
                    ("Working", helio_ui::live_dot(Ink::FRESH).into_any_element())
                }
                ChatState::Waiting => ("Waiting for you", dot(t.attention, 7.0).into_any_element()),
                ChatState::Paused => ("Paused", dot(t.attention, 7.0).into_any_element()),
                ChatState::Connecting => (
                    "Connecting…",
                    spinner(
                        ElementId::Name(format!("side-spin-{id}").into()),
                        11.0,
                        t.tertiary,
                    )
                    .into_any_element(),
                ),
                ChatState::Connected => {
                    ("Connected", dot(t.ink(Ink::FRESH), 6.0).into_any_element())
                }
                ChatState::Idle => ("Idle", div().into_any_element()),
            };
            let sub = if watched.contains(&id) {
                format!("{host} · {label} · you're logged in")
            } else {
                format!("{host} · {label}")
            };
            out.push(
                row(
                    ElementId::Name(format!("side-chat-{id}").into()),
                    page == Page::Chat(id),
                    nav_index(Page::Chat(id)),
                    IconName::Agent,
                    title,
                    sub,
                    badge,
                )
                .on_click(cx.listener(move |this, _, _, cx| this.set_page(Page::Chat(id), cx)))
                .into_any_element(),
            );
        }
        for (id, host, secs) in desktops {
            let sub = format!(
                "Your desktop · {}",
                if secs < 60 {
                    "now".to_string()
                } else {
                    format!("{} min", secs / 60)
                }
            );
            out.push(
                row(
                    ElementId::Name(format!("side-desk-{id}").into()),
                    page == Page::Desktop(id),
                    nav_index(Page::Desktop(id)),
                    IconName::Display,
                    host,
                    sub,
                    helio_ui::live_dot(Ink::FRESH).into_any_element(),
                )
                .on_click(cx.listener(move |this, _, _, cx| this.set_page(Page::Desktop(id), cx)))
                .into_any_element(),
            );
        }
        out
    }
}

/// Where `page`'s row is among the sidebar's, for the pill that slides
/// between them (and its focus handle).
fn nav_index(page: Page) -> usize {
    match page {
        Page::Hosts => 0,
        Page::Agents => 1,
        Page::Hosting => 2,
        Page::Setup => 3,
        Page::Host(p) => 10 + p as usize,
        Page::Settings(tab) => 30 + tab as usize,
        // Sessions, after the fixed rows; one id space each.
        Page::Chat(id) => 100 + 2 * id as usize,
        Page::Desktop(id) => 101 + 2 * id as usize,
    }
}
