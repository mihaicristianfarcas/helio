//! GNOME: where a session's virtual monitor goes in the desktop, through
//! mutter's display configuration (`org.gnome.Mutter.DisplayConfig`, what
//! GNOME's display settings use). As on Windows, the virtual monitor
//! becomes the whole desktop for the session — the host's own monitors off,
//! so windows open where the client sees them (Sunshine's "deactivate other
//! displays", Apollo's default) — unless the client or the settings keep
//! them, and it takes the client's density either way.
//!
//! The arrangement is applied as a temporary one: mutter keeps it out of
//! `monitors.xml`, and when the virtual monitor goes (the session ending,
//! or the host's D-Bus connection closing with it) the set of monitors is
//! the host's own again, for which mutter applies the arrangement it has
//! stored. So nothing here has to put anything back.

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use ashpd::zbus::{self, Connection, Proxy};
use ashpd::zvariant::{OwnedValue, Value};
use futures_util::StreamExt;

const DISPLAY_CONFIG: &str = "org.gnome.Mutter.DisplayConfig";

/// `ApplyMonitorsConfig`'s method 1: temporary (not stored).
const TEMPORARY: u32 = 1;

/// How long the virtual monitor may take to show up among the monitors:
/// mutter 50 adds it as the session starts, older ones once the stream's
/// format is agreed.
const ARRIVAL_WAIT: Duration = Duration::from_secs(15);

type Props = HashMap<String, OwnedValue>;
/// connector, vendor, product, serial.
type MonitorSpec = (String, String, String, String);
/// id, width, height, refresh, preferred scale, supported scales, properties.
type ModeInfo = (String, i32, i32, f64, f64, Vec<f64>, Props);
type MonitorInfo = (MonitorSpec, Vec<ModeInfo>, Props);
/// x, y, scale, transform, primary, monitors, properties.
type LogicalInfo = (i32, i32, f64, u32, bool, Vec<MonitorSpec>, Props);
type CurrentState = (u32, Vec<MonitorInfo>, Vec<LogicalInfo>, Props);

/// One logical monitor as `ApplyMonitorsConfig` takes it: x, y, scale,
/// transform, primary, and its monitors (connector, mode id).
#[derive(Debug, Clone, PartialEq)]
struct Logical {
    x: i32,
    y: i32,
    scale: f64,
    transform: u32,
    primary: bool,
    monitors: Vec<(String, String)>,
}

/// A monitor's current mode: its id, and the scales it supports.
type CurrentMode = (String, Vec<f64>);

/// The display configuration, as far as the arrangement needs it.
#[derive(Debug, Clone, Default)]
struct Layout {
    /// Each monitor: its connector, and its current mode.
    monitors: Vec<(String, Option<CurrentMode>)>,
    logical: Vec<(i32, i32, f64, u32, bool, Vec<String>)>,
}

impl Layout {
    fn from_state(state: &CurrentState) -> Layout {
        let current = |modes: &[ModeInfo]| {
            modes
                .iter()
                .find(|m| {
                    m.6.get("is-current")
                        .and_then(|v| bool::try_from(v).ok())
                        .unwrap_or(false)
                })
                .map(|m| (m.0.clone(), m.5.clone()))
        };
        Layout {
            monitors: state
                .1
                .iter()
                .map(|(spec, modes, _)| (spec.0.clone(), current(modes)))
                .collect(),
            logical: state
                .2
                .iter()
                .map(|l| {
                    (
                        l.0,
                        l.1,
                        l.2,
                        l.3,
                        l.4,
                        l.5.iter().map(|m| m.0.clone()).collect(),
                    )
                })
                .collect(),
        }
    }

    fn connectors(&self) -> HashSet<String> {
        self.monitors.iter().map(|m| m.0.clone()).collect()
    }
}

/// The arrangement for a session whose virtual monitor is the one monitor
/// of `now` not among `before`: alone (`isolate`) or beside the others as
/// mutter put it, at the supported scale nearest `scale`. None while it has
/// not shown up, or is not in the desktop to arrange.
fn plan(now: &Layout, before: &HashSet<String>, isolate: bool, scale: f64) -> Option<Vec<Logical>> {
    let (ours, mode) = now
        .monitors
        .iter()
        .find(|(c, _)| !before.contains(c))
        .and_then(|(c, m)| Some((c.clone(), m.clone()?)))?;
    let (mode_id, scales) = mode;
    let nearest = scales
        .iter()
        .copied()
        .min_by(|a, b| (a - scale).abs().total_cmp(&(b - scale).abs()))
        .unwrap_or(1.0);
    if isolate {
        return Some(vec![Logical {
            x: 0,
            y: 0,
            scale: nearest,
            transform: 0,
            primary: true,
            monitors: vec![(ours, mode_id)],
        }]);
    }
    let mode_of = |c: &str| {
        now.monitors
            .iter()
            .find(|m| m.0 == c)
            .and_then(|m| m.1.as_ref().map(|m| m.0.clone()))
    };
    let mut placed = false;
    let mut out = Vec::new();
    for (x, y, s, transform, primary, monitors) in &now.logical {
        let has_ours = monitors.contains(&ours);
        placed |= has_ours;
        out.push(Logical {
            x: *x,
            y: *y,
            scale: if has_ours { nearest } else { *s },
            transform: *transform,
            primary: *primary,
            monitors: monitors
                .iter()
                .map(|m| Some((m.clone(), mode_of(m)?)))
                .collect::<Option<_>>()?,
        });
    }
    placed.then_some(out)
}

