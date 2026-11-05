//! The release's own signature on a package, checked before an update is
//! unpacked or run, on every system.
//!
//! GitHub's SHA-256 of a release file says the file is the one that was
//! uploaded, not who uploaded it; on Linux a package carries nothing else.
//! So the release workflow signs every package with one Ed25519 key that
//! only its `sign` job can use (a secret of the `release` environment, on
//! `main`), and Helio is built with the public half
//! (`helio-update/release-signing.pub`, or `HELIO_RELEASE_KEY` at build
//! time: see build.rs). The signature's trusted comment, signed too, names
//! the package's file (`file:Helio-1.0.1-linux-x86_64.tar.gz`): product,
//! version, system and architecture, so an older package, or another
//! system's, cannot pass for the one offered.
//!
//! The format is minisign's, so anyone can check a download by hand
//! (`minisign -Vm FILE -p release-signing.pub`). A public key is base64 of
//! `Ed`, an 8-byte key id and the 32-byte key. A signature file is four
//! lines: an untrusted comment; base64 of `ED`, the key id and an Ed25519
//! signature of the file's BLAKE2b-512; `trusted comment: ` and the
//! comment; base64 of an Ed25519 signature of the first signature followed
//! by the comment. The older `Ed` form, which signs the file itself, is
//! refused: minisign has not made it by default for years, and the release
//! never does.

use std::io::Read;
use std::path::Path;

use base64::Engine;
use blake2::{Blake2b512, Digest};
use ed25519_dalek::{Signature, VerifyingKey};

/// The longest signature file taken. minisign's, with a trusted comment
/// naming one package, is under 400 bytes.
pub(crate) const MAX_SIGNATURE: u64 = 4096;

/// What starts the first line of a minisign file, which is not signed.
const UNTRUSTED: &str = "untrusted comment:";
/// What starts the third line of a signature file; the rest is signed.
const TRUSTED: &str = "trusted comment: ";

/// A key the release signs packages with.
#[derive(Debug)]
pub(crate) struct ReleaseKey {
    id: [u8; 8],
    key: VerifyingKey,
}

impl ReleaseKey {
    /// From minisign's text form: a public key file, or its key line alone.
    pub(crate) fn parse(text: &str) -> Result<ReleaseKey, String> {
        let bad = || "this build's release key is not a minisign public key".to_string();
        let mut lines = text
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty() && !l.starts_with(UNTRUSTED));
        let (Some(line), None) = (lines.next(), lines.next()) else {
            return Err(bad());
        };
        let bytes = decode(line).ok_or_else(bad)?;
        let (b"Ed", id, key) = split::<2, 8, 32>(&bytes).ok_or_else(bad)? else {
            return Err(bad());
        };
        let key = VerifyingKey::from_bytes(key).map_err(|_| bad())?;
        Ok(ReleaseKey { id: *id, key })
    }
}

/// The file beside `package` in a release that holds its signature.
pub(crate) fn signature_name(package: &str) -> String {
    format!("{package}.minisig")
}

/// Whether `signature`, the text of a minisign signature file, is `key`'s
/// signature of the file at `path`, made for a package named `name`.
pub(crate) fn check(
    key: &ReleaseKey,
    signature: &str,
    path: &Path,
    name: &str,
) -> Result<(), String> {
    let not_ours = || format!("{name} is not signed with {}'s release key", crate::APP);
    let mut lines = signature.lines();
    let (Some(untrusted), Some(signed), Some(trusted), Some(global)) =
        (lines.next(), lines.next(), lines.next(), lines.next())
    else {
        return Err(format!("{name}'s signature is not a minisign signature"));
    };
    let comment = trusted.strip_prefix(TRUSTED);
    let (Some(signed), Some(global), Some(comment)) = (decode(signed), decode(global), comment)
    else {
        return Err(format!("{name}'s signature is not a minisign signature"));
    };
    if !untrusted.starts_with(UNTRUSTED) || lines.any(|l| !l.trim().is_empty()) {
        return Err(format!("{name}'s signature is not a minisign signature"));
    }
    let (alg, id, signed) = split::<2, 8, 64>(&signed).ok_or_else(not_ours)?;
    let global: &[u8; 64] = global.as_slice().try_into().map_err(|_| not_ours())?;
    if alg != b"ED" || *id != key.id {
        return Err(not_ours());
    }
    // The comment first: it is short, and says which package this is.
    let mut whole = Vec::with_capacity(64 + comment.len());
    whole.extend_from_slice(signed);
    whole.extend_from_slice(comment.as_bytes());
    key.key
        .verify_strict(&whole, &Signature::from_bytes(global))
        .map_err(|_| not_ours())?;
    if comment != format!("file:{name}") {
        return Err(format!(
            "the signature offered with {name} was made for another file ({comment})"
        ));
    }
    let hash = blake2b(path)?;
    key.key
        .verify_strict(&hash, &Signature::from_bytes(signed))
        .map_err(|_| not_ours())
}

/// The file's BLAKE2b-512, read in pieces (a package is tens of MB).
fn blake2b(path: &Path) -> Result<[u8; 64], String> {
    let mut file = std::fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut hash = Blake2b512::new();
    let mut buf = vec![0u8; 256 * 1024];
    loop {
        match file.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => hash.update(&buf[..n]),
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
            Err(e) => return Err(format!("{}: {e}", path.display())),
        }
    }
    let mut out = [0u8; 64];
    out.copy_from_slice(&hash.finalize());
    Ok(out)
}

fn decode(line: &str) -> Option<Vec<u8>> {
    base64::engine::general_purpose::STANDARD
        .decode(line.trim())
        .ok()
}

