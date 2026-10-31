//! Paired clients. Pairing adds one; Helio's This computer pages and
//! `helio host` list and remove them.
//!
//! A client is a person's device, or an AI agent's identity on one: an agent
//! pairs with a key of its own, so the host knows it by key, not by what it
//! claims, and holds its sessions to the agent rules (`session`): it never
//! takes over a person, and a person always takes over from it.
//!
//! Each client has its permissions here (`helio_proto::permission`,
//! Apollo's client permissions): what it may see, which input it may send,
//! which way the clipboard goes, whether it may start apps, take over or
//! watch agents. They can be changed without unpairing it.
//!
//! An agent a person stopped stays stopped here, across restarts of the
//! host, until a person allows it again or pairs it again.
//!
//! Each client's rendezvous secret is kept here too: the one secret it and
//! the host share, from which every record that finds the host across the
//! internet is derived (`helio_nat::keys`). It is made when the client
//! pairs, or at its first session if it was added without a PIN pairing,
//! and is forgotten with the client: removing it removes the only copy the
//! host has. The file is written readable by its owner alone.
//!
//! A file from before permissions has none written: a person's device then
//! keeps everything it could do, and an agent what its `access` said. The
//! next save writes permissions in their place.

use std::path::{Path, PathBuf};

use helio_proto::permission::Permissions;
use helio_transport::PublicIdentity;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(from = "Stored", into = "Stored")]
pub struct Client {
    pub name: String,
    pub x25519: String,
    pub mlkem: String,
    /// Seconds since the Unix epoch.
    pub paired_at: u64,
    /// The client's rendezvous secret (hex), for connecting from the
    /// internet. Absent until it is made (`Clients::rendezvous_secret_or`).
    pub rendezvous_secret: Option<String>,
    /// An AI agent's identity (paired as one).
    pub agent: bool,
    /// What it may do here; only what its kind can be allowed
    /// (`Permissions::fit`).
    pub permissions: Permissions,
    /// An agent a person stopped: its sessions are refused until a person
    /// allows it again. Kept here, not in the host's memory, so that a
    /// restart of the host does not let it back.
    pub stopped: bool,
}

/// A client as `clients.toml` holds it.
#[derive(Serialize, Deserialize)]
struct Stored {
    name: String,
    x25519: String,
    mlkem: String,
    paired_at: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    rendezvous_secret: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    agent: bool,
    /// Absent from files written before permissions.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    permissions: Option<Permissions>,
    /// An agent's permissions in files written before them: read, never
    /// written.
    #[serde(default, skip_serializing)]
    access: Option<Access>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    stopped: bool,
}

impl From<Stored> for Client {
    fn from(s: Stored) -> Client {
        let permissions = match (s.permissions, s.agent) {
            (Some(p), agent) => p.fit(agent),
            (None, false) => Permissions::PERSON_ALL,
            (None, true) => s.access.unwrap_or_default().permissions(),
        };
        Client {
            name: s.name,
            x25519: s.x25519,
            mlkem: s.mlkem,
            paired_at: s.paired_at,
            rendezvous_secret: s.rendezvous_secret,
            agent: s.agent,
            permissions,
            stopped: s.stopped,
        }
    }
}

impl From<Client> for Stored {
    fn from(c: Client) -> Stored {
        Stored {
            name: c.name,
            x25519: c.x25519,
            mlkem: c.mlkem,
            paired_at: c.paired_at,
            rendezvous_secret: c.rendezvous_secret,
            agent: c.agent,
            permissions: Some(c.permissions),
            access: None,
            stopped: c.stopped,
        }
    }
}

/// What an agent could do before permissions, in three steps.
#[derive(Debug, Clone, Copy, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
enum Access {
    /// See the screen and use the keyboard and mouse.
    #[default]
    Control,
    /// See the screen only.
    View,
    /// Nothing: its sessions are refused.
    Off,
}

impl Access {
    fn permissions(self) -> Permissions {
        match self {
            Access::Control => Permissions::AGENT_ALL,
            Access::View => Permissions::SEE_ONLY,
            Access::Off => Permissions::NONE,
        }
    }
}

/// Who a peer is, as far as sessions go.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Person,
    Agent,
}

impl Client {
    pub fn public(&self) -> Option<PublicIdentity> {
        PublicIdentity::from_b64(&self.x25519, &self.mlkem).ok()
    }

