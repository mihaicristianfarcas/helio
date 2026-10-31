//! Being reachable from the internet with no port forwarding (docs/networking.md).
//!
//! The host learns the public address its NAT gives the tunnel's socket (STUN,
//! on that socket), publishes it on the Mainline DHT in one sealed rendezvous
//! record per paired client, and watches each client's records:
//!
//! - a client that is open (Helio running) publishes its presence; the host
//!   sends it a small packet every 20 s, which keeps the host's NAT open
//!   towards it, so when the user clicks, the client's handshake walks
//!   straight in;
//! - a client that wants to connect now publishes an intent; the host answers
//!   with handshake initiations towards it at once (the slower path, for
//!   when the warm one did not hold).
//!
//! Each client's records are sealed with its own secret (`helio_nat::keys`),
//! so no client can read another's, and the host publishes one record per
//! client, all at once (a put takes seconds): when its addresses change,
//! every 30 minutes, and when the UTC day turns and the slots move. A
//! removed client is not published for again; its last record expires from
//! the DHT within about two hours.
//!
//! All of it only while internet access is on (`internet_access`): off, the
//! host is not on the DHT at all (it leaves it when the setting goes off),
//! asks no STUN server, and maps no port; it is reachable on the local
//! network alone.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{Duration, Instant};

use helio_nat::keys::utc_day;
use helio_nat::rendezvous::{now_ms, Kind, Record, Rendezvous, Secret};
use helio_transport::PublicIdentity;

use crate::host::Host;

/// NAT mappings for UDP last tens of seconds to minutes: re-probe well inside.
const STUN_EVERY: Duration = Duration::from_secs(25);
/// DHT items expire after about two hours.
const REPUBLISH_EVERY: Duration = Duration::from_secs(30 * 60);
/// Intents older than this are stale.
const INTENT_FRESH: Duration = Duration::from_secs(60);
const IDLE: Duration = Duration::from_secs(5);
/// Keeping a path warm towards a present client.
const WARM_EVERY: Duration = Duration::from_secs(20);
/// A presence record this old means the client has gone.
const PRESENCE_FRESH: Duration = Duration::from_secs(6 * 60);
/// What keeps a path warm: not WireGuard (the tunnel ignores it), not STUN.
const WARM_PACKET: &[u8] = b"\xffhelio-warm";
/// The lease asked of the router for the tunnel's port, renewed at half.
const MAPPING_LEASE: Duration = Duration::from_secs(3600);
/// A router that did not map is asked again this much later.
const MAPPING_RETRY: Duration = Duration::from_secs(15 * 60);

/// A paired client the host publishes for and watches.
struct Watched {
    name: String,
    public: PublicIdentity,
    secret: Secret,
}

/// What the host last published for one client.
struct Published {
    endpoints: Vec<SocketAddr>,
    at: Instant,
    day: u64,
}

/// Whether a client's record is due: never published, the host's addresses
/// changed, it is due for its refresh, or the UTC day turned (its slot moved).
fn due(last: Option<&Published>, endpoints: &[SocketAddr], today: u64) -> bool {
    last.is_none_or(|p| {
        p.endpoints != endpoints || p.at.elapsed() >= REPUBLISH_EVERY || p.day != today
    })
}

/// The paired clients the host can publish for: those with a tunnel key and
/// a rendezvous secret.
fn watched(host: &Host) -> Vec<Watched> {
    host.clients
        .lock()
        .list()
        .iter()
        .filter_map(|c| {
            Some(Watched {
                name: c.name.clone(),
                public: c.public()?,
                secret: c.rendezvous_secret()?,
            })
        })
        .collect()
}

/// Publish the host's addresses for every client whose record is due, all at
/// once: each put takes seconds (docs/networking.md, Measured). True if any
/// was published.
fn publish_due(
    host: &Host,
    rv: &Rendezvous,
    clients: &[Watched],
    published: &mut HashMap<[u8; 32], Published>,
    endpoints: &[SocketAddr],
) -> bool {
    let today = utc_day(now_ms());
    let due: Vec<&Watched> = clients
        .iter()
        .filter(|c| due(published.get(&c.public.x25519), endpoints, today))
        .collect();
    let results: Vec<(&Watched, Result<(), String>)> = std::thread::scope(|s| {
        let running: Vec<_> = due
            .into_iter()
            .map(|c| {
                let record = Record::new(Kind::Host, endpoints.to_vec());
                let put = std::thread::Builder::new()
                    .name("rendezvous-publish".into())
                    .spawn_scoped(s, move || {
                        rv.publish(&c.secret, &record, &host.rendezvous_seq)
                    });
                (c, put)
            })
            .collect();
        running
            .into_iter()
            .map(|(c, put)| {
                let result = match put {
                    Ok(put) => put
                        .join()
                        .unwrap_or_else(|_| Err("the publish panicked".into())),
                    Err(e) => Err(e.to_string()),
                };
                (c, result)
            })
            .collect()
    });
    let mut any = false;
    for (c, result) in results {
        match result {
            Ok(()) => {
                any = true;
                published.insert(
                    c.public.x25519,
                    Published {
                        endpoints: endpoints.to_vec(),
                        at: Instant::now(),
                        day: today,
                    },
                );
            }
            Err(e) => tracing::debug!(client = c.name, error = %e, "rendezvous publish failed"),
        }
    }
    any
}

