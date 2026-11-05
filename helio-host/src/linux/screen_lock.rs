//! Whether a Linux host's screen is locked, for the agent rule that a
//! locked screen is a person's (docs/ai-agents.md, rule 3).
//!
//! Linux has no one lock screen: each desktop locks its own way. What they
//! share is logind's `LockedHint` on the user's graphical session, which a
//! locker sets when it tells logind (GNOME Shell's and KDE Plasma's do; a
//! locker that does not is not seen).
//! The host reads it once and then follows logind's PropertiesChanged
//! signal for it, so an agent session's check, four times a second, is an
//! atomic load and no D-Bus call.
//!
//! The session is the user's graphical one (`Display` on logind's
//! `user/self`): the host runs as a systemd user service, which belongs to
//! no session of its own. Started by hand inside the session, `session/auto`
//! is that session. Where logind cannot be asked (no system bus, no
//! systemd), nothing is known, and the host goes on as if unlocked; the
//! log says so once (docs/platforms/linux.md).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Once;

use ashpd::zbus::{self, Connection, Proxy};
use ashpd::zvariant::OwnedObjectPath;
use futures_util::StreamExt;

const LOGIND: &str = "org.freedesktop.login1";

static LOCKED: AtomicBool = AtomicBool::new(false);

/// Start following the lock, once (on the portal's runtime).
pub fn follow() {
    static START: Once = Once::new();
    START.call_once(|| match crate::portal::runtime() {
        Ok(rt) => {
            rt.spawn(async {
                if let Err(e) = follow_lock().await {
                    LOCKED.store(false, Ordering::Relaxed);
                    tracing::warn!(error = %e, "this host cannot tell when its screen is \
                        locked: an AI agent's input is not held at the lock screen");
                }
            });
        }
        Err(e) => tracing::warn!(error = e, "this host cannot tell when its screen is locked"),
    });
}

/// The screen is locked, as far as logind has been told.
pub fn locked() -> bool {
    follow();
    LOCKED.load(Ordering::Relaxed)
}

async fn follow_lock() -> zbus::Result<()> {
    let conn = Connection::system().await?;
    let session = graphical_session(&conn).await?;
    let proxy = Proxy::new(&conn, LOGIND, session, "org.freedesktop.login1.Session").await?;
    // Listening before reading: a change between the two is not missed.
    let mut changes = proxy.receive_property_changed::<bool>("LockedHint").await;
    let locked: bool = proxy.get_property("LockedHint").await?;
    LOCKED.store(locked, Ordering::Relaxed);
    tracing::info!(locked, "following the screen lock through logind");
    while let Some(change) = changes.next().await {
        let locked = change.get().await?;
        if LOCKED.swap(locked, Ordering::Relaxed) != locked {
            tracing::info!(locked, "the screen lock");
        }
    }
    Err(zbus::Error::Failure("logind's signal stream ended".into()))
}

/// The user's graphical session: logind's `Display` for this user, else
/// the session this process runs in.
async fn graphical_session(conn: &Connection) -> zbus::Result<OwnedObjectPath> {
    let user = Proxy::new(
        conn,
        LOGIND,
        "/org/freedesktop/login1/user/self",
        "org.freedesktop.login1.User",
    )
    .await?;
    let (_, display): (String, OwnedObjectPath) = user.get_property("Display").await?;
    if display.as_str() != "/" {
        return Ok(display);
    }
    Ok(OwnedObjectPath::try_from(
        "/org/freedesktop/login1/session/auto",
    )?)
}
