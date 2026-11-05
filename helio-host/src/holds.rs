//! What holds an AI agent from one of its sessions to the next.
//!
//! While an agent's session runs, what holds its input is the session's
//! (`session`'s `AgentRun`): a person paused it, a watcher took over, or
//! someone used the host's own keyboard or mouse. An agent can end its
//! session and ask for another at any moment (helio-agent's `connect`
//! tool does, and so does any action after the session ended), so what held
//! the old session is kept here and the new one starts held the same way.
//! A hold lasts until a person lets go: **Resume**, **Hand back**, or the
//! host's keyboard and mouse left alone for the hold's length. Without
//! this, an agent shed every hold by reconnecting (docs/ai-agents.md, the
//! rules the host holds agents to). A pause or a take-over is the person's
//! who took it, or the host's own Helio's to let go ([`let_go`]), never
//! another watcher's, who cannot take it from them either ([`take_over`]):
//! the key that takes over and hands back only knows that someone holds the
//! agent, so another watcher pressing it would hand the agent the host while
//! that person is at it.
//!
//! Someone at the host holds every agent, not only the one whose session
//! saw them: it is the host they are using. Between sessions nothing looks,
//! so a session about to start asks the platform how long the host's own
//! keyboard and mouse have been left alone, and compares it with the last
//! input a session injected, as a running session does (`local_hold`).
//!
//! Stopping an agent is kept with its pairing instead (`clients`'
//! `stopped`), so that it outlasts a restart of the host. What is kept
//! here lives as long as Helio Host's process: a restart ends every session
//! and forgets a pause or a take-over.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use helio_transport::PeerId;

/// Input at the host this much more recent than the last a session
/// injected is someone else's: the session's own takes a moment to
/// register.
pub const INJECTION_MARGIN: Duration = Duration::from_millis(300);

/// Who acts on an agent from the host's own Helio, through the API. No
/// device has this id: the endpoint never gives a peer 0 (`add_peer`).
pub const HOST_ITSELF: PeerId = 0;

/// `by` lets go of `hold`, which is the holder's to let go or the host's own
/// Helio's. Whether it was let go; someone else's stays as it is.
pub fn let_go(hold: &mut Option<PeerId>, by: PeerId) -> bool {
    let theirs = hold.is_some_and(|holder| holder == by || by == HOST_ITSELF);
    if theirs {
        *hold = None;
    }
    theirs
}

/// `by` takes the keyboard and mouse, unless someone else has them: taking
/// them, they could then hand back what that person holds. Whether `by` has
/// them now.
pub fn take_over(controller: &mut Option<PeerId>, by: PeerId) -> bool {
    let free = controller.is_none_or(|holder| holder == by);
    if free {
        *controller = Some(by);
    }
    free
}

/// A person's holds on one agent, from its last session.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Kept {
    /// Who paused it.
    pub paused: Option<PeerId>,
    /// The watcher who took over: they drive again when they watch again,
    /// and until they hand back, nobody else's input goes in.
    pub controller: Option<PeerId>,
}

/// The holds kept between sessions.
#[derive(Default)]
pub struct Holds {
    /// By the agent's key.
    agents: HashMap<[u8; 32], Kept>,
    /// Someone at the host holds agents until then.
    local_until: Option<Instant>,
    /// The last input a session injected (or, for a session that had just
    /// started, the end of its start's grace): input at the host after it
    /// is a person's.
    injected: Option<Instant>,
}

impl Holds {
    /// An agent's session ended: keep what a person held it with.
    pub fn keep(&mut self, agent: [u8; 32], kept: Kept) {
        if kept == Kept::default() {
            self.agents.remove(&agent);
        } else {
            self.agents.insert(agent, kept);
        }
    }

    /// A session ended (anyone's): when it last injected input, and until
    /// when someone at the host held its agent.
    pub fn session_ended(&mut self, injected: Option<Instant>, local_until: Option<Instant>) {
        self.injected = self.injected.max(injected);
        self.local_until = self.local_until.max(local_until);
    }