/// `bytes` as three arrays of the given lengths, if that is all it holds.
fn split<const A: usize, const B: usize, const C: usize>(
    bytes: &[u8],
) -> Option<(&[u8; A], &[u8; B], &[u8; C])> {
    if bytes.len() != A + B + C {
        return None;
    }
    let (a, rest) = bytes.split_at(A);
    let (b, c) = rest.split_at(B);
    Some((a.try_into().ok()?, b.try_into().ok()?, c.try_into().ok()?))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Made with minisign 0.12 (`minisign -G -W`, then `minisign -S -t
    /// 'file:Helio-1.0.1-linux-x86_64.tar.gz'`) with a key made for this
    /// test and thrown away.
    const KEY: &str = "untrusted comment: minisign public key 007F1F2DE4C6EB1F
RWQf68bkLR9/AHo/SrilDjAF1YW7SCfZOSCDnLjeA8Voa2OvEBEnSTTo
";
    const PACKAGE: &[u8] = b"a stand-in for Helio-1.0.1-linux-x86_64.tar.gz\n";
    const NAME: &str = "Helio-1.0.1-linux-x86_64.tar.gz";
    const SIGNATURE: &str = "untrusted comment: signature from minisign secret key
RUQf68bkLR9/AGA5/nXRVNPe2gbkA/dQKzpCo8VykWOjRx2c6kcnq7MrzN53okDXHZNfMDiM6stF203gQaPFpbmLGIL4cfzbDQA=
trusted comment: file:Helio-1.0.1-linux-x86_64.tar.gz
remegRgfQEh4upcG15LNQto5HqsU8dak8nR0+JdDTcwETOC02mTMDmHtm7GfwVUMlm2UMtQaWkGcvXoc/hysCQ==
";

    fn package(dir: &Path, bytes: &[u8]) -> std::path::PathBuf {
        let path = dir.join(NAME);
        std::fs::write(&path, bytes).unwrap();
        path
    }

    #[test]
    fn a_package_minisign_signed_for_its_name_is_accepted() {
        let dir = tempfile::tempdir().unwrap();
        let key = ReleaseKey::parse(KEY).unwrap();
        assert_eq!(
            check(&key, SIGNATURE, &package(dir.path(), PACKAGE), NAME),
            Ok(())
        );
        // The key line alone is the same key.
        let line = ReleaseKey::parse(KEY.lines().nth(1).unwrap()).unwrap();
        assert_eq!(line.id, key.id);
    }

    #[test]
    fn a_changed_package_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let key = ReleaseKey::parse(KEY).unwrap();
        let mut changed = PACKAGE.to_vec();
        changed[0] ^= 1;
        let err = check(&key, SIGNATURE, &package(dir.path(), &changed), NAME).unwrap_err();
        assert!(err.contains("not signed with"), "{err}");
    }

    #[test]
    fn a_signature_made_for_another_package_is_refused() {
        // Signed for 1.0.1, offered as 1.0.2: an older package passed off
        // as the newest, or another system's.
        let dir = tempfile::tempdir().unwrap();
        let key = ReleaseKey::parse(KEY).unwrap();
        let path = package(dir.path(), PACKAGE);
        let err = check(&key, SIGNATURE, &path, "Helio-1.0.2-linux-x86_64.tar.gz").unwrap_err();
        assert!(err.contains("made for another file"), "{err}");
        // And a comment changed to match is no longer signed.
        let forged = SIGNATURE.replace("1.0.1", "1.0.2");
        let err = check(&key, &forged, &path, "Helio-1.0.2-linux-x86_64.tar.gz").unwrap_err();
        assert!(err.contains("not signed with"), "{err}");
    }

    #[test]
    fn another_key_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        // The release's own key did not make this signature.
        let release = ReleaseKey::parse(include_str!("../release-signing.pub")).unwrap();
        let err = check(&release, SIGNATURE, &package(dir.path(), PACKAGE), NAME).unwrap_err();
        assert!(err.contains("not signed with"), "{err}");
    }

    #[test]
    fn the_unhashed_form_and_what_is_not_a_signature_are_refused() {
        let dir = tempfile::tempdir().unwrap();
        let key = ReleaseKey::parse(KEY).unwrap();
        let path = package(dir.path(), PACKAGE);
        let mut lines: Vec<String> = SIGNATURE.lines().map(String::from).collect();
        // `Ed`: the file itself signed, which minisign once made.
        let mut signed = decode(&lines[1]).unwrap();
        signed[1] = b'd';
        lines[1] = base64::engine::general_purpose::STANDARD.encode(&signed);
        assert!(check(&key, &lines.join("\n"), &path, NAME).is_err());
        for text in [
            "",
            "untrusted comment: x",
            &SIGNATURE.replace("trusted comment: ", "comment: "),
            &format!("{SIGNATURE}one line too many\n"),
            &SIGNATURE.replace("RUQ", "!!!"),
        ] {
            assert!(check(&key, text, &path, NAME).is_err(), "{text}");
        }
    }

    #[test]
    fn what_is_not_a_public_key_is_refused() {
        for text in [
            "",
            "untrusted comment: minisign public key 007F1F2DE4C6EB1F",
            "RWQf68bkLR9/AHo/",
            // Two keys.
            "RWQf68bkLR9/AHo/SrilDjAF1YW7SCfZOSCDnLjeA8Voa2OvEBEnSTTo\n\
             RWQf68bkLR9/AHo/SrilDjAF1YW7SCfZOSCDnLjeA8Voa2OvEBEnSTTo",
        ] {
            assert!(ReleaseKey::parse(text).is_err(), "{text}");
        }
        assert!(ReleaseKey::parse(include_str!("../release-signing.pub")).is_ok());
    }
}