pub fn run(host: Arc<Host>) {
    // The DHT, joined while internet access is on.
    let mut dht: Option<Rendezvous> = None;
    let port = host.endpoint.local_port();
    // What was published for each client, by its tunnel key.
    let mut published: HashMap<[u8; 32], Published> = HashMap::new();
    // The addresses last said to be reachable, for the log.
    let mut reachable: Vec<SocketAddr> = Vec::new();
    let mut last_probe: Option<Instant> = None;
    let mut seen: HashMap<[u8; 32], u64> = HashMap::new();
    let mut present: HashMap<[u8; 32], (Vec<SocketAddr>, Instant)> = HashMap::new();
    let mut last_warm = Instant::now();
    let mut turn = 0usize;
    let mut mapping = Mapping::default();

    while !host.stopping() {
        let (internet, want_mapping) = {
            let c = host.config.read();
            (c.internet_access, c.port_mapping)
        };
        if !internet {
            mapping.release(&host);
            if dht.take().is_some() {
                tracing::info!("internet access off: left the DHT");
                published.clear();
                reachable.clear();
                present.clear();
            }
            std::thread::sleep(IDLE);
            continue;
        }
        let rv = match &mut dht {
            Some(rv) => rv,
            empty => {
                host.stun.resolve_servers();
                match Rendezvous::join() {
                    Ok(rv) => empty.insert(rv),
                    Err(e) => {
                        tracing::warn!(error = %e, "cannot join the DHT; clients can only \
                            connect from the local network");
                        return;
                    }
                }
            }
        };
        if want_mapping {
            mapping.keep(&host, port);
        } else {
            mapping.release(&host);
        }
        if last_probe.is_none_or(|t| t.elapsed() >= STUN_EVERY) {
            last_probe = Some(Instant::now());
            host.stun.probe(|d, to| {
                let _ = host.endpoint.send_raw(d, to);
            });
            std::thread::sleep(Duration::from_millis(800));
        }
        // Turned off meanwhile: publish nothing more (the top of the loop
        // leaves the DHT).
        if !host.config.read().internet_access {
            continue;
        }

        let mut endpoints = host.stun.public();
        if let Some(m) = &mapping.current {
            if !endpoints.contains(&m.external) {
                endpoints.push(m.external);
            }
        }
        endpoints.extend(helio_nat::keys::global_ipv6(port));
        let clients = watched(&host);
        // A client removed: nothing more is published for it or done
        // towards it.
        let paired = |key: &[u8; 32]| clients.iter().any(|c| c.public.x25519 == *key);
        published.retain(|k, _| paired(k));
        seen.retain(|k, _| paired(k));
        present.retain(|k, _| paired(k));
        if !endpoints.is_empty()
            && publish_due(&host, rv, &clients, &mut published, &endpoints)
            && reachable != endpoints
        {
            tracing::info!(endpoints = ?endpoints, symmetric_nat = host.stun.looks_symmetric(), "reachable from the internet");
            reachable = endpoints;
        }

        // One paired client per round: each look-up is a few DHT round trips.
        if clients.is_empty() {
            std::thread::sleep(IDLE);
            continue;
        }
        let c = &clients[turn % clients.len()];
        turn = turn.wrapping_add(1);
        let key = c.public.x25519;
        let (intent, presence) = std::thread::scope(|s| {
            let presence = std::thread::Builder::new()
                .name("rendezvous-watch".into())
                .spawn_scoped(s, || rv.resolve(&c.secret, Kind::Presence));
            let intent = rv.resolve(&c.secret, Kind::Intent);
            (intent, presence.ok().and_then(|h| h.join().ok()).flatten())
        });
        if let Some(r) = intent {
            if r.age() < INTENT_FRESH && seen.get(&key) != Some(&r.nonce) {
                seen.insert(key, r.nonce);
                punch(&host, c, &r.endpoints);
            }
        }
        if let Some(r) = presence.filter(|r| r.age() < PRESENCE_FRESH) {
            let fresh_until = Instant::now() + PRESENCE_FRESH.saturating_sub(r.age());
            if present.get(&key).is_none_or(|(e, _)| *e != r.endpoints) {
                tracing::info!(client = c.name, endpoints = ?r.endpoints, "keeping a path open to a client");
            }
            present.insert(key, (r.endpoints, fresh_until));
        }

        // Keep the host's NAT open towards every present client not
        // streaming right now.
        if last_warm.elapsed() >= WARM_EVERY {
            last_warm = Instant::now();
            present.retain(|_, (_, until)| *until > Instant::now());
            for (x25519, (endpoints, _)) in &present {
                let busy = host.endpoint.peer_by_key(x25519).is_some_and(|p| {
                    host.endpoint
                        .since_rx(&p)
                        .is_some_and(|d| d < Duration::from_secs(10))
                });
                if !busy {
                    for &ep in endpoints {
                        let _ = host.endpoint.send_raw(WARM_PACKET, ep);
                    }
                }
            }
        }
        std::thread::sleep(Duration::from_millis(300));
    }
    mapping.release(&host);
}