    /// What holds `agent`'s new session. It is the session's from now on,
    /// and kept here again when that ends.
    pub fn take(&mut self, agent: &[u8; 32]) -> Kept {
        self.agents.remove(agent).unwrap_or_default()
    }

    /// Until when someone at the host holds an agent's session starting
    /// `now`: as long as was kept, or longer for input at the host since
    /// the last a session injected. `idle`: how long the host's keyboard and
    /// mouse have been left alone, where the platform can tell; `hold`: how
    /// long input there holds an agent (0: it does not).
    pub fn local_until(
        &self,
        now: Instant,
        idle: Option<Duration>,
        hold: Duration,
    ) -> Option<Instant> {
        if hold.is_zero() {
            return None;
        }
        // No session injected anything yet: any input at the host was a
        // person's.
        let baseline = self
            .injected
            .unwrap_or_else(|| now.checked_sub(hold + INJECTION_MARGIN).unwrap_or(now));
        let seen = idle.and_then(|idle| local_hold(now, idle, baseline, hold));
        self.local_until.max(seen).filter(|&t| t > now)
    }
}

/// Whether input at the host `idle` ago is someone else's, the last input a
/// session injected (or the end of its start's grace) being `baseline`: it
/// is when it came more than [`INJECTION_MARGIN`] after it. If so, the agent
/// is held until `hold` after that input.
///
/// A heuristic: a person's key pressed within the margin of the agent's own
/// input goes unseen until their next one (docs/ai-agents.md).
pub fn local_hold(
    now: Instant,
    idle: Duration,
    baseline: Instant,
    hold: Duration,
) -> Option<Instant> {
    (idle + INJECTION_MARGIN < now.saturating_duration_since(baseline))
        .then(|| now.checked_sub(idle).unwrap_or(now) + hold)
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: [u8; 32] = [7; 32];
    const OTHER: [u8; 32] = [9; 32];
    const HOLD: Duration = Duration::from_secs(10);

    #[test]
    fn a_hold_is_let_go_by_the_person_who_took_it() {
        let mut hold = Some(5);
        assert!(let_go(&mut hold, 5));
        assert_eq!(hold, None);
    }

    #[test]
    fn another_watcher_does_not_let_go_of_someones_hold() {
        let mut hold = Some(5);
        assert!(!let_go(&mut hold, 6));
        assert_eq!(hold, Some(5));
    }

    #[test]
    fn the_hosts_own_helio_lets_go_of_anyones_hold() {
        let mut hold = Some(5);
        assert!(let_go(&mut hold, HOST_ITSELF));
        assert_eq!(hold, None);
    }

    #[test]
    fn a_watcher_does_not_let_go_of_a_hold_from_the_hosts_own_helio() {
        let mut hold = Some(HOST_ITSELF);
        assert!(!let_go(&mut hold, 5));
        assert_eq!(hold, Some(HOST_ITSELF));
    }

    #[test]
    fn a_watcher_takes_over_when_nobody_has_the_keyboard_and_mouse() {
        let mut controller = None;
        assert!(take_over(&mut controller, 5));
        assert_eq!(controller, Some(5));
        // Asked again, control messages being sent twice.
        assert!(take_over(&mut controller, 5));
        assert_eq!(controller, Some(5));
    }

    #[test]
    fn a_watcher_does_not_take_over_from_another_person() {
        let mut controller = Some(5);
        assert!(!take_over(&mut controller, 6));
        assert_eq!(controller, Some(5));
    }

    #[test]
    fn nobody_lets_go_of_a_hold_that_is_not_there() {
        let mut hold = None;
        assert!(!let_go(&mut hold, 5));
        assert!(!let_go(&mut hold, HOST_ITSELF));
    }

    #[test]
    fn a_paused_agent_starts_its_next_session_paused() {
        let mut holds = Holds::default();
        holds.keep(
            KEY,
            Kept {
                paused: Some(HOST_ITSELF),
                controller: None,
            },
        );
        assert_eq!(holds.take(&KEY).paused, Some(HOST_ITSELF));
        // Another agent was not paused.
        assert_eq!(holds.take(&OTHER), Kept::default());
    }

    #[test]
    fn a_take_over_outlasts_the_agents_session_until_handed_back() {
        let mut holds = Holds::default();
        let kept = Kept {
            paused: None,
            controller: Some(42),
        };
        holds.keep(KEY, kept);
        let session = holds.take(&KEY);
        assert_eq!(session.controller, Some(42));
        // That session ends still taken over: the next one is too.
        holds.keep(KEY, session);
        assert_eq!(holds.take(&KEY).controller, Some(42));
        // Handed back, then ended: nothing holds the next.
        holds.keep(KEY, Kept::default());
        assert_eq!(holds.take(&KEY), Kept::default());
    }

    #[test]
    fn someone_at_the_host_holds_the_agents_next_session() {
        let now = Instant::now();
        let mut holds = Holds::default();
        let until = now + Duration::from_secs(8);
        holds.session_ended(Some(now - Duration::from_secs(1)), Some(until));
        // The host's keyboard and mouse look idle since the agent's own
        // input: the kept hold still holds.
        let idle = Some(Duration::from_secs(1));
        assert_eq!(holds.local_until(now, idle, HOLD), Some(until));
        // And it ends when it ends.
        let later = until + Duration::from_millis(1);
        assert_eq!(holds.local_until(later, None, HOLD), None);
    }

    #[test]
    fn input_at_the_host_since_the_last_session_holds_a_new_one() {
        let now = Instant::now();
        let mut holds = Holds::default();
        holds.session_ended(Some(now - Duration::from_secs(5)), None);
        // Someone typed 2 s ago, 3 s after the last injected input.
        let typed = Duration::from_secs(2);
        assert_eq!(
            holds.local_until(now, Some(typed), HOLD),
            Some(now - typed + HOLD)
        );
        // The session's own input is not someone at the host.
        let own = Duration::from_secs(5);
        assert_eq!(holds.local_until(now, Some(own), HOLD), None);
    }

    #[test]
    fn with_no_session_before_any_recent_input_at_the_host_holds() {
        let now = Instant::now();
        let holds = Holds::default();
        assert!(holds
            .local_until(now, Some(Duration::from_secs(4)), HOLD)
            .is_some());
        assert_eq!(
            holds.local_until(now, Some(Duration::from_secs(60)), HOLD),
            None
        );
        // A platform that cannot tell holds nothing new.
        assert_eq!(holds.local_until(now, None, HOLD), None);
    }

    #[test]
    fn nothing_holds_with_the_local_input_hold_off() {
        let now = Instant::now();
        let mut holds = Holds::default();
        holds.session_ended(None, Some(now + HOLD));
        assert_eq!(
            holds.local_until(now, Some(Duration::ZERO), Duration::ZERO),
            None
        );
    }

    #[test]
    fn input_within_the_margin_of_the_last_injected_is_the_sessions_own() {
        let now = Instant::now();
        let baseline = now - Duration::from_secs(1);
        // 900 ms idle: 100 ms after the baseline, inside the margin.
        assert_eq!(
            local_hold(now, Duration::from_millis(900), baseline, HOLD),
            None
        );
        // 500 ms idle: 500 ms after it, someone else's.
        let idle = Duration::from_millis(500);
        assert_eq!(
            local_hold(now, idle, baseline, HOLD),
            Some(now - idle + HOLD)
        );
        // A baseline still ahead (a start's grace) sees nothing.
        assert_eq!(
            local_hold(now, Duration::ZERO, now + Duration::from_secs(2), HOLD),
            None
        );
    }
}
