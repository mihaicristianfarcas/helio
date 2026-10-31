//! Before the host can be shown: the host not answering (starting, or
//! failed), or signing in (Windows).
//! Pages like the others: a title at the top, rows under it.

use gpui::{div, prelude::*, px, AnyElement, Context, Window};
use helio_ui::{
    button, field, notice, page_header, rows, section, setting, spinner, IconName, Ink, TextField,
    Theme, Type,
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
        let waiting = div()
            .flex()
            .items_center()
            .gap(px(8.0))
            .child(spinner("waiting-host", 11.0, t.tertiary))
            .child(
                button("retry", "Look Again", t).on_click(cx.listener(|this, _, _, cx| {
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

    pub(super) fn sign_in_page(
        &mut self,
        setup: bool,
        t: Theme,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let host = if self.snap.state.name.is_empty() {
            "this computer".to_string()
        } else {
            self.snap.state.name.clone()
        };
        let (title, detail, action) = if setup {
            (
                "Create Helio Host's admin account".to_string(),
                format!(
                    "Helio is not an administrator here, so it signs in to the host on {host} \
                        once, with this account. The password needs 8 characters or more."
                ),
                "Create Account",
            )
        } else {
            (
                format!("Sign in to Helio Host on {host}"),
                "Use the account you made for Helio Host on this computer. This window stays \
                 signed in."
                    .to_string(),
                "Sign In",
            )
        };
        if !self.user.read(cx).is_focused(window)
            && !self.password.read(cx).is_focused(window)
            && self.user.read(cx).text().is_empty()
        {
            TextField::focus(&self.user, window, cx);
        }
        let ok = !self.user.read(cx).text().trim().is_empty()
            && !self.password.read(cx).text().is_empty()
            && !self.signing_in;
        let content = div()
            .flex()
            .flex_col()
            .gap(px(22.0))
            .child(page_header(title, Some(detail.into()), None, t))
            .child(
                section("", t).child(rows(
                    [
                        setting(
                            "User name",
                            None,
                            div().w(px(240.0)).child(field(&self.user, t)),
                            t,
                        )
                        .into_any_element(),
                        setting(
                            "Password",
                            None,
                            div().w(px(240.0)).child(field(&self.password, t)),
                            t,
                        )
                        .into_any_element(),
                    ],
                    t,
                )),
            )
            .children(
                self.sign_in_error
                    .clone()
                    .map(|e| notice(IconName::Warning, Ink::DANGER, e, t)),
            )
            .child(
                div().flex().justify_end().child(
                    button(
                        "sign-in",
                        if self.signing_in {
                            "Signing in…"
                        } else {
                            action
                        },
                        t,
                    )
                    .accent()
                    .disabled(!ok)
                    .on_click(cx.listener(|this, _, _, cx| this.sign_in(cx))),
                ),
            );
        page(&self.page_scroll, t, "sign-in-page", content)
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
