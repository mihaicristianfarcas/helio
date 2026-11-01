//! Reaching a paired host across the internet (docs/networking.md).
//!
//! While Helio is open, [`presence`] keeps each host's public address cached
//! and tells each host this client is around, so the host keeps its NAT open
//! towards the client's (stable) tunnel port: a click then connects at once.
//! When connecting, [`connect`] also looks the host up afresh and publishes an
//! intent, so the host punches towards us even if the warm path went cold.
//!
//! Each host's records, and this client's for that host, are found and
//! opened with the secret pairing gave this client for that host alone
//! (`helio_nat::keys`): two hosts' records cannot be linked to one client.

use std::net::{SocketAddr, UdpSocket};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use helio_nat::keys::Sequence;
use helio_nat::rendezvous::{Kind, Record, Rendezvous, Secret};
use helio_nat::stun::Stun;
use helio_transport::{Endpoint, Peer};

use crate::store::Hosts;

/// What a client needs to find one host.
#[derive(Clone)]
pub struct Wan {
    pub secret: Secret,
    /// Where to remember what the host's record says (hosts.toml), and the
    /// host's tunnel key there.
    pub dir: PathBuf,
    pub host_x25519: String,
}

/// Leaves the secret out: what is printed may end up in a log.
impl std::fmt::Debug for Wan {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Wan")
            .field("host_x25519", &self.host_x25519)
            .finish_non_exhaustive()
    }
}

impl Wan {
    /// For `host`, if it gave this client a rendezvous secret.
    pub fn for_host(dir: &Path, host: &crate::store::KnownHost) -> Option<Wan> {
        Some(Wan {
            secret: host.rendezvous_secret()?,
            dir: dir.to_path_buf(),
            host_x25519: host.x25519.clone(),
        })
    }

    /// Look the host up and remember where it is. Blocking (DHT round trips).
    fn refresh_host(&self, rv: &Rendezvous) -> Option<Vec<SocketAddr>> {
        let r = rv.resolve(&self.secret, Kind::Host)?;
        if let Ok(true) = Hosts::load(&self.dir).set_wan_addresses(&self.host_x25519, &r.endpoints)
        {
            tracing::info!(endpoints = ?r.endpoints, "the host's internet address changed");
        }
        Some(r.endpoints)
    }
}

/// The sequence numbers of this client's records, kept in its data folder.
fn sequence(dir: &Path) -> Sequence {
    Sequence::open(
        dir.join("rendezvous.toml"),
        helio_transport::identity::write_private,
    )
}

/// One DHT client per process: joining takes seconds, so it is started early
/// ([`warm_up`]) and shared.
fn dht() -> &'static Mutex<Option<Arc<Rendezvous>>> {
    static DHT: OnceLock<Mutex<Option<Arc<Rendezvous>>>> = OnceLock::new();
    DHT.get_or_init(|| Mutex::new(None))
}

fn shared() -> Option<Arc<Rendezvous>> {
    let mut slot = dht().lock().unwrap();
    if slot.is_none() {
        match Rendezvous::join() {
            Ok(r) => *slot = Some(Arc::new(r)),
            Err(e) => tracing::warn!(error = %e, "cannot join the DHT"),
        }
    }
    slot.clone()
}

/// Start joining the DHT now, so a later connection does not wait for it.
pub fn warm_up() {
    std::thread::spawn(|| {
        shared();
    });
}