/// The monitors there are, before the virtual one is asked for.
pub async fn connectors(conn: &Connection) -> zbus::Result<HashSet<String>> {
    Ok(Layout::from_state(&current(conn).await?).connectors())
}

async fn current(conn: &Connection) -> zbus::Result<CurrentState> {
    display_config(conn)
        .await?
        .call("GetCurrentState", &())
        .await
}

async fn display_config(conn: &Connection) -> zbus::Result<Proxy<'static>> {
    Proxy::new(
        conn,
        DISPLAY_CONFIG,
        "/org/gnome/Mutter/DisplayConfig",
        DISPLAY_CONFIG,
    )
    .await
}

/// Arrange the session's virtual monitor once it shows up (see [`plan`]).
pub async fn arrange(
    conn: Connection,
    before: HashSet<String>,
    isolate: bool,
    scale: f64,
) -> Result<(), String> {
    let config = display_config(&conn)
        .await
        .map_err(|e| format!("mutter's display configuration: {e}"))?;
    let mut changed = config
        .receive_signal("MonitorsChanged")
        .await
        .map_err(|e| format!("mutter's display configuration: {e}"))?;
    let deadline = tokio::time::Instant::now() + ARRIVAL_WAIT;
    loop {
        let state = current(&conn)
            .await
            .map_err(|e| format!("mutter's display configuration: {e}"))?;
        if let Some(logical) = plan(&Layout::from_state(&state), &before, isolate, scale) {
            let scale = logical
                .iter()
                .find(|l| l.monitors.iter().any(|m| !before.contains(&m.0)))
                .map_or(1.0, |l| l.scale);
            apply(&config, state.0, &logical).await?;
            tracing::info!(
                isolated = isolate,
                scale,
                "the virtual monitor is arranged in the desktop"
            );
            return Ok(());
        }
        match tokio::time::timeout_at(deadline, changed.next()).await {
            Ok(Some(_)) => continue,
            _ => {
                return Err(format!(
                    "the virtual monitor was not among mutter's monitors within {} s",
                    ARRIVAL_WAIT.as_secs()
                ))
            }
        }
    }
}

async fn apply(config: &Proxy<'_>, serial: u32, logical: &[Logical]) -> Result<(), String> {
    type Monitor<'a> = (&'a str, &'a str, HashMap<&'a str, Value<'a>>);
    let logical: Vec<(i32, i32, f64, u32, bool, Vec<Monitor>)> = logical
        .iter()
        .map(|l| {
            (
                l.x,
                l.y,
                l.scale,
                l.transform,
                l.primary,
                l.monitors
                    .iter()
                    .map(|(c, m)| (c.as_str(), m.as_str(), HashMap::new()))
                    .collect(),
            )
        })
        .collect();
    let props: HashMap<&str, Value> = HashMap::new();
    config
        .call::<_, _, ()>("ApplyMonitorsConfig", &(serial, TEMPORARY, logical, props))
        .await
        .map_err(|e| format!("mutter refused the arrangement: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A desktop with its own monitor, and the session's virtual one to
    /// its right.
    fn layout() -> Layout {
        Layout {
            monitors: vec![
                (
                    "DP-1".into(),
                    Some(("2560x1440@60".into(), vec![1.0, 1.25, 1.5, 2.0])),
                ),
                (
                    "Meta-1".into(),
                    Some(("1512x982@120".into(), vec![1.0, 1.25, 1.75, 2.0])),
                ),
            ],
            logical: vec![
                (0, 0, 1.25, 0, true, vec!["DP-1".into()]),
                (2048, 0, 1.0, 0, false, vec!["Meta-1".into()]),
            ],
        }
    }

    fn before() -> HashSet<String> {
        HashSet::from(["DP-1".to_string()])
    }

    #[test]
    fn an_isolated_virtual_monitor_is_the_whole_desktop() {
        let plan = plan(&layout(), &before(), true, 2.0).unwrap();
        assert_eq!(
            plan,
            [Logical {
                x: 0,
                y: 0,
                scale: 2.0,
                transform: 0,
                primary: true,
                monitors: vec![("Meta-1".into(), "1512x982@120".into())],
            }]
        );
    }

    #[test]
    fn a_kept_desktop_stays_as_mutter_arranged_it() {
        let plan = plan(&layout(), &before(), false, 1.5).unwrap();
        assert_eq!(plan.len(), 2);
        assert_eq!(
            plan[0].scale, 1.25,
            "the host's own monitor keeps its scale"
        );
        assert_eq!(plan[0].monitors, [("DP-1".into(), "2560x1440@60".into())]);
        assert_eq!(
            (plan[1].x, plan[1].scale),
            (2048, 1.25),
            "the virtual one where mutter put it, at the supported scale nearest 1.5"
        );
    }

    #[test]
    fn nothing_is_arranged_before_the_virtual_monitor_shows_up() {
        let mut now = layout();
        now.monitors.truncate(1);
        now.logical.truncate(1);
        assert_eq!(plan(&now, &before(), true, 1.0), None);
    }
}
