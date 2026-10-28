//! The sidebar: Hosts, Agents, the open sessions, the settings pages.

use gpui::{div, prelude::*, px, AnyElement, Context, ElementId, FontWeight};
use pingpong_ui::{dot, icon, spinner, IconName, Ink, Radius, Theme, Type};

use crate::settings::Tab;

use super::{Page, PingApp};

/// Ping's ball, as in its icon.
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

impl PingApp {
    pub(super) fn sidebar(&mut self, t: Theme, cx: &mut Context<Self>) -> impl IntoElement {
        let page = self.page;
        let paired = self.model.items.iter().filter(|i| i.is_paired()).count();
        let glide = self.nav_glide.clone();
        let nav = |id: &'static str,
                   label: &'static str,
                   name: IconName,
                   target: Page,
                   trailing: Option<AnyElement>,
                   cx: &mut Context<Self>| {
            let on = page == target;
            pingpong_ui::nav_row(id, label, name, on, nav_index(target), &glide, false, t)
                .when(!on, |d| {
                    d.on_click(cx.listener(move |this, _, _, cx| this.set_page(target, cx)))
                })
                .children(trailing)
        };
        let agent_badge = if self.agents.is_running() {
            Some(pingpong_ui::live_dot(Ink::FRESH).into_any_element())
        } else if self.agents.awaiting_answer() {
            Some(dot(t.attention, 7.0).into_any_element())
        } else {
            None
        };
        let count = (paired > 0).then(|| pingpong_ui::nav_count(paired, t).into_any_element());
        let footer: AnyElement = if self.model.searching {
            div()
                .flex()
                .items_center()
                .gap(px(6.0))
                .child(spinner("sidebar-searching", 11.0, t.tertiary))
                .child("Looking for hosts…")
                .into_any_element()
        } else {
            div()
                .flex()
                .items_center()
                .gap(px(7.0))
                .child(brand_mark(8.0))
                .child(format!("Ping {}", env!("CARGO_PKG_VERSION")))
                .into_any_element()
        };
        let sessions = self.session_rows(t, cx);
        pingpong_ui::sidebar(t)
            .child(
                pingpong_ui::nav_list(&glide, Some(nav_index(page)), t)
                    .child(nav(
                        "nav-hosts",
                        "Hosts",
                        IconName::Devices,
                        Page::Hosts,
                        count,
                        cx,
                    ))
                    .child(nav(
                        "nav-agents",
                        "Agents",
                        IconName::Agent,
                        Page::Agents,
                        agent_badge,
                        cx,
                    ))
                    .children(sessions)
                    .child(pingpong_ui::sidebar_header("Settings", t))
                    .child(nav(
                        "nav-general",
                        "General",
                        IconName::Settings,
                        Page::Settings(Tab::General),
                        None,
                        cx,
                    ))
                    .child(nav(
                        "nav-video",
                        "Video",
                        IconName::Display,
                        Page::Settings(Tab::Video),
                        None,
                        cx,
                    ))
                    .child(nav(
                        "nav-audio",
                        "Audio",
                        IconName::Speaker,
                        Page::Settings(Tab::Audio),
                        None,
                        cx,
                    ))
                    .child(nav(
                        "nav-input",
                        "Input",
                        IconName::Keyboard,
                        Page::Settings(Tab::Input),
                        None,
                        cx,
                    ))
                    .child(nav(
                        "nav-agent-setup",
                        "Agent setup",
                        IconName::Sparkle,
                        Page::Settings(Tab::Agents),
                        None,
                        cx,
                    )),
            )
            .children(
                pingpong_ui::updates::notice(&super::UPDATE_APP, &self.update_status, t).map(
                    |notice| {
                        notice.on_click(cx.listener(|this, _, _, cx| this.show_updates(false, cx)))
                    },
                ),
            )
            .child(pingpong_ui::sidebar_foot(t).child(footer))
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
                .border_color(pingpong_ui::rgba(0.0, 0.0, 0.0, 0.0))
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
                ChatState::Working => (
                    "Working",
                    pingpong_ui::live_dot(Ink::FRESH).into_any_element(),
                ),
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
                    pingpong_ui::live_dot(Ink::FRESH).into_any_element(),
                )
                .on_click(cx.listener(move |this, _, _, cx| this.set_page(Page::Desktop(id), cx)))
                .into_any_element(),
            );
        }
        out
    }
}

/// Where `page`'s row is among the sidebar's, for the pill that slides
/// between them.
fn nav_index(page: Page) -> usize {
    match page {
        Page::Hosts => 0,
        Page::Agents => 1,
        Page::Settings(tab) => 2 + tab as usize,
        // Sessions, after the fixed rows; one id space each.
        Page::Chat(id) => 100 + 2 * id as usize,
        Page::Desktop(id) => 101 + 2 * id as usize,
    }
}
