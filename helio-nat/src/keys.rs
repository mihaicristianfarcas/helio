//! The keys of a rendezvous record's slot on the DHT, and the sequence
//! number each record carries.
//!
//! Pairing gives each client a 32-byte secret of its own, which the host
//! keeps with that client (`clients.toml`) and the client with that host
//! (`hosts.toml`). Everything else is derived from it with HKDF-SHA256, per
//! role (host, intent, presence) and per UTC day: the Ed25519 key a record
//! is signed with (BEP 44 stores only what verifies) and its BEP 44 salt;
//! and once per secret, the ChaCha20-Poly1305 key records are sealed with
//! (the slot is their associated data). A DHT node sees a key and a
//! salt that change at midnight UTC and are random to it: nothing names the
//! product, the host or the client, and no two days, roles or clients share
//! one. Only the two devices that hold the secret can find a slot, sign it
//! or open what it holds.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use hkdf::Hkdf;
use mainline::SigningKey;
use sha2::Sha256;

use crate::rendezvous::{Kind, Secret};

/// HKDF's salt: separates these keys from any other use of the secret. It
/// never leaves the process, so naming the product here tells no one
/// anything.
const DERIVATION: &[u8] = b"helio rendezvous";

/// A BEP 44 salt's length. Any length up to 64 bytes is valid; the key
/// alone already makes the slot unique, so the salt only has to look as
/// random as the key does.
pub const SALT_LEN: usize = 16;

const DAY_MS: u64 = 86_400_000;

/// The UTC day `ms` (milliseconds since the Unix epoch) falls on, counted
/// from the epoch: what a slot rotates with.
pub fn utc_day(ms: u64) -> u64 {
    ms / DAY_MS
}

/// Where one kind of record lives on the DHT for one day: the key it is
/// signed with and its salt; and the key it is sealed with.
pub struct Slot {
    pub(crate) kind: Kind,
    pub(crate) signing: SigningKey,
    pub(crate) public: [u8; 32],
    pub(crate) salt: [u8; SALT_LEN],
    pub(crate) seal: [u8; 32],
}

impl Slot {
    /// The slot `secret` gives a record of `kind` on UTC day `day`.
    pub fn derive(secret: &Secret, kind: Kind, day: u64) -> Slot {
        let mut info = [0u8; 9];
        info[0] = kind as u8;
        info[1..].copy_from_slice(&day.to_be_bytes());
        let hk = Hkdf::<Sha256>::new(Some(DERIVATION), secret);
        let mut okm = [0u8; 32 + SALT_LEN];
        hk.expand(&info, &mut okm)
            .expect("48 bytes is within HKDF-SHA256's limit of 8160");
        // One sealing key for all of a secret's slots, so that what binds a
        // record to its slot is the associated data, not the key.
        let mut seal = [0u8; 32];
        hk.expand(b"seal", &mut seal)
            .expect("32 bytes is within HKDF-SHA256's limit of 8160");
        let seed: [u8; 32] = okm[..32].try_into().expect("32 bytes");
        let signing = SigningKey::from_bytes(&seed);
        Slot {
            kind,
            public: signing.verifying_key().to_bytes(),
            signing,
            salt: okm[32..].try_into().expect("SALT_LEN bytes"),
            seal,
        }
    }

    /// The public key a DHT node sees.
    pub fn public(&self) -> [u8; 32] {
        self.public
    }

    /// The BEP 44 salt a DHT node sees.
    pub fn salt(&self) -> [u8; SALT_LEN] {
        self.salt
    }

    /// What a sealed record is bound to: this slot's key and salt, so a
    /// record cannot be moved to another slot (another role, day or
    /// client) and still open.
    pub(crate) fn associated_data(&self) -> [u8; 32 + SALT_LEN] {
        let mut ad = [0u8; 32 + SALT_LEN];
        ad[..32].copy_from_slice(&self.public);
        ad[32..].copy_from_slice(&self.salt);
        ad
    }
}

/// The sequence numbers a device's records carry. A DHT node keeps the
/// record with the highest one in a slot and refuses a lower one, so each
/// number is the time in milliseconds or, when the clock stands at or
/// behind the last one (it stepped back), one more than the last: a record
/// is never refused for its clock. The last is kept on disk, so a restart
/// on a clock stepped back does not go back either.
///
/// One counter serves every slot a device writes: increasing across all of
/// them, it increases in each.
pub struct Sequence {
    path: PathBuf,
    write: fn(&Path, &[u8]) -> std::io::Result<()>,
    last: Mutex<u64>,
}

