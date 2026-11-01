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
//! client, eight at once (a put takes seconds): when its addresses change,
//! every 30 minutes, when the UTC day turns and the slots move, and when a
//! client is paired again (a new secret). A put that fails is tried again a
//! minute later, then twice as late each time, up to 30 minutes, so one
//! refused slot does not hold up the others. A removed client is not
//! published for again; its last record expires from the DHT within about
//! two hours.
//!
//! An intent is acted on once, when the host first sees its nonce, not by
//! the time it carries: a client's clock may be minutes off the host's. Its
//! timestamp only rules out one more than 10 minutes old.
//!
//! All of it only while internet access is on (`internet_access`): off, the
//! host is not on the DHT at all (it leaves it when the setting goes off),
//! asks no STUN server, and maps no port; it is reachable on the local
//! network alone.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use helio_nat::keys::utc_day;
use helio_nat::rendezvous::{now_ms, Kind, Record, Rendezvous, Secret};
use helio_transport::{Endpoint, PublicIdentity};

use crate::host::Host;

/// NAT mappings for UDP last tens of seconds to minutes: re-probe well inside.
const STUN_EVERY: Duration = Duration::from_secs(25);
/// DHT items expire after about two hours.
const REPUBLISH_EVERY: Duration = Duration::from_secs(30 * 60);
/// An intent stamped longer ago than this, by its own clock, is ignored
/// (one left on the DHT from a connect long done): a client's clock within
/// this of the host's makes no difference. A client re-publishes its intent
/// every 20 s while it connects, each with a new nonce.
const INTENT_MAX_AGE: Duration = Duration::from_secs(10 * 60);
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
/// A client whose record could not be put is tried again this much later,
/// then twice as late each time, up to `REPUBLISH_EVERY`: a slot that
/// refuses the host's record (a paired client can sign one with a higher
/// sequence number into its own) does not cost a put every round, which
/// would hold up every other client's look-ups.
const PUBLISH_RETRY: Duration = Duration::from_secs(60);
/// Puts at once, at most: each is a thread and an iterative DHT query.
const PUBLISH_AT_ONCE: usize = 8;

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
    /// The secret it was sealed with: a client paired again has a new one,
    /// and its slots with it.
    secret: Secret,
}

/// A client whose last put failed.
struct Retry {
    /// Not tried again before.
    next: Instant,
    wait: Duration,
    /// The host's addresses then: on new ones (back on a network, say), it
    /// is tried at once.
    endpoints: Vec<SocketAddr>,
}

impl Retry {
    /// After a failed put, following `last`'s.
    fn after(last: Option<&Retry>, endpoints: &[SocketAddr]) -> Retry {
        let wait = last.map_or(PUBLISH_RETRY, |r| (r.wait * 2).min(REPUBLISH_EVERY));
        Retry {
            next: Instant::now() + wait,
            wait,
            endpoints: endpoints.to_vec(),
        }
    }

    fn waits(&self, endpoints: &[SocketAddr]) -> bool {
        self.endpoints == endpoints && Instant::now() < self.next
    }
}