fn wait_ready(rv: &Rendezvous, stop: &AtomicBool, limit: Duration) -> bool {
    let t = Instant::now();
    while !rv.is_ready() {
        if stop.load(Ordering::Relaxed) || t.elapsed() > limit {
            return false;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    true
}

/// Run alongside a stream's handshake until the tunnel is up (or `stop`).
/// `add_remote` feeds the host's public addresses into the handshake race.
pub fn connect(
    wan: Wan,
    endpoint: Arc<Endpoint>,
    peer: Arc<Peer>,
    stun: Arc<Stun>,
    stop: Arc<AtomicBool>,
    add_remote: impl Fn(SocketAddr) + Send + 'static,
) {
    let started = Instant::now();
    let done = || stop.load(Ordering::Relaxed) || peer.is_established();

    stun.resolve_servers();
    stun.probe(|d, to| {
        let _ = endpoint.send_raw(d, to);
    });
    let Some(rv) = shared() else { return };
    if !wait_ready(&rv, &stop, Duration::from_secs(15)) || done() {
        return;
    }

    // Where the host says it is now (the cached address is already racing).
    let lookup = {
        let (rv, wan, stop) = (rv.clone(), wan.clone(), stop.clone());
        std::thread::spawn(move || {
            if stop.load(Ordering::Relaxed) {
                return;
            }
            match wan.refresh_host(&rv) {
                Some(endpoints) => {
                    tracing::info!(endpoints = ?endpoints, "host found on the DHT");
                    for ep in endpoints {
                        add_remote(ep);
                    }
                }
                None => tracing::info!(
                    "the host has no rendezvous record (is internet access on in Helio Host?)"
                ),
            }
        })
    };

    // Our intent: re-published while the handshake has not landed.
    let port = endpoint.local_port();
    let seq = sequence(&wan.dir);
    let mut last_publish: Option<Instant> = None;
    while !done() && started.elapsed() < Duration::from_secs(60) {
        if last_publish.is_none_or(|t| t.elapsed() >= Duration::from_secs(20)) {
            let mut endpoints = stun.public();
            if endpoints.is_empty() {
                stun.probe(|d, to| {
                    let _ = endpoint.send_raw(d, to);
                });
                std::thread::sleep(Duration::from_millis(500));
                endpoints = stun.public();
            }
            endpoints.extend(helio_nat::keys::global_ipv6(port));
            if !endpoints.is_empty() {
                tracing::info!(endpoints = ?endpoints, "asking the host to punch through");
                if let Err(e) = rv.publish(&wan.secret, &Record::new(Kind::Intent, endpoints), &seq)
                {
                    tracing::debug!(error = %e, "intent publish failed");
                }
                last_publish = Some(Instant::now());
            }
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    let _ = lookup.join();
}

/// Our public addresses for this identity's tunnel port, by STUN from a
/// socket bound to it. None while a stream holds the port, or one starts
/// meanwhile (the stream does its own STUN).
fn public_for_port(dir: &Path) -> Option<Vec<SocketAddr>> {
    // DNS first: a stream starting meanwhile waits for the lock, not for this.
    let stun = Stun::new();
    stun.resolve_servers();
    let _lock = crate::store::TunnelLock::for_borrowing(dir)?;
    let port = crate::store::tunnel_port(dir);
    let socket = UdpSocket::bind(("0.0.0.0", port)).ok()?;
    socket
        .set_read_timeout(Some(Duration::from_millis(50)))
        .ok()?;
    stun.probe(|d, to| {
        let _ = socket.send_to(d, to);
    });
    let mut buf = [0u8; 512];
    let deadline = Instant::now() + Duration::from_millis(1200);
    while Instant::now() < deadline && stun.answers() < 2 {
        if crate::store::stream_waiting() {
            return None;
        }
        if let Ok((n, from)) = socket.recv_from(&mut buf) {
            stun.on_datagram(from, &buf[..n]);
        }
    }
    let mut out = stun.public();
    out.extend(helio_nat::keys::global_ipv6(port));
    Some(out)
}

/// While Helio is open: keep hosts' internet addresses fresh and tell each
/// host we are around, so it keeps a path warm (see the module docs).
pub fn presence(dir: PathBuf) {
    const EVERY: Duration = Duration::from_secs(60);
    const REPUBLISH: Duration = Duration::from_secs(4 * 60);
    std::thread::Builder::new()
        .name("helio-presence".into())
        .spawn(move || {
            let Some(rv) = shared() else { return };
            let never = AtomicBool::new(false);
            if !wait_ready(&rv, &never, Duration::from_secs(30)) {
                return;
            }
            let seq = sequence(&dir);
            // By host: what this client last said, when, and under which
            // secret (paired again, it has a new one, and new slots).
            let mut published: std::collections::HashMap<
                String,
                (Vec<SocketAddr>, Instant, [u8; 32]),
            > = Default::default();
            loop {
                let hosts: Vec<Wan> = Hosts::load(&dir)
                    .list()
                    .iter()
                    .filter_map(|h| Wan::for_host(&dir, h))
                    .collect();
                if !hosts.is_empty() {
                    let ours = public_for_port(&dir);
                    for wan in &hosts {
                        wan.refresh_host(&rv);
                        let Some(endpoints) = ours.clone().filter(|e| !e.is_empty()) else {
                            continue;
                        };
                        let stale = published.get(&wan.host_x25519).is_none_or(|(e, at, s)| {
                            *e != endpoints || at.elapsed() >= REPUBLISH || *s != wan.secret
                        });
                        if stale {
                            let record = Record::new(Kind::Presence, endpoints.clone());
                            if rv.publish(&wan.secret, &record, &seq).is_ok() {
                                published.insert(
                                    wan.host_x25519.clone(),
                                    (endpoints, Instant::now(), wan.secret),
                                );
                            }
                        }
                    }
                }
                std::thread::sleep(EVERY);
            }
        })
        .ok();
}