impl Sequence {
    /// The counter kept at `path`. `write` saves the file (readable only by
    /// its owner, as every file in a data folder is).
    pub fn open(path: PathBuf, write: fn(&Path, &[u8]) -> std::io::Result<()>) -> Sequence {
        Sequence {
            path,
            write,
            last: Mutex::new(0),
        }
    }

    fn saved(&self) -> u64 {
        std::fs::read_to_string(&self.path)
            .ok()
            .and_then(|text| {
                text.lines()
                    .filter_map(|l| l.split_once('='))
                    .find(|(k, _)| k.trim() == "seq")
                    .and_then(|(_, v)| v.trim().parse().ok())
            })
            .unwrap_or(0)
    }

    /// The number for a record made at `now_ms`, saved before it is
    /// returned. The file is read again each time: the app's presence
    /// thread and a stream in another process (`helio stream`) both
    /// publish. Two of them racing could each take the same number, but
    /// never for the same slot (one writes presence, the other intents),
    /// and per slot is all a DHT node compares.
    pub fn next(&self, now_ms: u64) -> i64 {
        let mut last = self.last.lock().unwrap_or_else(|e| e.into_inner());
        let n = now_ms.max((*last).max(self.saved()).saturating_add(1));
        let n = n.min(i64::MAX as u64);
        let text = format!(
            "# The last sequence number this device's rendezvous records carried \
                (docs/networking.md).\nseq = {n}\n"
        );
        if let Err(e) = (self.write)(&self.path, text.as_bytes()) {
            // Publishing goes on: the number is only lost to a restart.
            tracing::warn!(path = %self.path.display(), error = %e, "cannot save the rendezvous \
                sequence number");
        }
        *last = n;
        n as i64
    }
}

/// This machine's global IPv6 addresses with `port`: reachable without NAT
/// traversal where the firewall allows (it usually does for replies).
pub fn global_ipv6(port: u16) -> Vec<std::net::SocketAddr> {
    if_addrs::get_if_addrs()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|i| match i.ip() {
            std::net::IpAddr::V6(v6) if (v6.segments()[0] & 0xE000) == 0x2000 => {
                Some(std::net::SocketAddr::new(std::net::IpAddr::V6(v6), port))
            }
            _ => None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rendezvous::random_32;

    #[test]
    fn records_from_different_days_share_no_key_or_salt() {
        let secret = random_32();
        let day = utc_day(crate::rendezvous::now_ms());
        let mut seen_keys = Vec::new();
        let mut seen_salts = Vec::new();
        for kind in [Kind::Host, Kind::Intent, Kind::Presence] {
            for d in [day - 1, day, day + 1] {
                let slot = Slot::derive(&secret, kind, d);
                assert!(!seen_keys.contains(&slot.public), "{kind:?} on day {d}");
                assert!(!seen_salts.contains(&slot.salt), "{kind:?} on day {d}");
                seen_keys.push(slot.public);
                seen_salts.push(slot.salt);
            }
        }
        // Another client's secret: no slot in common either.
        let other = Slot::derive(&random_32(), Kind::Host, day);
        assert!(!seen_keys.contains(&other.public));
        assert!(!seen_salts.contains(&other.salt));
    }

    #[test]
    fn the_same_secret_role_and_day_give_the_same_slot() {
        let secret = random_32();
        let (a, b) = (
            Slot::derive(&secret, Kind::Intent, 20_000),
            Slot::derive(&secret, Kind::Intent, 20_000),
        );
        assert_eq!((a.public, a.salt, a.seal), (b.public, b.salt, b.seal));
    }

    #[test]
    fn nothing_in_a_salt_names_the_product() {
        let slot = Slot::derive(&random_32(), Kind::Host, 20_000);
        for word in [&b"helio"[..], b"rv/"] {
            assert!(!slot.salt.windows(word.len()).any(|w| w == word));
        }
    }

    #[test]
    fn a_sequence_follows_the_clock_and_never_goes_back() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("rendezvous.toml");
        let write = |p: &Path, b: &[u8]| std::fs::write(p, b);
        let seq = Sequence::open(path.clone(), write);
        assert_eq!(seq.next(5_000), 5_000);
        assert_eq!(seq.next(9_000), 9_000, "the clock, when it is ahead");
        assert_eq!(seq.next(1_000), 9_001, "the clock stepped back");
        assert_eq!(seq.next(9_001), 9_002, "the same millisecond twice");
        // A restart, with the clock still behind.
        let again = Sequence::open(path, write);
        assert_eq!(again.next(2_000), 9_003);
    }
}
