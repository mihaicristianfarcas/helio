//! The host's web API from this computer: whether it answers, for
//! `helio-host status`, and asking it to restart, for `helio-host restart`.
//!
//! HTTPS on localhost, trusting only the certificate the host made for
//! itself (`web-cert.pem`, the one Helio Host's window pins,
//! `helio-app/src/host/api.rs`): something else on the port is not the host.
//!
//! Both only read the host's folder: `helio-host status` runs without an
//! administrator, often before the host has ever run, and must create
//! nothing there; `helio-host restart` reads the local token the host wrote
//! for its own computer.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::WebPkiSupportedAlgorithms;
use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{ClientConfig, ClientConnection, DigitallySignedStruct, SignatureScheme, StreamOwned};

use crate::config::{HostConfig, DEFAULT_WEB_PORT};

/// How long the host has to answer. It answers on localhost within
/// milliseconds; one that takes longer is reported as not answering rather
/// than holding up the app that asked.
const TIMEOUT: Duration = Duration::from_secs(2);

/// The web port in the host's settings, without writing them (the default
/// when there are none yet).
pub fn web_port(dir: &Path) -> u16 {
    std::fs::read_to_string(HostConfig::path(dir))
        .ok()
        .and_then(|t| toml::from_str::<HostConfig>(&t).ok())
        .map_or(DEFAULT_WEB_PORT, |c| c.web_port)
}

/// Whether the host on this computer answers its web API on `port`.
pub fn answers(dir: &Path, port: u16) -> bool {
    let request = b"GET /api/state HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n";
    let Some(mut tls) = connect(dir, port) else {
        return false;
    };
    let mut status = [0u8; 12];
    tls.write_all(request).is_ok()
        && tls.read_exact(&mut status).is_ok()
        && &status == b"HTTP/1.1 200"
}

/// `POST path` with an empty JSON body, signed in with `token`: the status
/// code and the body, or None when no host answered.
pub fn post(dir: &Path, port: u16, path: &str, token: &str) -> Option<(u16, String)> {
    let request = format!(
        "POST {path} HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer {token}\r\n\
            Content-Type: application/json\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{{}}"
    );
    let mut tls = connect(dir, port)?;
    tls.write_all(request.as_bytes()).ok()?;
    let mut reply = Vec::new();
    // The host closes the connection after its answer, maybe without
    // TLS's goodbye: what arrived before it is the answer.
    let _ = tls.read_to_end(&mut reply);
    parse_reply(&reply)
}

/// An HTTP/1.1 answer's status code and body.
fn parse_reply(reply: &[u8]) -> Option<(u16, String)> {
    let text = String::from_utf8_lossy(reply);
    let (head, body) = text.split_once("\r\n\r\n")?;
    let code = head.strip_prefix("HTTP/1.1 ")?.get(..3)?.parse().ok()?;
    Some((code, body.to_string()))
}

/// A TLS connection to the host on `port`, trusting only its certificate.
fn connect(dir: &Path, port: u16) -> Option<StreamOwned<ClientConnection, TcpStream>> {
    // The host makes it when it first runs: none, and no host has answered yet.
    let pem = std::fs::read(dir.join("web-cert.pem")).ok()?;
    let cert = CertificateDer::from_pem_slice(&pem).ok()?;
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let verifier = Arc::new(Pinned {
        cert,
        algorithms: provider.signature_verification_algorithms,
    });
    let config = ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .ok()?
        .dangerous()
        .with_custom_certificate_verifier(verifier)
        .with_no_client_auth();
    let name = ServerName::try_from("localhost").ok()?;
    let connection = ClientConnection::new(Arc::new(config), name).ok()?;
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let tcp = TcpStream::connect_timeout(&addr, TIMEOUT).ok()?;
    let _ = tcp.set_read_timeout(Some(TIMEOUT));
    let _ = tcp.set_write_timeout(Some(TIMEOUT));
    Some(StreamOwned::new(connection, tcp))
}

/// Trusts the one certificate the host made for itself, and checks the
/// handshake was signed with its key.
#[derive(Debug)]
struct Pinned {
    cert: CertificateDer<'static>,
    algorithms: WebPkiSupportedAlgorithms,
}

impl ServerCertVerifier for Pinned {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        if end_entity.as_ref() == self.cert.as_ref() {
            Ok(ServerCertVerified::assertion())
        } else {
            Err(rustls::Error::InvalidCertificate(
                rustls::CertificateError::UnknownIssuer,
            ))
        }
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(message, cert, dss, &self.algorithms)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(message, cert, dss, &self.algorithms)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.algorithms.supported_schemes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_answer_gives_its_status_code_and_body() {
        let reply =
            b"HTTP/1.1 200 OK\r\ncontent-type: application/json\r\n\r\n{\"restart\":\"now\"}";
        assert_eq!(
            parse_reply(reply),
            Some((200, "{\"restart\":\"now\"}".to_string()))
        );
    }

    #[test]
    fn a_cut_off_answer_gives_nothing() {
        assert_eq!(parse_reply(b"HTTP/1.1 200 OK\r\ncontent-ty"), None);
        assert_eq!(parse_reply(b""), None);
    }
}
