//! The engine's Authenticode signature, checked before Windows runs it as
//! SYSTEM: a valid signature, chained to a root this PC trusts, by the
//! expected publisher.
//!
//! `WinVerifyTrust` with the generic Authenticode policy, without UI and
//! without revocation checks (the service is set up offline as well as
//! on); then the signing certificate's whole subject, compared with
//! `HELIO_HOST_WINDOWS_PUBLISHER` as the build was given it, written as Windows
//! writes it (`CN=…, O=…, L=…, S=…, C=…`, what
//! `(Get-AuthenticodeSignature helio-host.exe).SignerCertificate.Subject` says).
//!
//! The whole subject, not only its common name (the publisher UAC shows):
//! two organisations can share a name, rarely also a place. Not a
//! thumbprint either: a certificate is renewed often (every few days, with
//! Azure Trusted Signing), and its subject stays while its thumbprint does
//! not.
//!
//! Windows releases are not signed yet. Until they are, a failed check only
//! warns: the release workflow builds with `--features require-signature`
//! (and `HELIO_HOST_WINDOWS_PUBLISHER` set) once they are, and from then on a
//! failed check refuses. A debug build never requires it.

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

/// Whether an engine that fails the check is refused (else warned about).
pub const REQUIRED: bool = cfg!(feature = "require-signature") && !cfg!(debug_assertions);

/// The publisher a signed engine must name: the signing certificate's
/// subject, given to the build.
pub const PUBLISHER: Option<&str> = option_env!("HELIO_HOST_WINDOWS_PUBLISHER");

// A build that refuses unsigned engines must know whose to accept.
const _: () = assert!(
    !REQUIRED || PUBLISHER.is_some(),
    "--features require-signature needs HELIO_HOST_WINDOWS_PUBLISHER, the signing certificate's \
        subject (CN=…, O=…, C=…)"
);

/// Check `file`'s signature; what is wrong with it, for a person.
pub fn check(file: &Path) -> Result<(), String> {
    let publisher = signer(file)?;
    match PUBLISHER {
        Some(expected) if publisher != expected => {
            Err(format!("it is signed by {publisher}, not by {expected}"))
        }
        _ => Ok(()),
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
    // No window: no UI may be shown (`WTD_UI_NONE`), and a service has none.
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

/// The signing certificate's subject (`CN=…, O=…, C=…`, the most specific
/// part first, as .NET and PowerShell write it), from what a successful
/// `WinVerifyTrust` keeps.
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
