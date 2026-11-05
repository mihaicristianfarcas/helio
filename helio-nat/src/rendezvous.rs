//! Rendezvous records on the BitTorrent Mainline DHT (BEP 44 mutable items):
//! where a host and one of its paired clients can reach each other, readable
//! by those two alone.
//!
//! Each host-client pair shares a 32-byte secret, which the host makes when
//! the client pairs. Three kinds of record use it, each in a slot of its own:
//! where the host is (`Host`, published by the host, one record per paired
//! client, so removing a client stops its record and nothing else), a client
//! asking to connect now (`Intent`), and a client that is open and may
//! connect (`Presence`). A slot's Ed25519 key and BEP 44 salt are derived
//! from the secret, the kind and the UTC day (`keys::Slot`), so they change
//! at midnight UTC; a reader looks in today's slot and yesterday's at once.
//! A record is sealed with ChaCha20-Poly1305 under a key derived from the
//! secret, with its slot's key and salt as associated data: it opens only in
//! the slot it was published in.
//!
//! What a DHT node still sees: the address a record is published from and
//! looked up from, and when; how many records a host publishes together
//! (one per paired client); and each record's size. A node that sees one
//! address publish into a slot and another look it up learns that the two
//! talk, on that day (docs/networking.md).
//!
//! A record's sequence number is its time in milliseconds unless the clock
//! stepped back (`keys::Sequence`): the newer record always wins.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{ChaCha20Poly1305, Key, Nonce};
use mainline::{Dht, MutableItem};

use crate::keys::{utc_day, Sequence, Slot};

/// BEP 44's limit on a value.
const MAX_VALUE: usize = 1000;
const MAX_ENDPOINTS: usize = 12;

/// The secret one host shares with one client.
pub type Secret = [u8; 32];