    pub fn rendezvous_secret(&self) -> Option<[u8; 32]> {
        let h = self.rendezvous_secret.as_deref()?;
        let bytes: Vec<u8> = (0..h.len())
            .step_by(2)
            .filter_map(|i| u8::from_str_radix(h.get(i..i + 2)?, 16).ok())
            .collect();
        bytes.try_into().ok()
    }
}

fn hex(bytes: [u8; 32]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct File {
    #[serde(default, rename = "client")]
    clients: Vec<Client>,
}

/// The paired clients, as `clients.toml` held them when last read.
///
/// Two programs change the file: the running host (pairing, This
/// computer, a client's rendezvous secret) and `helio host` (add-client,
/// remove-client, permissions). Each change re-reads the file under a lock
/// shared by both (`clients.lock`), so neither saves a stale copy over the
/// other's: a device removed from the command line is not brought back by
/// the running host's next save, which that very device could otherwise
/// trigger.
pub struct Clients {
    path: PathBuf,
    list: Vec<Client>,
}

impl Clients {
    pub fn load(dir: &Path) -> Clients {
        let path = dir.join("clients.toml");
        let list = read(&path).unwrap_or_else(|e| {
            tracing::error!(path = %path.display(), error = e, "the paired clients could not be \
                read: Helio Host goes on with none, and leaves the file as it is");
            Vec::new()
        });
        Clients { path, list }
    }

    pub fn list(&self) -> &[Client] {
        &self.list
    }

    /// Re-read the file under its lock, apply `f` (which says whether it
    /// changed anything, and what to return), and save if it did. Refused
    /// when the file cannot be read: saving would replace every pairing in
    /// it with the few made since.
    fn modify<T>(&mut self, f: impl FnOnce(&mut Vec<Client>) -> (bool, T)) -> std::io::Result<T> {
        let _lock = self.lock()?;
        self.list = read(&self.path).map_err(|e| {
            std::io::Error::other(format!(
                "Helio Host could not read {} ({e}) and will not save over it. Fix the file or \
                    move it away, then try again.",
                self.path.display()
            ))
        })?;
        let (changed, out) = f(&mut self.list);
        if changed {
            self.save()?;
        }
        Ok(out)
    }

    /// The lock every change to the file holds, across processes: until
    /// the file it returns is dropped.
    fn lock(&self) -> std::io::Result<std::fs::File> {
        let dir = self.path.parent().unwrap_or(Path::new("."));
        std::fs::create_dir_all(dir)?;
        let file = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(dir.join("clients.lock"))?;
        file.lock()?;
        Ok(file)
    }

    fn save(&self) -> std::io::Result<()> {
        let text = toml::to_string_pretty(&File {
            clients: self.list.clone(),
        })
        .map_err(std::io::Error::other)?;
        helio_transport::identity::write_private(&self.path, text.as_bytes())
    }

    /// What a newly paired client gets, unless the person pairing it
    /// chooses (`Permissions::on_pairing`).
    pub fn default_permissions(&self, agent: bool) -> Permissions {
        let people = self.list.iter().filter(|c| !c.agent).count();
        Permissions::on_pairing(agent, people)
    }

    /// Add (or rename, if the key is already known) a client. `agent`: the
    /// identity is an AI agent's. `permissions`: what it may do, or the
    /// default (`default_permissions`, counted before it is added).
    /// `rendezvous_secret`: the one pairing gave it (None: made at its
    /// first session).
    pub fn add(
        &mut self,
        name: &str,
        public: &PublicIdentity,
        rendezvous_secret: Option<[u8; 32]>,
        agent: bool,
        permissions: Option<Permissions>,
    ) -> std::io::Result<Client> {
        let (x, m) = public.to_b64();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let rendezvous_secret = rendezvous_secret.map(hex);
        self.modify(|list| {
            // Pairing again replaces the entry: it does not count as a
            // person already here when the default is worked out.
            let others = list.iter().filter(|c| c.x25519 != x && !c.agent);
            let permissions = permissions
                .unwrap_or_else(|| Permissions::on_pairing(agent, others.count()))
                .fit(agent);
            let client = Client {
                name: name.to_string(),
                x25519: x.clone(),
                mlkem: m,
                paired_at: now,
                rendezvous_secret,
                agent,
                permissions,
                // Paired again, by a person who entered its PIN: a new start.
                stopped: false,
            };
            match list.iter_mut().find(|c| c.x25519 == x) {
                Some(existing) => *existing = client.clone(),
                None => list.push(client.clone()),
            }
            (true, client)
        })
    }

    /// A device a person paired with its PIN: what they chose, else what
    /// the prompt showed them (`default_permissions`). The prompt cannot
    /// know the device's key, which arrives sealed after the PIN, so a
    /// device pairing again counts there as a person already here; it gets
    /// that (`add`, which `helio host add-client` uses, leaves it out of
    /// the count). The default is worked out again as the PIN is typed, as
    /// the prompt shows it each time it asks: it differs from what was on
    /// screen only if a person's device was removed in the second between.
    pub fn pair(
        &mut self,
        name: &str,
        public: &PublicIdentity,
        rendezvous_secret: Option<[u8; 32]>,
        agent: bool,
        chosen: Option<Permissions>,
    ) -> std::io::Result<Client> {
        let shown = self.default_permissions(agent);
        self.add(
            name,
            public,
            rendezvous_secret,
            agent,
            Some(chosen.unwrap_or(shown)),
        )
    }

    /// A paired client's rendezvous secret: the one it has, else `fresh`,
    /// kept as its own from now on. None for a key that is not paired.
    ///
    /// Asked at every session's start, on the host's receive thread: a
    /// secret already kept is answered from this copy, without touching the
    /// file. One is made under the file's lock, from the file, so a client
    /// removed beside the running host (`helio host remove-client`) is
    /// given none.
    pub fn rendezvous_secret_or(
        &mut self,
        x25519_b64: &str,
        fresh: [u8; 32],
    ) -> std::io::Result<Option<[u8; 32]>> {
        match self.list.iter().find(|c| c.x25519 == x25519_b64) {
            None => return Ok(None),
            Some(c) => {
                if let Some(secret) = c.rendezvous_secret() {
                    return Ok(Some(secret));
                }
            }
        }
        self.modify(|list| {
            let Some(c) = list.iter_mut().find(|c| c.x25519 == x25519_b64) else {
                return (false, None);
            };
            if let Some(secret) = c.rendezvous_secret() {
                return (false, Some(secret));
            }
            c.rendezvous_secret = Some(hex(fresh));
            (true, Some(fresh))
        })
    }

    pub fn remove(&mut self, x25519_b64: &str) -> std::io::Result<Option<Client>> {
        self.modify(
            |list| match list.iter().position(|c| c.x25519 == x25519_b64) {
                Some(i) => (true, Some(list.remove(i))),
                None => (false, None),
            },
        )
    }

    /// Set what a client may do (only what its kind can be allowed). The
    /// permissions it now has, or None if there is no such client.
    pub fn set_permissions(
        &mut self,
        x25519_b64: &str,
        permissions: Permissions,
    ) -> std::io::Result<Option<Permissions>> {
        self.modify(|list| {
            let Some(c) = list.iter_mut().find(|c| c.x25519 == x25519_b64) else {
                return (false, None);
            };
            c.permissions = permissions.fit(c.agent);
            (true, Some(c.permissions))
        })
    }

    /// Stop an agent (its sessions are refused), or allow it again. True if
    /// that changed anything.
    pub fn set_stopped(&mut self, x25519_b64: &str, stopped: bool) -> std::io::Result<bool> {
        let apply = |list: &mut Vec<Client>| {
            let Some(c) = list.iter_mut().find(|c| c.x25519 == x25519_b64) else {
                return (false, false);
            };
            if c.stopped == stopped || (stopped && !c.agent) {
                return (false, false);
            }
            c.stopped = stopped;
            (true, true)
        };
        self.modify(apply).inspect_err(|_| {
            // Not saved (the file could not be locked, read or written):
            // this list holds it all the same, until the host starts again.
            apply(&mut self.list);
        })
    }

    /// A person stopped the agent with this key, and has not allowed it
    /// again.
    pub fn is_stopped(&self, key: &[u8; 32]) -> bool {
        self.by_key(key).is_some_and(|c| c.stopped)
    }

    fn by_key(&self, key: &[u8; 32]) -> Option<&Client> {
        self.list
            .iter()
            .find(|c| c.public().is_some_and(|p| &p.x25519 == key))
    }

    /// Who the client with this key is, and what it may do. A key that is
    /// not (or no longer) paired may do nothing.
    pub fn role_of(&self, key: &[u8; 32]) -> (Role, Permissions) {
        match self.by_key(key) {
            Some(c) if c.agent => (Role::Agent, c.permissions),
            Some(c) => (Role::Person, c.permissions),
            None => (Role::Person, Permissions::NONE),
        }
    }

    pub fn name_of(&self, key: &[u8; 32]) -> Option<&str> {
        self.by_key(key).map(|c| c.name.as_str())
    }
}

/// The clients in `path`: none if there is no file yet. Anything else that
/// is not a list of clients (a damaged file, one a newer Helio Host wrote) is an
/// error, so that it is not taken for an empty list and saved over.
fn read(path: &Path) -> Result<Vec<Client>, String> {
    match std::fs::read_to_string(path) {
        Ok(text) => toml::from_str::<File>(&text)
            .map(|f| f.clients)
            .map_err(|e| e.to_string().lines().next().unwrap_or_default().to_string()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(e) => Err(e.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use helio_proto::permission::{KEYBOARD, MOUSE, UNWATCHED, VIEW};
    use helio_transport::Identity;

    #[test]
    fn clients_persist_and_are_keyed_by_public_key() {
        let dir = tempfile::tempdir().unwrap();
        let id = Identity::generate();
        let mut c = Clients::load(dir.path());
        c.add("mac", id.public(), None, false, None).unwrap();
        c.add("macbook", id.public(), None, false, None).unwrap();
        let c = Clients::load(dir.path());
        assert_eq!(c.list().len(), 1);
        assert_eq!(c.name_of(&id.public().x25519), Some("macbook"));
        assert_eq!(c.list()[0].public().as_ref(), Some(id.public()));
        // Pairing again is not a second person: still the first one's.
        assert_eq!(
            c.role_of(&id.public().x25519),
            (Role::Person, Permissions::PERSON_ALL)
        );
        // A key that is not paired may do nothing.
        let stranger = Identity::generate();
        assert_eq!(c.role_of(&stranger.public().x25519).1, Permissions::NONE);
    }

    #[test]
    fn the_first_person_gets_everything_later_ones_see_only_unless_chosen() {
        let dir = tempfile::tempdir().unwrap();
        let (a, b, c, agent) = (
            Identity::generate(),
            Identity::generate(),
            Identity::generate(),
            Identity::generate(),
        );
        let mut list = Clients::load(dir.path());
        assert_eq!(list.default_permissions(false), Permissions::PERSON_ALL);
        list.add("agent", agent.public(), None, true, None).unwrap();
        // Agents do not count.
        assert_eq!(list.default_permissions(false), Permissions::PERSON_ALL);
        list.add("mac", a.public(), None, false, None).unwrap();
        assert_eq!(list.default_permissions(false), Permissions::SEE_ONLY);
        list.add("tv", b.public(), None, false, None).unwrap();
        list.add(
            "kid",
            c.public(),
            None,
            false,
            Some(Permissions::PERSON_CONTROL),
        )
        .unwrap();
        let list = Clients::load(dir.path());
        let of = |id: &Identity| list.role_of(&id.public().x25519);
        assert_eq!(of(&a), (Role::Person, Permissions::PERSON_ALL));
        assert_eq!(of(&b), (Role::Person, Permissions::SEE_ONLY));
        assert_eq!(of(&c), (Role::Person, Permissions::PERSON_CONTROL));
        assert_eq!(of(&agent), (Role::Agent, Permissions::AGENT_ALL));
    }

    #[test]
    fn an_agents_permissions_persist_and_fit_an_agent() {
        let dir = tempfile::tempdir().unwrap();
        let agent = Identity::generate();
        let mut c = Clients::load(dir.path());
        c.add("mac agent", agent.public(), None, true, None)
            .unwrap();
        let (x, _) = agent.public().to_b64();
        // Only while watched.
        let watched = Permissions::from_bits(VIEW | KEYBOARD | MOUSE);
        assert_eq!(c.set_permissions(&x, watched).unwrap(), Some(watched));
        let text = std::fs::read_to_string(dir.path().join("clients.toml")).unwrap();
        assert!(!text.contains("access"), "{text}");
        let c = Clients::load(dir.path());
        assert_eq!(c.role_of(&agent.public().x25519), (Role::Agent, watched));
        // What a person's device could have means nothing for an agent.
        let mut c = c;
        let all = Permissions::PERSON_ALL.with(UNWATCHED, true);
        assert_eq!(
            c.set_permissions(&x, all).unwrap(),
            Some(Permissions::AGENT_ALL)
        );
        assert_eq!(c.set_permissions("nobody", all).unwrap(), None);
    }

    /// The running host's list and `helio host`'s, on one folder: a change
    /// one makes is not undone by the other's next save, whoever triggers it
    /// (here the removed device itself, asking for its rendezvous secret).
    #[test]
    fn a_change_made_beside_the_running_host_is_not_saved_over() {
        let dir = tempfile::tempdir().unwrap();
        let (gone, narrowed, kept) = (
            Identity::generate(),
            Identity::generate(),
            Identity::generate(),
        );
        let mut host = Clients::load(dir.path());
        host.add("gone", gone.public(), None, false, None).unwrap();
        host.add("narrowed", narrowed.public(), None, false, None)
            .unwrap();
        host.add("kept", kept.public(), None, false, None).unwrap();
        let b64 = |id: &Identity| id.public().to_b64().0;

        let mut cli = Clients::load(dir.path());
        cli.remove(&b64(&gone)).unwrap();
        cli.set_permissions(&b64(&narrowed), Permissions::NONE)
            .unwrap();

        // The host still has its own copy; the removed device asks for its
        // rendezvous secret, and a change the host makes saves.
        assert_eq!(
            host.rendezvous_secret_or(&b64(&gone), [7; 32]).unwrap(),
            None,
            "a removed device gets no secret"
        );
        assert_eq!(
            host.rendezvous_secret_or(&b64(&kept), [8; 32]).unwrap(),
            Some([8; 32])
        );
        let file = Clients::load(dir.path());
        assert_eq!(
            file.role_of(&gone.public().x25519).1,
            Permissions::NONE,
            "the removed device came back"
        );
        assert_eq!(file.role_of(&narrowed.public().x25519).1, Permissions::NONE);
        assert_eq!(file.list().len(), 2);
        assert!(file.list().iter().any(|c| c.rendezvous_secret.is_some()));
    }

    #[test]
    fn a_client_without_a_rendezvous_secret_is_given_one_once() {
        let dir = tempfile::tempdir().unwrap();
        let mac = Identity::generate();
        let x = mac.public().to_b64().0;
        let mut c = Clients::load(dir.path());
        c.add("mac", mac.public(), None, false, None).unwrap();
        assert_eq!(c.rendezvous_secret_or(&x, [1; 32]).unwrap(), Some([1; 32]));
        assert_eq!(
            c.rendezvous_secret_or(&x, [2; 32]).unwrap(),
            Some([1; 32]),
            "the one it has stays"
        );
        assert_eq!(
            Clients::load(dir.path()).list()[0].rendezvous_secret(),
            Some([1; 32])
        );
    }

    #[test]
    fn a_removed_clients_rendezvous_secret_is_forgotten() {
        let dir = tempfile::tempdir().unwrap();
        let mac = Identity::generate();
        let x = mac.public().to_b64().0;
        let file = dir.path().join("clients.toml");
        let mut c = Clients::load(dir.path());
        c.add("mac", mac.public(), Some([5; 32]), false, None)
            .unwrap();
        assert!(std::fs::read_to_string(&file)
            .unwrap()
            .contains(&"05".repeat(32)));
        c.remove(&x).unwrap();
        assert!(!std::fs::read_to_string(&file)
            .unwrap()
            .contains(&"05".repeat(32)));
        assert_eq!(
            c.rendezvous_secret_or(&x, [6; 32]).unwrap(),
            None,
            "and none is made for it again"
        );
    }

    #[test]
    fn a_device_paired_again_gets_what_the_prompt_showed() {
        let dir = tempfile::tempdir().unwrap();
        let mac = Identity::generate();
        let mut c = Clients::load(dir.path());
        c.add("mac", mac.public(), None, false, None).unwrap();
        let (x, _) = mac.public().to_b64();
        c.set_permissions(&x, Permissions::SEE_ONLY).unwrap();
        // It pairs again, and the PIN is typed with the prompt's default
        // left as it is: what the prompt showed is what it gets.
        let shown = c.default_permissions(false);
        let paired = c.pair("mac", mac.public(), None, false, None).unwrap();
        assert_eq!(paired.permissions, shown);
        assert_eq!(paired.permissions, Permissions::SEE_ONLY);
    }

    #[test]
    fn a_stopped_agent_stays_stopped_across_a_restart_until_allowed_again() {
        let dir = tempfile::tempdir().unwrap();
        let (agent, person) = (Identity::generate(), Identity::generate());
        let mut c = Clients::load(dir.path());
        c.add("bot", agent.public(), None, true, None).unwrap();
        c.add("mac", person.public(), None, false, None).unwrap();
        let (x, _) = agent.public().to_b64();
        assert!(c.set_stopped(&x, true).unwrap());
        // The host starts again: still stopped.
        let mut c = Clients::load(dir.path());
        assert!(c.is_stopped(&agent.public().x25519));
        assert!(c.set_stopped(&x, false).unwrap());
        assert!(!Clients::load(dir.path()).is_stopped(&agent.public().x25519));
        // Only agents are stopped.
        let (p, _) = person.public().to_b64();
        assert!(!c.set_stopped(&p, true).unwrap());
        // Paired again (a person entered its PIN): a new start.
        c.set_stopped(&x, true).unwrap();
        c.add("bot", agent.public(), None, true, None).unwrap();
        assert!(!c.is_stopped(&agent.public().x25519));
    }

    #[test]
    fn a_stop_that_cannot_be_saved_holds_until_the_host_restarts() {
        let dir = tempfile::tempdir().unwrap();
        let agent = Identity::generate();
        let mut c = Clients::load(dir.path());
        c.add("bot", agent.public(), None, true, None).unwrap();
        // The file broken beside the running host (a hand edit).
        std::fs::write(dir.path().join("clients.toml"), "not toml [").unwrap();
        let (x, _) = agent.public().to_b64();
        assert!(c.set_stopped(&x, true).is_err());
        assert!(c.is_stopped(&agent.public().x25519));
    }

    #[test]
    fn a_file_from_before_permissions_keeps_what_each_client_could_do() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("clients.toml"),
            "[[client]]\nname = \"old\"\nx25519 = \"a\"\nmlkem = \"b\"\npaired_at = 1\n\n\
             [[client]]\nname = \"bot\"\nx25519 = \"c\"\nmlkem = \"d\"\npaired_at = 1\nagent = true\n\n\
             [[client]]\nname = \"viewer\"\nx25519 = \"e\"\nmlkem = \"f\"\npaired_at = 1\nagent = true\n\
             access = \"view\"\n\n\
             [[client]]\nname = \"gone\"\nx25519 = \"g\"\nmlkem = \"h\"\npaired_at = 1\nagent = true\n\
             access = \"off\"\n\n\
             [[client]]\nname = \"new\"\nx25519 = \"i\"\nmlkem = \"j\"\npaired_at = 1\n\
             permissions = [\"view\", \"mouse\", \"fly\"]\n",
        )
        .unwrap();
        let c = Clients::load(dir.path());
        let got: Vec<_> = c.list().iter().map(|c| (c.agent, c.permissions)).collect();
        assert_eq!(
            got,
            vec![
                (false, Permissions::PERSON_ALL),
                (true, Permissions::AGENT_ALL),
                (true, Permissions::SEE_ONLY),
                (true, Permissions::NONE),
                (false, Permissions::from_bits(VIEW | MOUSE)),
            ]
        );
    }

    #[test]
    fn a_clients_file_that_cannot_be_read_is_left_as_it_is() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("clients.toml");
        let damaged = "[[client]]\nname = \"mac\"\nx25519 = ";
        std::fs::write(&path, damaged).unwrap();
        let mut c = Clients::load(dir.path());
        assert!(c.list().is_empty());
        let refused = c
            .add("new", Identity::generate().public(), None, false, None)
            .unwrap_err();
        assert!(
            refused.to_string().contains("will not save over it"),
            "{refused}"
        );
        assert_eq!(std::fs::read_to_string(&path).unwrap(), damaged);
        // No file yet is no clients yet, and saving works.
        std::fs::remove_file(&path).unwrap();
        let mut c = Clients::load(dir.path());
        c.add("new", Identity::generate().public(), None, false, None)
            .unwrap();
        assert_eq!(Clients::load(dir.path()).list().len(), 1);
    }
}
