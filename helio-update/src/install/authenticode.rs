//! The installer's Authenticode signature, checked before it runs: a
//! valid signature, chained to a root this PC trusts, by Helio's publisher.
//!
//! The same check the engine makes of its copy in Program Files before
//! Windows runs it as SYSTEM (`helio-host/src/hosting/signature.rs`):
//! `WinVerifyTrust` with the generic Authenticode policy, without UI and
//! without revocation checks; then the signing certificate's whole
//! subject (`CN=…, O=…, C=…`, as `Get-AuthenticodeSignature` writes it),
//! compared with `HELIO_HOST_WINDOWS_PUBLISHER` as the build was given it.
//! The subject, not a thumbprint: the certificate is renewed, its subject
//! stays.
//!
//! A build given a publisher (the release workflow's, once it signs the
//! Windows programs, `.github/workflows/release.yml`) refuses an installer
//! not signed by it: unsigned, changed after signing, signed by someone
//! else, or by a certificate this PC does not trust. A build without one
//! (releases made before Helio had a certificate) has only the SHA-256
//! GitHub keeps to go by, and says so in its log.

use std::path::Path;

use windows::core::{HRESULT, HSTRING, PCWSTR};
use windows::Win32::Foundation::{
    HANDLE, HWND, INVALID_HANDLE_VALUE, TRUST_E_BAD_DIGEST, TRUST_E_NOSIGNATURE,
};
use windows::Win32::Security::Cryptography::{
    CertGetNameStringW, CERT_NAME_RDN_TYPE, CERT_NAME_STR_REVERSE_FLAG, CERT_X500_NAME_STR,
};
use windows::Win32::Security::WinTrust::{
    WTHelperGetProvCertFromChain, WTHelperGetProvSignerFromChain, WTHelperProvDataFromStateData,
    WinVerifyTrust, WINTRUST_ACTION_GENERIC_VERIFY_V2, WINTRUST_DATA, WINTRUST_DATA_0,
    WINTRUST_FILE_INFO, WTD_CHOICE_FILE, WTD_REVOKE_NONE, WTD_STATEACTION_CLOSE,
    WTD_STATEACTION_VERIFY, WTD_UI_NONE,
};

/// The publisher an installer must be signed by: the signing certificate's
/// subject, given to the build (the engine's, `HELIO_HOST_WINDOWS_PUBLISHER`:
/// one certificate signs every Windows program of a release).
pub(crate) const PUBLISHER: Option<&str> = option_env!("HELIO_HOST_WINDOWS_PUBLISHER");

/// Check `file`'s signature against `publisher` (this build's, as a rule:
/// [`PUBLISHER`]); what is wrong with it, for a person. Without a
/// publisher to require, any outcome is let through, and logged.
pub(crate) fn check(file: &Path, publisher: Option<&str>) -> Result<(), String> {
    let name = file
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    match (signer(file), publisher) {
        (Ok(signer), Some(expected)) if signer == expected => Ok(()),
        (Ok(signer), Some(expected)) => Err(format!(
            "{name} is signed by {signer}, not by {expected}, Helio's publisher"
        )),
        (Err(problem), Some(_)) => Err(format!("{name} is refused: {problem}")),
        (Ok(signer), None) => {
            tracing::info!(
                signer,
                "the installer is signed; this build requires no publisher"
            );
            Ok(())
        }
        (Err(problem), None) => {
            tracing::warn!(
                problem,
                "the installer's signature was not checked: this build knows no publisher, \
                 so GitHub's SHA-256 is all it goes by"
            );
            Ok(())
        }
    }
}