pub fn random_32() -> [u8; 32] {
    let mut b = [0u8; 32];
    rand_core::RngCore::fill_bytes(&mut rand_core::OsRng, &mut b);
    b
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// The host: where to find it.
    Host = 0,
    /// A client asking to connect now: where to punch towards.
    Intent = 1,
    /// A client that is open and may connect: keep a path warm towards it.
    Presence = 2,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Record {
    pub kind: Kind,
    /// Milliseconds since the Unix epoch, when published.
    pub at_ms: u64,
    /// Tells two intents from each other.
    pub nonce: u64,
    pub endpoints: Vec<SocketAddr>,
}

pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

impl Record {
    pub fn new(kind: Kind, endpoints: Vec<SocketAddr>) -> Record {
        let nonce = rand_core::RngCore::next_u64(&mut rand_core::OsRng);
        Record {
            kind,
            at_ms: now_ms(),
            nonce,
            endpoints,
        }
    }

    pub fn age(&self) -> Duration {
        Duration::from_millis(now_ms().saturating_sub(self.at_ms))
    }

    fn encode(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(32 + self.endpoints.len() * 19);
        b.push(self.kind as u8);
        b.extend_from_slice(&self.at_ms.to_le_bytes());
        b.extend_from_slice(&self.nonce.to_le_bytes());
        let eps: Vec<&SocketAddr> = self.endpoints.iter().take(MAX_ENDPOINTS).collect();
        b.push(eps.len() as u8);
        for ep in eps {
            match ep.ip() {
                IpAddr::V4(ip) => {
                    b.push(4);
                    b.extend_from_slice(&ip.octets());
                }
                IpAddr::V6(ip) => {
                    b.push(6);
                    b.extend_from_slice(&ip.octets());
                }
            }
            b.extend_from_slice(&ep.port().to_le_bytes());
        }
        b
    }

    fn decode(b: &[u8]) -> Option<Record> {
        let kind = match *b.first()? {
            0 => Kind::Host,
            1 => Kind::Intent,
            2 => Kind::Presence,
            _ => return None,
        };
        let at_ms = u64::from_le_bytes(b.get(1..9)?.try_into().ok()?);
        let nonce = u64::from_le_bytes(b.get(9..17)?.try_into().ok()?);
        let n = *b.get(17)? as usize;
        let mut off = 18;
        let mut endpoints = Vec::with_capacity(n);
        for _ in 0..n.min(MAX_ENDPOINTS) {
            let ip = match *b.get(off)? {
                4 => {
                    let o: [u8; 4] = b.get(off + 1..off + 5)?.try_into().ok()?;
                    off += 5;
                    IpAddr::V4(Ipv4Addr::from(o))
                }
                6 => {
                    let o: [u8; 16] = b.get(off + 1..off + 17)?.try_into().ok()?;
                    off += 17;
                    IpAddr::V6(Ipv6Addr::from(o))
                }
                _ => return None,
            };
            let port = u16::from_le_bytes(b.get(off..off + 2)?.try_into().ok()?);
            off += 2;
            endpoints.push(SocketAddr::new(ip, port));
        }
        Some(Record {
            kind,
            at_ms,
            nonce,
            endpoints,
        })
    }
}

/// Seal a record for `slot`.
pub fn seal(slot: &Slot, record: &Record) -> Vec<u8> {
    let nonce: [u8; 12] = random_32()[..12].try_into().expect("12 bytes");
    let cipher = ChaCha20Poly1305::new(Key::from_slice(&slot.seal));
    let plain = record.encode();
    let body = cipher
        .encrypt(
            Nonce::from_slice(&nonce),
            Payload {
                msg: &plain,
                aad: &slot.associated_data(),
            },
        )
        .expect("in-memory encryption");
    let mut out = nonce.to_vec();
    out.extend_from_slice(&body);
    out
}

/// The record in `sealed`, if it was sealed for `slot` and is of its kind.
pub fn open(slot: &Slot, sealed: &[u8]) -> Option<Record> {
    let (nonce, body) = sealed.split_at_checked(12)?;
    let cipher = ChaCha20Poly1305::new(Key::from_slice(&slot.seal));
    let plain = cipher
        .decrypt(
            Nonce::from_slice(nonce),
            Payload {
                msg: body,
                aad: &slot.associated_data(),
            },
        )
        .ok()?;
    Record::decode(&plain).filter(|r| r.kind == slot.kind)
}

/// A DHT client. Joining the network takes a few seconds; keep one per
/// process.
pub struct Rendezvous {
    dht: Dht,
}

impl Rendezvous {
    /// Join the Mainline DHT (as a client: we answer no one's queries).
    pub fn join() -> std::io::Result<Rendezvous> {
        Ok(Rendezvous {
            dht: Dht::client()?,
        })
    }

    /// Join a private network on this computer's loopback instead, as a
    /// testnet's nodes are (tests). Bound to loopback too: a socket on
    /// every address hears nothing back on a Windows CI runner, whose
    /// firewall lets loopback alone through.
    pub fn join_with(bootstrap: &[String]) -> std::io::Result<Rendezvous> {
        Ok(Rendezvous {
            dht: Dht::builder()
                .bootstrap(bootstrap)
                .bind_address(Ipv4Addr::LOCALHOST)
                .build()?,
        })
    }

    #[allow(deprecated)]
    pub fn is_ready(&self) -> bool {
        self.dht.bootstrapped()
    }

    /// Publish `record` in its slot: the one `secret` gives its kind on the
    /// UTC day of its timestamp, numbered by `seq`. Blocking (a few round
    /// trips: seconds on the Mainline DHT).
    #[allow(deprecated)]
    pub fn publish(&self, secret: &Secret, record: &Record, seq: &Sequence) -> Result<(), String> {
        let slot = Slot::derive(secret, record.kind, utc_day(record.at_ms));
        let value = seal(&slot, record);
        if value.len() > MAX_VALUE {
            return Err("record too large".into());
        }
        let item = MutableItem::new(
            slot.signing.clone(),
            &value,
            seq.next(record.at_ms),
            Some(&slot.salt),
        );
        self.dht
            .put_mutable(item, None)
            .map(|_| ())
            .map_err(|e| e.to_string())
    }

    /// The newest record of `kind` sealed with `secret`. Blocking.
    pub fn resolve(&self, secret: &Secret, kind: Kind) -> Option<Record> {
        self.resolve_at(secret, kind, now_ms())
    }

    /// The newest record of `kind` sealed with `secret`, as of `now_ms`:
    /// from the slots of the days [`days_to_look`] gives, looked up at
    /// once, so the others cost no time. Blocking.
    pub fn resolve_at(&self, secret: &Secret, kind: Kind, now_ms: u64) -> Option<Record> {
        let days = days_to_look(now_ms);
        let found: Vec<(i64, Record)> = std::thread::scope(|s| {
            let others: Vec<_> = days[1..]
                .iter()
                .map(|&day| {
                    std::thread::Builder::new()
                        .name("helio-rendezvous".into())
                        .spawn_scoped(s, move || {
                            self.resolve_slot(&Slot::derive(secret, kind, day))
                        })
                })
                .collect();
            let first = self.resolve_slot(&Slot::derive(secret, kind, days[0]));
            let others = others
                .into_iter()
                .map(|h| h.ok().and_then(|h| h.join().ok()).flatten());
            std::iter::once(first).chain(others).flatten().collect()
        });
        // The higher sequence number is the later publication, whatever
        // the publisher's clock said.
        found
            .into_iter()
            .max_by_key(|(seq, _)| *seq)
            .map(|(_, r)| r)
    }

    #[allow(deprecated)]
    fn resolve_slot(&self, slot: &Slot) -> Option<(i64, Record)> {
        let item = self
            .dht
            .get_mutable_most_recent(&slot.public, Some(&slot.salt))?;
        Some((item.seq(), open(slot, item.value())?))
    }

    /// What a slot holds, unopened: what any DHT node can fetch (tests).
    #[cfg(test)]
    #[allow(deprecated)]
    fn raw(&self, slot: &Slot) -> Option<Vec<u8>> {
        self.dht
            .get_mutable_most_recent(&slot.public, Some(&slot.salt))
            .map(|item| item.value().to_vec())
    }
}

/// How far ahead of the reader's a publisher's clock may be and still be
/// found: as far as an intent may be old (the host's `INTENT_MAX_AGE`).
const CLOCK_AHEAD_MS: u64 = 10 * 60 * 1000;

/// The UTC days whose slots may hold the newest record, as of `now_ms`,
/// today's first: yesterday's too (published before midnight, or by a
/// clock behind the reader's), and in a day's last 10 minutes tomorrow's
/// (by a clock that far ahead, already in it).
fn days_to_look(now_ms: u64) -> Vec<u64> {
    let today = utc_day(now_ms);
    let mut days = vec![today, today.saturating_sub(1)];
    if utc_day(now_ms + CLOCK_AHEAD_MS) != today {
        days.push(today + 1);
    }
    days
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn a_days_last_minutes_look_into_tomorrow_too() {
        let day = 20_000u64;
        let noon = day * 86_400_000 + 12 * 3_600_000;
        assert_eq!(days_to_look(noon), vec![day, day - 1]);
        let late = (day + 1) * 86_400_000 - 5 * 60_000;
        assert_eq!(days_to_look(late), vec![day, day - 1, day + 1]);
    }

    fn record(kind: Kind) -> Record {
        Record::new(
            kind,
            vec![
                "203.0.113.7:47800".parse().unwrap(),
                "[2001:db8::1]:47800".parse().unwrap(),
            ],
        )
    }

    fn sequence() -> (tempfile::TempDir, Sequence) {
        let dir = tempfile::tempdir().unwrap();
        let write = |p: &Path, b: &[u8]| std::fs::write(p, b);
        let seq = Sequence::open(dir.path().join("rendezvous.toml"), write);
        (dir, seq)
    }

    fn testnet() -> mainline::Testnet {
        mainline::Testnet::builder(8)
            .bind_address(Ipv4Addr::LOCALHOST)
            .build()
            .unwrap()
    }

    /// Two devices on `net`, once both found its nodes: a publish before
    /// that finds no one to store with. On a loaded machine (a CI runner,
    /// where it failed so) that takes longer.
    fn joined(net: &mainline::Testnet) -> (Rendezvous, Rendezvous) {
        let a = Rendezvous::join_with(&net.bootstrap).unwrap();
        let b = Rendezvous::join_with(&net.bootstrap).unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        while !(a.is_ready() && b.is_ready()) && std::time::Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
        }
        (a, b)
    }

    /// Noon UTC on a day long past: a time no test runs across midnight
    /// from.
    const NOON: u64 = 20_000 * 86_400_000 + 43_200_000;

    #[test]
    fn records_seal_and_open() {
        let secret = random_32();
        let slot = Slot::derive(&secret, Kind::Host, 20_000);
        let r = record(Kind::Host);
        assert_eq!(open(&slot, &seal(&slot, &r)), Some(r.clone()));
        assert_eq!(
            open(
                &Slot::derive(&random_32(), Kind::Host, 20_000),
                &seal(&slot, &r)
            ),
            None,
            "another client's secret opens nothing"
        );
        let mut tampered = seal(&slot, &r);
        tampered[20] ^= 1;
        assert_eq!(open(&slot, &tampered), None);
    }

    #[test]
    fn a_record_opens_only_in_the_slot_it_was_sealed_for() {
        let secret = random_32();
        let sealed = seal(
            &Slot::derive(&secret, Kind::Intent, 20_000),
            &record(Kind::Intent),
        );
        // Same secret, so the same sealing key: only the associated data
        // (the slot's key and salt) tells these apart.
        for other in [
            Slot::derive(&secret, Kind::Intent, 20_001),
            Slot::derive(&secret, Kind::Presence, 20_000),
            Slot::derive(&secret, Kind::Host, 20_000),
        ] {
            assert_eq!(open(&other, &sealed), None);
        }
    }

    #[test]
    fn a_removed_client_cannot_open_new_host_records() {
        let net = testnet();
        let (host, removed) = joined(&net);
        let (_dir, seq) = sequence();
        let (gone, kept) = (random_32(), random_32());
        host.publish(&gone, &record(Kind::Host), &seq).unwrap();
        host.publish(&kept, &record(Kind::Host), &seq).unwrap();

        // `gone` is removed: the host forgets its secret, and moves.
        let moved = Record::new(Kind::Host, vec!["198.51.100.9:47800".parse().unwrap()]);
        host.publish(&kept, &moved, &seq).unwrap();
        let seen = removed.resolve(&gone, Kind::Host);
        assert_ne!(
            seen.map(|r| r.endpoints),
            Some(moved.endpoints.clone()),
            "its own slot holds the old record at most"
        );

        // The new record, fetched as any DHT node can: no slot the removed
        // client's secret gives opens it.
        let today = utc_day(moved.at_ms);
        let raw = removed
            .raw(&Slot::derive(&kept, Kind::Host, today))
            .unwrap();
        for kind in [Kind::Host, Kind::Intent, Kind::Presence] {
            for day in [today - 1, today, today + 1] {
                assert_eq!(open(&Slot::derive(&gone, kind, day), &raw), None);
            }
        }
        assert_eq!(
            open(&Slot::derive(&kept, Kind::Host, today), &raw),
            Some(moved),
            "the client still paired opens it"
        );
    }

    #[test]
    fn a_clock_stepping_back_still_publishes() {
        let net = testnet();
        let (host, client) = joined(&net);
        let (_dir, seq) = sequence();
        let secret = random_32();
        let first = Record {
            at_ms: NOON,
            ..record(Kind::Host)
        };
        host.publish(&secret, &first, &seq).unwrap();
        // The clock steps back an hour: the next record is stamped earlier.
        let later = Record {
            at_ms: NOON - 3_600_000,
            ..Record::new(Kind::Host, vec!["198.51.100.9:47800".parse().unwrap()])
        };
        host.publish(&secret, &later, &seq).unwrap();
        assert_eq!(client.resolve_at(&secret, Kind::Host, NOON), Some(later));
    }

    #[test]
    fn a_record_crosses_a_dht() {
        let net = testnet();
        let (host, client) = joined(&net);
        let (_dir, seq) = sequence();
        let secret = random_32();

        let first = record(Kind::Host);
        host.publish(&secret, &first, &seq).unwrap();
        assert_eq!(client.resolve(&secret, Kind::Host), Some(first.clone()));

        // A newer record replaces it.
        let second = Record::new(Kind::Host, vec!["10.0.0.1:1".parse().unwrap()]);
        host.publish(&secret, &second, &seq).unwrap();
        assert_eq!(client.resolve(&secret, Kind::Host), Some(second.clone()));

        // The client's records live beside it, each in its own slot.
        let intent = Record::new(Kind::Intent, vec!["10.0.0.2:2".parse().unwrap()]);
        client.publish(&secret, &intent, &seq).unwrap();
        assert_eq!(host.resolve(&secret, Kind::Intent), Some(intent));
        assert_eq!(host.resolve(&secret, Kind::Presence), None);
        assert_eq!(
            client.resolve(&secret, Kind::Host),
            Some(second),
            "the host record is untouched"
        );
        assert_eq!(
            client.resolve(&random_32(), Kind::Host),
            None,
            "nothing under another secret"
        );
    }

    #[test]
    fn yesterdays_record_is_found_and_older_ones_are_not() {
        let net = testnet();
        let (host, client) = joined(&net);
        let (_dir, seq) = sequence();
        let secret = random_32();
        let before_midnight = Record {
            at_ms: NOON + 43_200_000 - 1_000,
            ..record(Kind::Host)
        };
        host.publish(&secret, &before_midnight, &seq).unwrap();
        let after_midnight = NOON + 43_200_000 + 1_000;
        assert_eq!(
            client.resolve_at(&secret, Kind::Host, after_midnight),
            Some(before_midnight)
        );
        assert_eq!(
            client.resolve_at(&secret, Kind::Host, after_midnight + 86_400_000),
            None,
            "the day after, it is two days old"
        );
    }
}
