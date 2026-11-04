//! Before the host can be shown: the host not answering (starting, or
//! failed), or another user's (its token is not this user's to read).
//! Pages like the others: a title at the top, rows under it.

use gpui::{div, prelude::*, px, AnyElement, Context};
use helio_ui::{
    button, notice, page_header, rows, section, setting, spinner, IconName, Ink, Theme, Type,
};

use super::switch::State;
use super::worker::Cmd;

use super::{page, HostPages};

impl HostPages {
    /// Hosting is on, but the host's API does not answer: it is starting,
    /// or it failed (and why), with a way to start it again.
    pub(super) fn not_running(&mut self, t: Theme, cx: &mut Context<Self>) -> AnyElement {
        let (state, busy, error) = {
            let h = self.hosting.read(cx);
            (h.state.clone(), h.busy, h.error.clone())
        };
        let (title, line) = match &state {
            State::Failed(why) => (
                "The host stopped",
                format!("Hosting is on, but the host does not run: {why}."),
            ),
            State::Starting | State::On { .. } | State::Unknown => (
                "Starting the host",
                "This page shows the host as soon as it answers.".to_string(),
            ),
            State::Off => (
                "Hosting is off",
                "Turn it on in Settings → Hosting.".to_string(),
            ),
        };
        let again: AnyElement = if busy.is_some() {
            spinner("host-switching", 13.0, t.tertiary).into_any_element()
        } else {
            let hosting = self.hosting.clone();
            button("start-host", "Start It Again", t)
                .on_click(cx.listener(move |this, _, _, cx| {
                    hosting.update(cx, |h, cx| h.turn(true, cx));
                    this.send(Cmd::Refresh);
                    cx.notify();
                }))
                .into_any_element()
        };
        // Asked again all along until it answers: the arrow turns, and a
        // click asks now.
        let waiting = div().flex().items_center().child(
            button("retry", "Look Again", t)
                .icon(IconName::Refresh)
                .spinning(true)
                .on_click(cx.listener(|this, _, _, cx| {
                    this.send(Cmd::Refresh);
                    cx.notify();
                })),
        );
        let content = div()
            .flex()
            .flex_col()
            .gap(px(22.0))
            .child(page_header(title, Some(line.into()), None, t))
            .children(error.map(|e| notice(IconName::Warning, Ink::DANGER, e, t)))
            .child(
                section("", t).child(rows(
                    [
                        setting(
                            "Start the host again",
                            Some(
                                "Runs helio-host enable once more: the system starts the host and \
                                    keeps it running."
                                    .into(),
                            ),
                            again,
                            t,
                        )
                        .into_any_element(),
                        setting(
                            "Waiting for it",
                            Some("Looking for the host on this computer.".into()),
                            waiting,
                            t,
                        )
                        .into_any_element(),
                    ],
                    t,
                )),
            );
        page(&self.page_scroll, t, "not-running-page", content)
    }

    /// The host runs, but its token is not this user's to read: on Windows
    /// it is the user's who turned hosting on (and administrators'), with a
    /// way to make this user that one; elsewhere the host is another
    /// user's, who runs it.
    pub(super) fn not_allowed(&mut self, t: Theme, cx: &mut Context<Self>) -> AnyElement {
        let line = if cfg!(windows) {
            "Another user of this PC turned hosting on, so its token is theirs to read. \
                Turn it on again from this account to manage it here."
        } else {
            "Helio Host runs here as another user, so its token is theirs. Open Helio as \
                that user to manage it."
        };
        let mut content = div().flex().flex_col().gap(px(22.0)).child(page_header(
            "Another user's host",
            Some(line.into()),
            None,
            t,
        ));
        if cfg!(windows) {
            let busy = self.hosting.read(cx).busy.is_some();
            let take: AnyElement = if busy {
                spinner("host-switching", 13.0, t.tertiary).into_any_element()
            } else {
                let hosting = self.hosting.clone();
                button("own-host", "Turn On Here", t)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        hosting.update(cx, |h, cx| h.turn(true, cx));
                        this.send(Cmd::Refresh);
                        cx.notify();
                    }))
                    .into_any_element()
            };
            content = content.child(section("", t).child(rows(
                [setting(
                    "Turn hosting on from this account",
                    Some(
                        "Windows asks for an administrator; the host then starts again with its \
                            token readable by you, and no longer by the other user."
                            .into(),
                    ),
                    take,
                    t,
                )
                .into_any_element()],
                t,
            )));
        }
        page(&self.page_scroll, t, "not-allowed-page", content)
    }
}

/// While something is on its way (the connection, the host's first
/// answer): what, with a spinner, where a page's title goes.
pub(super) fn waiting(id: &'static str, what: &'static str, t: Theme) -> AnyElement {
    page(
        &gpui::ScrollHandle::new(),
        t,
        id,
        div()
            .flex()
            .items_center()
            .gap(px(8.0))
            .text_size(px(Type::BODY))
            .text_color(t.tertiary)
            .child(spinner("waiting-spin", 13.0, t.tertiary))
            .child(what),
    )
}
