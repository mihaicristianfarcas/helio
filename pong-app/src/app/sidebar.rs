//! The sidebar: the pages, and the host's name and state at its foot.

use gpui::{div, prelude::*, px, AnyElement, Context, FontWeight};
use pingpong_ui::{dot, icon, icon_button, live_dot, IconName, Ink, Radius, Theme, Type};

use crate::worker::Conn;

use super::{Confirm, Page, PongApp};

impl PongApp {
    pub(super) fn sidebar(&mut self, t: Theme, cx: &mut Context<Self>) -> impl IntoElement {
        let page = self.page;
        let ready = self.snap.conn == Conn::Ready;
        let pending = self
            .snap
            .status
            .as_ref()
            .map(|s| s.pending.len())
            .unwrap_or(0);
        let streaming = self
            .snap
            .status
            .as_ref()
            .and_then(|s| s.session.as_ref())
            .is_some();
        let glide = self.nav_glide.clone();
        let nav = |id: &'static str,
                   label: &'static str,
                   name: IconName,
                   target: Page,
                   trailing: Option<AnyElement>,
                   cx: &mut Context<Self>| {
            let on = page == target && ready;
            pingpong_ui::nav_row(id, label, name, on, target as usize, &glide, false, t)
                .when(!ready, |d| d.opacity(0.45))
                .when(!on && ready, |d| {
                    d.on_click(cx.listener(move |this, _, _, cx| this.set_page(target, cx)))
                })
                .children(trailing)
        };
        let overview_badge = streaming.then(|| live_dot(Ink::FRESH).into_any_element());
        let devices_badge = (pending > 0).then(|| {
            div()
                .px(px(6.0))
                .rounded(px(8.0))
                .bg(t.accent)
                .text_size(px(Type::META))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(t.on_accent)
                .child(pending.to_string())
                .into_any_element()
        });
        let (dot_color, state) = match &self.snap.conn {
            Conn::Ready if streaming => (t.fresh, "Streaming"),
            Conn::Ready => (t.fresh, "Running"),
            Conn::Connecting => (Ink::IDLE, "Connecting…"),
            Conn::NotRunning => (t.danger, "Not running"),
            Conn::SignIn { .. } => (t.attention, "Signed out"),
        };
        let host = if self.snap.state.name.is_empty() {
            "This computer".to_string()
        } else {
            self.snap.state.name.clone()
        };
        let can_sign_out = ready && !self.snap.local;
        pingpong_ui::sidebar(t)
            .child(
                pingpong_ui::nav_list(&glide, ready.then_some(page as usize), t)
                    .child(nav(
                        "nav-overview",
                        "Overview",
                        IconName::Activity,
                        Page::Overview,
                        overview_badge,
                        cx,
                    ))
                    .child(nav(
                        "nav-devices",
                        "Devices",
                        IconName::Devices,
                        Page::Devices,
                        devices_badge,
                        cx,
                    ))
                    .child(pingpong_ui::sidebar_header("Settings", t))
                    .child(nav(
                        "nav-general",
                        "General",
                        IconName::Settings,
                        Page::General,
                        None,
                        cx,
                    ))
                    .child(nav(
                        "nav-video",
                        "Video",
                        IconName::Display,
                        Page::Video,
                        None,
                        cx,
                    ))
                    .child(nav(
                        "nav-network",
                        "Network",
                        IconName::Globe,
                        Page::Network,
                        None,
                        cx,
                    ))
                    .child(nav(
                        "nav-agents",
                        "AI agents",
                        IconName::Agent,
                        Page::Agents,
                        None,
                        cx,
                    ))
                    .child(div().h(px(10.0)))
                    .child(nav(
                        "nav-logs",
                        "Logs",
                        IconName::List,
                        Page::Logs,
                        None,
                        cx,
                    )),
            )
            .children(
                pingpong_ui::updates::notice(&super::UPDATE_APP, &self.updates.status(), t).map(
                    |notice| notice.on_click(cx.listener(|this, _, _, cx| this.show_updates(cx))),
                ),
            )
            .child(
                div()
                    .flex_none()
                    .mx(px(8.0))
                    .mb(px(8.0))
                    .px(px(8.0))
                    .py(px(8.0))
                    .flex()
                    .items_center()
                    .gap(px(9.0))
                    .rounded(px(Radius::ROW))
                    .child(
                        div()
                            .flex_none()
                            .size(px(28.0))
                            .rounded(px(Radius::ROW))
                            .bg(t.primary.alpha(0.06))
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(icon(
                                if self.os() == "macos" {
                                    IconName::Laptop
                                } else {
                                    IconName::Monitor
                                },
                                15.0,
                                t.secondary,
                            )),
                    )
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
                                    .text_size(px(12.5))
                                    .font_weight(FontWeight::MEDIUM)
                                    .child(host),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(5.0))
                                    .text_size(px(Type::META))
                                    .text_color(t.tertiary)
                                    .child(dot(dot_color, 6.0))
                                    .child(state),
                            ),
                    )
                    .when(can_sign_out, |d| {
                        d.child(
                            icon_button("sign-out", IconName::Power, "Sign out of Pong here", t)
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.confirm = Some(Confirm::SignOut);
                                    cx.notify();
                                })),
                        )
                    }),
            )
    }
}