/// Whether a client's record is due: never published, the host's addresses
/// changed, it is due for its refresh, the UTC day turned (its slot moved),
/// or the client was paired again (a new secret).
fn due(last: Option<&Published>, endpoints: &[SocketAddr], today: u64, secret: &Secret) -> bool {
    last.is_none_or(|p| {
        p.endpoints != endpoints
            || p.at.elapsed() >= REPUBLISH_EVERY
            || p.day != today
            || p.secret != *secret
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

/// Publish the host's addresses for every client whose record is due, at
/// once (`PUBLISH_AT_ONCE` at a time): each put takes seconds
/// (docs/networking.md, Measured). One that failed waits (`Retry`). True if
/// any was published.
fn publish_due(
    host: &Host,
    rv: &Rendezvous,
    clients: &[Watched],
    published: &mut HashMap<[u8; 32], Published>,
    retries: &mut HashMap<[u8; 32], Retry>,
    endpoints: &[SocketAddr],
) -> bool {
    let today = utc_day(now_ms());
    let due: Vec<&Watched> = clients
        .iter()
        .filter(|c| due(published.get(&c.public.x25519), endpoints, today, &c.secret))
        .filter(|c| {
            !retries
                .get(&c.public.x25519)
                .is_some_and(|r| r.waits(endpoints))
        })
        .collect();
    let mut results: Vec<(&Watched, Result<(), String>)> = Vec::new();
    for chunk in due.chunks(PUBLISH_AT_ONCE) {
        results.extend(publish(host, rv, chunk, endpoints));
    }
    let mut any = false;
    for (c, result) in results {
        let key = c.public.x25519;
        match result {
            Ok(()) => {
                any = true;
                retries.remove(&key);
                published.insert(
                    key,
                    Published {
                        endpoints: endpoints.to_vec(),
                        at: Instant::now(),
                        day: today,
                        secret: c.secret,
                    },
                );
            }
            Err(e) => {
                let retry = Retry::after(retries.get(&key), endpoints);
                tracing::debug!(client = c.name, error = %e, retry_secs = retry.wait.as_secs(),
                    "rendezvous publish failed");
                retries.insert(key, retry);
            }
        }
    }
    any
}

/// Put the host's record for each of `clients`, all at once.
fn publish<'a>(
    host: &Host,
    rv: &Rendezvous,
    clients: &[&'a Watched],
    endpoints: &[SocketAddr],
) -> Vec<(&'a Watched, Result<(), String>)> {
    std::thread::scope(|s| {
        let running: Vec<_> = clients
            .iter()
            .map(|&c| {
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
    })
}

/// The intents the host has acted on: the last nonce from each client.
#[derive(Default)]
struct Intents {
    seen: HashMap<[u8; 32], u64>,
}

impl Intents {
    /// Whether to act on intent `r` from `client`, at `now_ms` by the
    /// host's clock. An intent is fresh when the host first sees its nonce,
    /// wherever the client's clock stands, and is acted on then only; its
    /// own timestamp only rules out one older than `INTENT_MAX_AGE`.
    fn first_sight(&mut self, client: [u8; 32], r: &Record, now_ms: u64) -> bool {
        if now_ms.saturating_sub(r.at_ms) > INTENT_MAX_AGE.as_millis() as u64 {
            return false;
        }
        self.seen.insert(client, r.nonce) != Some(r.nonce)
    }
}

/// Look a client's intent and presence up, both at once.
fn lookup(rv: &Rendezvous, c: &Watched) -> (Option<Record>, Option<Record>) {
    std::thread::scope(|s| {
        let presence = std::thread::Builder::new()
            .name("rendezvous-watch".into())
            .spawn_scoped(s, || rv.resolve(&c.secret, Kind::Presence));
        let intent = rv.resolve(&c.secret, Kind::Intent);
        (intent, presence.ok().and_then(|h| h.join().ok()).flatten())
    })
}

/// Punch towards a client whose intent the host sees for the first time.
/// True if it did.
fn on_intent(
    endpoint: &Endpoint,
    stop: &AtomicBool,
    intents: &mut Intents,
    c: &Watched,
    r: &Record,
    now_ms: u64,
) -> bool {
    if !intents.first_sight(c.public.x25519, r, now_ms) {
        return false;
    }
    punch(endpoint, stop, c, &r.endpoints);
    true
}

pub fn run(host: Arc<Host>) {
    // The DHT, joined while internet access is on.
    let mut dht: Option<Rendezvous> = None;
    let port = host.endpoint.local_port();
    // What was published for each client, by its tunnel key.
    let mut published: HashMap<[u8; 32], Published> = HashMap::new();
    let mut retries: HashMap<[u8; 32], Retry> = HashMap::new();
    // The addresses last said to be reachable, for the log.
    let mut reachable: Vec<SocketAddr> = Vec::new();
    let mut last_probe: Option<Instant> = None;
    let mut intents = Intents::default();
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
                retries.clear();
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
        retries.retain(|k, _| paired(k));
        intents.seen.retain(|k, _| paired(k));
        present.retain(|k, _| paired(k));
        if !endpoints.is_empty()
            && publish_due(
                &host,
                rv,
                &clients,
                &mut published,
                &mut retries,
                &endpoints,
            )
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
        let (intent, presence) = lookup(rv, c);
        if let Some(r) = intent {
            on_intent(
                &host.endpoint,
                host.stop_flag(),
                &mut intents,
                c,
                &r,
                now_ms(),
            );
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
fn punch(endpoint: &Endpoint, stop: &AtomicBool, c: &Watched, endpoints: &[SocketAddr]) {
    let Some(peer) = endpoint.peer_by_key(&c.public.x25519) else {
        return;
    };
    if peer.is_established()
        && endpoint
            .since_rx(&peer)
            .is_some_and(|d| d < Duration::from_secs(5))
    {
        return;
    }
    tracing::info!(client = c.name, endpoints = ?endpoints, "client wants to connect from \
        the internet; punching");
    let Ok(packets) = endpoint.initiation(&peer) else {
        return;
    };
    for _ in 0..4 {
        for &ep in endpoints {
            let _ = endpoint.send_initiation(&peer, &packets, ep);
        }
        std::thread::sleep(Duration::from_millis(250));
        if stop.load(Ordering::Relaxed) {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use helio_nat::keys::Sequence;
    use helio_transport::{Identity, Received};

    #[test]
    fn an_intent_counts_from_when_the_host_first_sees_it() {
        let mut intents = Intents::default();
        let client = [1; 32];
        let now = 20_000 * 86_400_000;
        let intent = |at_ms, nonce| Record {
            kind: Kind::Intent,
            at_ms,
            nonce,
            endpoints: Vec::new(),
        };
        assert!(
            intents.first_sight(client, &intent(now - 5 * 60_000, 1), now),
            "a client's clock five minutes behind"
        );
        assert!(
            !intents.first_sight(client, &intent(now - 5 * 60_000, 1), now + 30_000),
            "acted on once"
        );
        assert!(
            intents.first_sight(client, &intent(now + 5 * 60_000, 2), now),
            "or five minutes ahead"
        );
        assert!(
            !intents.first_sight(client, &intent(now - 11 * 60_000, 3), now),
            "older than ten minutes by its own clock"
        );
    }

    /// Receive (which answers handshakes) and tick, until `stop`.
    fn pump(ep: Arc<Endpoint>, stop: Arc<AtomicBool>) -> std::thread::JoinHandle<()> {
        std::thread::spawn(move || {
            let mut buf = vec![0u8; 65536];
            let mut last_tick = Instant::now();
            while !stop.load(Ordering::Relaxed) {
                let _: Option<Received> = ep.recv(&mut buf).ok();
                if last_tick.elapsed() > Duration::from_millis(100) {
                    ep.tick();
                    last_tick = Instant::now();
                }
            }
        })
    }

    /// The internet path end to end on loopback: a client publishes an
    /// intent on a DHT, the host finds it and punches, and the tunnel comes
    /// up from the host's side, to an address the host learned from the
    /// DHT alone.
    #[test]
    fn an_intent_on_the_dht_gets_the_host_punching_through() {
        let net = mainline::Testnet::builder(8)
            .bind_address(std::net::Ipv4Addr::LOCALHOST)
            .build()
            .unwrap();
        let host_rv = Rendezvous::join_with(&net.bootstrap).unwrap();
        let client_rv = Rendezvous::join_with(&net.bootstrap).unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        while !(host_rv.is_ready() && client_rv.is_ready()) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
        }

        let host = Arc::new(Endpoint::bind(Arc::new(Identity::generate()), 0).unwrap());
        let client = Arc::new(Endpoint::bind(Arc::new(Identity::generate()), 0).unwrap());
        host.add_peer(client.identity().public().clone(), None)
            .unwrap();
        // The client knows the host's key, not where it is.
        client
            .add_peer(host.identity().public().clone(), None)
            .unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let pumps = [
            pump(host.clone(), stop.clone()),
            pump(client.clone(), stop.clone()),
        ];

        let secret = helio_nat::rendezvous::random_32();
        let dir = tempfile::tempdir().unwrap();
        let seq = Sequence::open(dir.path().join("rendezvous.toml"), |p, b| {
            std::fs::write(p, b)
        });
        let from: SocketAddr = format!("127.0.0.1:{}", client.local_port())
            .parse()
            .unwrap();
        client_rv
            .publish(&secret, &Record::new(Kind::Intent, vec![from]), &seq)
            .unwrap();

        let c = Watched {
            name: "mac".into(),
            public: client.identity().public().clone(),
            secret,
        };
        let (intent, presence) = lookup(&host_rv, &c);
        let intent = intent.expect("the host finds the intent");
        assert_eq!(presence, None);
        let mut intents = Intents::default();
        let never = AtomicBool::new(false);
        assert!(on_intent(
            &host,
            &never,
            &mut intents,
            &c,
            &intent,
            now_ms()
        ));
        let to_client = host
            .peer_by_key(&client.identity().public().x25519)
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        while !to_client.is_established() {
            assert!(
                Instant::now() < deadline,
                "the punch did not bring the tunnel up"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(
            !on_intent(&host, &never, &mut intents, &c, &intent, now_ms()),
            "one punch per intent"
        );

        stop.store(true, Ordering::Relaxed);
        for p in pumps {
            p.join().unwrap();
        }
    }

    #[test]
    fn a_record_is_due_when_the_addresses_change_or_the_day_turns() {
        let here: Vec<SocketAddr> = vec!["203.0.113.7:47800".parse().unwrap()];
        let moved: Vec<SocketAddr> = vec!["198.51.100.9:47800".parse().unwrap()];
        let secret = [1; 32];
        assert!(due(None, &here, 20_000, &secret), "never published");
        let last = Published {
            endpoints: here.clone(),
            at: Instant::now(),
            day: 20_000,
            secret,
        };
        assert!(!due(Some(&last), &here, 20_000, &secret));
        assert!(due(Some(&last), &moved, 20_000, &secret), "the host moved");
        assert!(
            due(Some(&last), &here, 20_001, &secret),
            "a new day: a new slot"
        );
        assert!(
            due(Some(&last), &here, 20_000, &[2; 32]),
            "paired again: a new secret, new slots"
        );
    }

    #[test]
    fn a_failed_put_waits_longer_each_time_unless_the_host_moved() {
        let here: Vec<SocketAddr> = vec!["203.0.113.7:47800".parse().unwrap()];
        let moved: Vec<SocketAddr> = vec!["198.51.100.9:47800".parse().unwrap()];
        let first = Retry::after(None, &here);
        assert_eq!(first.wait, PUBLISH_RETRY);
        assert!(first.waits(&here));
        assert!(!first.waits(&moved), "on new addresses, at once");
        let mut r = first;
        for _ in 0..10 {
            r = Retry::after(Some(&r), &here);
        }
        assert_eq!(r.wait, REPUBLISH_EVERY, "never longer than a refresh");
    }
}
