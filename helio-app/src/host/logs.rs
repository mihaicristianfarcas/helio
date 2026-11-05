//! The host's log, following its end.

use gpui::{div, prelude::*, px, AnyElement, ClipboardItem, Context};
use helio_ui::{button, IconName, Ink, Metrics, Radius, Theme};

use super::HostPages;

impl HostPages {
    pub(super) fn logs(&mut self, t: Theme, _cx: &mut Context<Self>) -> AnyElement {
        let text = self.snap.logs.clone().unwrap_or_default();
        let copy = text.clone();
        let lines: Vec<AnyElement> = text
            .lines()
            .map(|l| {
                // "2026-09-29T07:23:36.084844Z  INFO helio_host::host: …": the time
                // of day, the level, the rest.
                let mut parts = l.splitn(2, ' ');
                let stamp = parts.next().unwrap_or_default();
                let rest = parts.next().unwrap_or_default().trim_start();
                let (level, message) = rest.split_once(' ').unwrap_or(("", rest));
                let time = stamp
                    .get(11..19)
                    .filter(|_| stamp.len() > 20 && stamp.as_bytes()[10] == b'T');
                let Some(time) = time else {
                    return div()
                        .text_color(t.primary.alpha(0.78))
                        .child(l.to_string())
                        .into_any_element();
                };
                let tint = match level {
                    "ERROR" => t.ink(Ink::DANGER),
                    "WARN" => t.ink(Ink::ATTENTION),
                    _ => t.tertiary,
                };
                div()
                    .flex()
                    .gap(px(10.0))
                    .child(
                        div()
                            .flex_none()
                            .text_color(t.tertiary)
                            .child(time.to_string()),
                    )
                    .child(
                        div()
                            .flex_none()
                            .w(px(40.0))
                            .text_color(tint)
                            .child(level.to_string()),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_color(t.primary.alpha(0.82))
                            .child(message.trim_start().to_string()),
                    )
                    .into_any_element()
            })
            .collect();
        let empty = lines.is_empty();
        let actions = helio_ui::page_actions(vec![button("copy-logs", "Copy", t)
            .icon(IconName::Copy)
            .disabled(empty)
            .on_click(move |_, _, cx| {
                cx.write_to_clipboard(ClipboardItem::new_string(copy.clone()))
            })
            .into_any_element()]);
        div()
            .size_full()
            .flex()
            .flex_col()
            .child(helio_ui::page_strip())
            // The page's column, as every page's; the log fills the rest of
            // the window's height.
            .child(
                div()
                    .flex_none()
                    .flex()
                    .justify_center()
                    .px(px(28.0))
                    .pb(px(14.0))
                    .child(div().w_full().min_w_0().max_w(px(Metrics::PAGE)).child(
                        helio_ui::page_header(
                            "Logs",
                            Some("The last 400 lines of helio-host.log.".into()),
                            Some(actions),
                            t,
                        ),
                    )),
            )
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .justify_center()
                    .px(px(28.0))
                    .pb(px(24.0))
                    .child(
                        div()
                            .id("logs")
                            .h_full()
                            .w_full()
                            .min_w_0()
                            .max_w(px(Metrics::PAGE))
                            .overflow_y_scroll()
                            .track_scroll(&self.logs_scroll)
                            .rounded(px(Radius::CARD))
                            .border_1()
                            .border_color(t.card_stroke)
                            .bg(t.card)
                            .p(px(12.0))
                            .font_family(helio_ui::mono_font())
                            .text_size(px(11.0))
                            .line_height(px(16.0))
                            .when(empty, |d| {
                                d.child(div().text_color(t.tertiary).child("Reading the log…"))
                            })
                            .children(lines),
                    ),
            )
            .into_any_element()
    }
}
