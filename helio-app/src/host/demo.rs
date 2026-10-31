//! The host's part of HELIO_UI_DEMO (see docs/ui.md): a made-up host, or
//! the real one in a made-up state, for checks and screenshots without a
//! host at work; and the host pages' own steps, which the window's demo
//! driver (`app/demo.rs`) runs.

use std::sync::OnceLock;

use gpui::Context;
use helio_proto::permission::{self, Permissions};

use super::api::{Client, LogEntry, Pending, Session, Status};
use super::worker::{Conn, Snapshot};
use super::{now_ms, HostPages};

/// The host a check pretends to have, whoever looks at it (the window and
/// the tray icon alike): the made-up one (`host-sample`), or the real one
/// signed out (`host-signin`), before its account is made (`host-setup`),
/// or not answering (`host-offline`); and a restart it has ahead
/// (`host-restart=now|after_session`).
#[derive(Default)]
struct Pretence {
    sample: bool,
    conn: Option<Conn>,
    restart: Option<String>,
}

fn pretence() -> &'static Pretence {
    static PRETENCE: OnceLock<Pretence> = OnceLock::new();
    PRETENCE.get_or_init(|| {
        let mut p = Pretence::default();
        if let Ok(spec) = std::env::var("HELIO_UI_DEMO") {
            for part in spec.split(',').map(str::trim) {
                match part {
                    "host-sample" => p.sample = true,
                    "host-signin" => p.conn = Some(Conn::SignIn { setup: false }),
                    "host-setup" => p.conn = Some(Conn::SignIn { setup: true }),
                    "host-offline" => p.conn = Some(Conn::NotRunning),
                    _ => {
                        if let Some(when) = part.strip_prefix("host-restart=") {
                            p.restart = Some(when.to_string());
                        }
                    }
                }
            }
        }
        p
    })
}

/// Make `s` what HELIO_UI_DEMO pretends (nothing, when it is not set).
pub fn pretend(s: &mut Snapshot) {
    let p = pretence();
    if p.sample {
        sample(s);
    }
    if let Some(conn) = &p.conn {
        s.conn = conn.clone();
    }
    if let (Some(when), Some(status)) = (&p.restart, s.status.as_mut()) {
        status.restart = Some(when.clone());
    }
}

impl HostPages {
    /// `signin-as=USER:PASSWORD`: type them and sign in.
    pub fn demo_sign_in(&mut self, user: String, password: String, cx: &mut Context<Self>) {
        self.user.update(cx, |f, cx| f.set_text(user, cx));
        self.password.update(cx, |f, cx| f.set_text(password, cx));
        self.sign_in(cx);
    }

    /// `permissions=NAME`: the permissions sheet of the paired device so
    /// called.
    pub fn demo_permissions(&mut self, name: &str, cx: &mut Context<Self>) {
        match self.snap.clients.iter().find(|c| c.name == name) {
            Some(c) => self.edit_permissions(c.key.clone(), cx),
            None => tracing::warn!(name, "no paired device of that name"),
        }
    }
}

/// `host-sample`: a made-up host at work, for screenshots.
fn sample(s: &mut Snapshot) {
    let now = now_ms() / 1000;
    s.conn = Conn::Ready;
    s.state.name = "gaming-pc".into();
    s.state.os = "windows".into();
    let status = s.status.get_or_insert_with(Status::default);
    status.name = "gaming-pc".into();
    if status.id.is_empty() {
        status.id = "7f3a91c2".into();
    }
    status.version = env!("CARGO_PKG_VERSION").into();
    status.port = 47800;
    status.internet = true;
    status.public = vec!["203.0.113.24:47800".into()];
    status.clients = 3;
    status.tunnels = 1;
    // On a Mac, one granted and one not: both of the Hosting page's rows.
    if cfg!(target_os = "macos") {
        status.grants = Some(super::api::Grants {
            screen: true,
            accessibility: false,
        });
    }
    status.session = Some(Session {
        client: "macbook".into(),
        width: 3024,
        height: 1890,
        fps: 120,
        codec: "hevc".into(),
        bitrate_kbps: 100_000,
        started_unix: now - 754,
        encoded_fps: 119.0,
        mbps: 62.4,
        host_latency_ms: 4.8,
        recoveries: 2,
        client_loss_pct: 0.03,
        rtt_ms: 3.1,
        agent: None,
    });
    status.pending = vec![Pending {
        id: 7,
        client_name: "MacBook Air".into(),
        agent: false,
        peer: "192.168.1.23:53112".into(),
        waiting_secs: 14,
        // Devices are paired already: a later one sees only.
        permissions: Permissions::SEE_ONLY,
    }];
    s.clients = vec![
        Client {
            name: "macbook".into(),
            id: "c41d09aa".into(),
            key: "k1".into(),
            paired_at: now - 86_400 * 2,
            online: true,
            agent: false,
            permissions: Permissions::PERSON_ALL,
        },
        Client {
            name: "macbook agent".into(),
            id: "a90b7e13".into(),
            key: "k2".into(),
            paired_at: now - 86_400,
            online: false,
            agent: true,
            permissions: Permissions::AGENT_WATCHED,
        },
        Client {
            name: "living-room".into(),
            id: "5be2f0d7".into(),
            key: "k3".into(),
            paired_at: now - 86_400 * 40,
            online: false,
            agent: false,
            permissions: Permissions::PERSON_CONTROL.with(permission::CLIPBOARD_READ, true),
        },
    ];
    // The made-up host's settings (the host's own defaults), so the
    // settings pages have something to show without a host.
    if s.config.is_none() {
        s.config = Some(serde_json::json!({
            "name": "gaming-pc",
            "port": 47800,
            "pairing_port": 47801,
            "web_port": 47802,
            "nvenc_preset": 1,
            "nvenc_two_pass": true,
            "nvidia_max_power": true,
            "allow_hdr": true,
            "allow_yuv444": true,
            "nvidia_dxgi_present": true,
            "max_bitrate_kbps": 0,
            "max_fps": 0,
            "allow_hevc": true,
            "allow_av1": true,
            "pace_mbps": 800,
            "keep_host_displays": false,
            "allow_takeover": true,
            "adaptive_bitrate": true,
            "internet_access": true,
            "agents": true,
            "agent_local_input_hold_secs": 10,
            "port_mapping": true,
            "clipboard": true,
        }));
    }
    let ms = now_ms();
    s.log = vec![
        LogEntry {
            unix_ms: ms - 184_000,
            client: "macbook agent".into(),
            text: "key Super".into(),
        },
        LogEntry {
            unix_ms: ms - 181_000,
            client: "macbook agent".into(),
            text: "type \"notepad\"".into(),
        },
        LogEntry {
            unix_ms: ms - 176_000,
            client: "macbook agent".into(),
            text: "key Return".into(),
        },
        LogEntry {
            unix_ms: ms - 150_000,
            client: "macbook agent".into(),
            text: "click (212, 188)".into(),
        },
    ];
}