/// The publisher whose valid, trusted signature `file` carries.
fn signer(file: &Path) -> Result<String, String> {
    let path = HSTRING::from(file.as_os_str());
    let mut info = WINTRUST_FILE_INFO {
        cbStruct: std::mem::size_of::<WINTRUST_FILE_INFO>() as u32,
        pcwszFilePath: PCWSTR(path.as_ptr()),
        ..Default::default()
    };
    let mut data = WINTRUST_DATA {
        cbStruct: std::mem::size_of::<WINTRUST_DATA>() as u32,
        dwUIChoice: WTD_UI_NONE,
        fdwRevocationChecks: WTD_REVOKE_NONE,
        dwUnionChoice: WTD_CHOICE_FILE,
        Anonymous: WINTRUST_DATA_0 { pFile: &mut info },
        dwStateAction: WTD_STATEACTION_VERIFY,
        ..Default::default()
    };
    let mut action = WINTRUST_ACTION_GENERIC_VERIFY_V2;
    // No window: no UI may be shown (`WTD_UI_NONE`).
    let no_window = HWND(INVALID_HANDLE_VALUE.0);
    // SAFETY: `data` points at `info`, which points at `path`; all three
    // outlive both calls. The state the first call keeps is released by the
    // second (WTD_STATEACTION_CLOSE), whatever the first found.
    let verdict = unsafe {
        let verdict = WinVerifyTrust(no_window, &mut action, (&raw mut data).cast());
        let publisher = (verdict == 0).then(|| publisher_of(data.hWVTStateData));
        data.dwStateAction = WTD_STATEACTION_CLOSE;
        WinVerifyTrust(no_window, &mut action, (&raw mut data).cast());
        match publisher {
            Some(p) => Ok(p),
            None => Err(HRESULT(verdict)),
        }
    };
    match verdict {
        Ok(Some(publisher)) => Ok(publisher),
        Ok(None) => Err("its signature names no publisher".into()),
        Err(e) if e == TRUST_E_NOSIGNATURE => Err("it is not signed".into()),
        Err(e) if e == TRUST_E_BAD_DIGEST => {
            Err("it was changed after it was signed (its signature does not match it)".into())
        }
        Err(e) => Err(format!("its signature is not trusted: {}", e.message())),
    }
}

/// The signing certificate's subject, the most specific part first, from
/// what a successful `WinVerifyTrust` keeps.
///
/// # Safety
/// `state` is the `hWVTStateData` of a verification that succeeded and is
/// not closed yet.
unsafe fn publisher_of(state: HANDLE) -> Option<String> {
    // SAFETY: per the contract; each pointer is checked before it is read,
    // and all of them belong to `state`, alive until it is closed.
    unsafe {
        let provider = WTHelperProvDataFromStateData(state);
        if provider.is_null() {
            return None;
        }
        let signer = WTHelperGetProvSignerFromChain(provider, 0, false, 0);
        if signer.is_null() {
            return None;
        }
        let cert = WTHelperGetProvCertFromChain(signer, 0);
        if cert.is_null() || (*cert).pCert.is_null() {
            return None;
        }
        let format = CERT_X500_NAME_STR.0 | CERT_NAME_STR_REVERSE_FLAG;
        let mut name = [0u16; 1024];
        let len = CertGetNameStringW(
            (*cert).pCert,
            CERT_NAME_RDN_TYPE,
            0,
            Some((&raw const format).cast()),
            Some(&mut name),
        ) as usize;
        // The length counts the NUL; 1 is an empty name.
        (len > 1).then(|| String::from_utf16_lossy(&name[..len - 1]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A program this PC trusts, validly signed by a publisher that is not
    /// Helio's: Microsoft Edge's, which Windows 10 and 11 come with and
    /// which carries its signature in the file (most of System32 is signed
    /// through catalogs, which a file's own check does not see).
    fn someone_elses_program() -> Option<std::path::PathBuf> {
        let x86 = std::env::var_os("ProgramFiles(x86)")?;
        let edge = Path::new(&x86).join(r"Microsoft\Edge\Application\msedge.exe");
        edge.is_file().then_some(edge)
    }

    #[test]
    fn an_unsigned_or_unreadable_installer_is_refused_once_a_publisher_is_known() {
        let dir = tempfile::tempdir().unwrap();
        // This test's own program: built here, signed by no one.
        let unsigned = dir.path().join("Helio-1.0.1-windows-x86_64-setup.exe");
        std::fs::copy(std::env::current_exe().unwrap(), &unsigned).unwrap();
        let err = check(&unsigned, Some("CN=Helio")).unwrap_err();
        assert!(err.contains("it is not signed"), "{err}");
        // Not a program at all.
        let text = dir.path().join("setup.exe");
        std::fs::write(&text, b"not a program").unwrap();
        assert!(check(&text, Some("CN=Helio")).is_err());
        // A build that knows no publisher has only the checksum to go by.
        assert_eq!(check(&unsigned, None), Ok(()));
    }

    #[test]
    fn an_installer_signed_by_someone_else_is_refused() {
        let Some(edge) = someone_elses_program() else {
            eprintln!("skipped: no Microsoft Edge on this PC to stand for another publisher");
            return;
        };
        let signer = signer(&edge).expect("Edge carries a trusted signature");
        assert!(signer.contains("O=Microsoft Corporation"), "{signer}");
        // Signed, trusted, and still not Helio's.
        let err = check(&edge, Some("CN=Helio, O=Helio, C=RO")).unwrap_err();
        assert!(
            err.contains(&format!("signed by {signer}, not by CN=Helio")),
            "{err}"
        );
        // Its own publisher is accepted: the comparison is of the whole
        // subject, as Windows writes it.
        assert_eq!(check(&edge, Some(&signer)), Ok(()));
    }
}