/// The router's forwarding of the tunnel's port, when it grants one.
#[derive(Default)]
struct Mapping {
    current: Option<helio_nat::portmap::Mapping>,
    due: Option<Instant>,
    /// Why the last try failed (said once, not every retry).
    failed: Option<String>,
}

impl Mapping {
    /// Map the port, or renew the mapping, when it is time.
    fn keep(&mut self, host: &Host, port: u16) {
        if self.due.is_some_and(|d| Instant::now() < d) {
            return;
        }
        match self.current.as_mut() {
            Some(m) => match m.renew() {
                Ok(()) => self.due = Some(Instant::now() + renew_after(m.lease)),
                Err(e) => {
                    tracing::warn!(error = e, "the router would not renew the port mapping");
                    self.current = None;
                    self.due = Some(Instant::now() + Duration::from_secs(60));
                }
            },
            None => match helio_nat::portmap::map(port, MAPPING_LEASE) {
                Ok(m) => {
                    tracing::info!(protocol = m.protocol(), external = %m.external, lease_secs = m.lease.as_secs(), "the router forwards the tunnel's port");
                    *host.port_mapping.lock() = format!(
                        "UDP {} forwarded from {} ({})",
                        port,
                        m.external,
                        m.protocol()
                    );
                    self.due = Some(Instant::now() + renew_after(m.lease));
                    self.failed = None;
                    self.current = Some(m);
                }
                Err(e) => {
                    if self.failed.as_ref() != Some(&e) {
                        tracing::info!(
                            reason = e,
                            "no port mapping on the router; hole punching still works"
                        );
                    }
                    *host.port_mapping.lock() =
                        format!("Not forwarded: {}", e.split("; ").next().unwrap_or(&e));
                    self.failed = Some(e);
                    self.due = Some(Instant::now() + MAPPING_RETRY);
                }
            },
        }
    }

    /// Give the port back (internet access or mapping turned off, or Helio Host
    /// stopping).
    fn release(&mut self, host: &Host) {
        if let Some(m) = self.current.take() {
            m.remove();
        }
        self.due = None;
        self.failed = None;
        host.port_mapping.lock().clear();
    }
}

fn renew_after(lease: Duration) -> Duration {
    // A permanent mapping is checked now and then (the router may restart).
    if lease.is_zero() {
        Duration::from_secs(30 * 60)
    } else {
        (lease / 2).max(Duration::from_secs(60))
    }
}

/// Send handshake initiations towards a client that wants to connect, a few
/// times over a second: each opens our NAT towards it.
fn punch(host: &Host, c: &Watched, endpoints: &[SocketAddr]) {
    let Some(peer) = host.endpoint.peer_by_key(&c.public.x25519) else {
        return;
    };
    if peer.is_established()
        && host
            .endpoint
            .since_rx(&peer)
            .is_some_and(|d| d < Duration::from_secs(5))
    {
        return;
    }
    tracing::info!(client = c.name, endpoints = ?endpoints, "client wants to connect from \
        the internet; punching");
    let Ok(packets) = host.endpoint.initiation(&peer) else {
        return;
    };
    for _ in 0..4 {
        for &ep in endpoints {
            let _ = host.endpoint.send_initiation(&peer, &packets, ep);
        }
        std::thread::sleep(Duration::from_millis(250));
        if host.stop_flag().load(Ordering::Relaxed) {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_record_is_due_when_the_addresses_change_or_the_day_turns() {
        let here: Vec<SocketAddr> = vec!["203.0.113.7:47800".parse().unwrap()];
        let moved: Vec<SocketAddr> = vec!["198.51.100.9:47800".parse().unwrap()];
        assert!(due(None, &here, 20_000), "never published");
        let last = Published {
            endpoints: here.clone(),
            at: Instant::now(),
            day: 20_000,
        };
        assert!(!due(Some(&last), &here, 20_000));
        assert!(due(Some(&last), &moved, 20_000), "the host moved");
        assert!(due(Some(&last), &here, 20_001), "a new day: a new slot");
    }
}
