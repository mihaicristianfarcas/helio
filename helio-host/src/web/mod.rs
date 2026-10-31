//! Helio Host's API, for Helio's This computer pages on this computer
//! (`helio-app/src/host`) and `helio host` (`probe`): the session, pairing
//! requests and their PINs, the paired devices and what each may do, the
//! settings (what the host reads only when it starts restarts it:
//! `restart`), the log. What Sunshine's web UI does, in Helio's own window.
//!
//! HTTPS on the API port, with a self-signed certificate made on first run,
//! on localhost only (127.0.0.1 and ::1): nothing on the network reaches
//! it. A device a person cannot sit at pairs with `helio host pair PIN`
//! over SSH. Every call but `/api/state` carries a bearer token: the local
//! token this host writes where only its own user (on Windows, SYSTEM and
//! Administrators) can read it, or an app token Helio got by signing in once
//! with the admin account (kept hashed here, in app-tokens.toml), which a
//! Windows Helio needs: it is not elevated, and the host's folder is
//! administrators' only.

pub mod probe;

use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use argon2::password_hash::{
    rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString,
};
use argon2::Argon2;
use axum::extract::{Path as UrlPath, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::config::HostConfig;
use crate::host::Host;
use crate::pairing::PinResult;
use helio_proto::permission::Permissions;

const LOCAL_TOKEN: &str = "local-token";
const APP_TOKENS: &str = "app-tokens.toml";

#[derive(Serialize, Deserialize, Default)]
struct Credentials {
    user: String,
    hash: String,
}

/// Tokens Helio Host's app got by signing in, by their SHA-256.
#[derive(Serialize, Deserialize, Default)]
struct AppTokens {
    #[serde(default)]
    tokens: Vec<AppToken>,
}

#[derive(Serialize, Deserialize, Clone)]
struct AppToken {
    name: String,
    sha256: String,
    created_unix: u64,
}

struct Web {
    host: Arc<Host>,
    failures: Mutex<Vec<Instant>>,
    /// Readable only by this host's own user: that user's Helio Host app.
    local_token: Option<String>,
    app_tokens: Mutex<AppTokens>,
}

type Shared = Arc<Web>;

fn credentials_path(dir: &Path) -> std::path::PathBuf {
    dir.join("web-credentials.toml")
}

fn load_credentials(dir: &Path) -> Option<Credentials> {
    std::fs::read_to_string(credentials_path(dir))
        .ok()
        .and_then(|t| toml::from_str(&t).ok())
}

fn token() -> String {
    let mut b = [0u8; 32];
    rand_core::RngCore::fill_bytes(&mut rand_core::OsRng, &mut b);
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn sha256_hex(text: &str) -> String {
    use sha2::Digest;
    sha2::Sha256::digest(text.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Equal, in time that does not depend on where they differ.
fn same(a: &str, b: &str) -> bool {
    a.len() == b.len()
        && a.bytes()
            .zip(b.bytes())
            .fold(0u8, |acc, (x, y)| acc | (x ^ y))
            == 0
}

fn bearer_of(headers: &HeaderMap) -> Option<String> {
    let raw = headers.get(header::AUTHORIZATION)?.to_str().ok()?;
    raw.strip_prefix("Bearer ").map(|t| t.trim().to_string())
}

fn load_app_tokens(dir: &Path) -> AppTokens {
    std::fs::read_to_string(dir.join(APP_TOKENS))
        .ok()
        .and_then(|t| toml::from_str(&t).ok())
        .unwrap_or_default()
}

fn save_app_tokens(dir: &Path, tokens: &AppTokens) -> std::io::Result<()> {
    let text = toml::to_string(tokens).unwrap_or_default();
    helio_transport::identity::write_private(&dir.join(APP_TOKENS), text.as_bytes())?;
    crate::private::restrict_to_admins(&dir.join(APP_TOKENS))
}

/// A fresh local token, where only this host's user can read it: 0600 on a
/// Mac or Linux (the host runs as its user), SYSTEM and Administrators on
/// Windows (the host is a service; an app that is not elevated signs in).
/// None when the file cannot be made private: then there is none.
fn write_local_token(dir: &Path) -> Option<String> {
    let path = dir.join(LOCAL_TOKEN);
    let t = token();
    let written = helio_transport::identity::write_private(&path, t.as_bytes())
        .and_then(|()| crate::private::restrict_to_admins(&path));
    match written {
        Ok(()) => Some(t),
        Err(e) => {
            tracing::warn!(error = %e, "no local token for Helio Host's app (it signs in instead)");
            let _ = std::fs::remove_file(&path);
            None
        }
    }
}

impl Web {
    fn authed(&self, headers: &HeaderMap) -> bool {
        let Some(t) = bearer_of(headers) else {
            return false;
        };
        if self.local_token.as_deref().is_some_and(|l| same(l, &t)) {
            return true;
        }
        let hash = sha256_hex(&t);
        self.app_tokens
            .lock()
            .tokens
            .iter()
            .any(|a| same(&a.sha256, &hash))
    }
}

fn unauthorized() -> Response {
    (
        StatusCode::UNAUTHORIZED,
        Json(json!({"error": "not signed in"})),
    )
        .into_response()
}

macro_rules! require_auth {
    ($web:expr, $headers:expr) => {
        if !$web.authed(&$headers) {
            return unauthorized();
        }
    };
}

async fn state(State(web): State<Shared>, headers: HeaderMap) -> Response {
    let setup_needed = load_credentials(&web.host.data_dir).is_none();
    Json(json!({
        "setup_needed": setup_needed,
        "signed_in": web.authed(&headers),
        "name": web.host.config.read().name,
        "os": std::env::consts::OS,
    }))
    .into_response()
}

#[derive(Deserialize)]
struct Login {
    user: String,
    password: String,
}

/// Make the admin account, which a Windows Helio signs in with (the first
/// time, `/api/app-token` with `setup`).
fn setup(web: &Web, req: &Login) -> Result<(), Box<Response>> {
    let fail = |code: StatusCode, error: String| {
        Err(Box::new(
            (code, Json(json!({ "error": error }))).into_response(),
        ))
    };
    let dir = &web.host.data_dir;
    if load_credentials(dir).is_some() {
        return fail(StatusCode::CONFLICT, "already set up".into());
    }
    if req.user.trim().is_empty() || req.password.len() < 8 {
        return fail(
            StatusCode::BAD_REQUEST,
            "choose a user name and a password of at least 8 characters".into(),
        );
    }
    let salt = SaltString::generate(&mut OsRng);
    let Ok(hash) = Argon2::default().hash_password(req.password.as_bytes(), &salt) else {
        return fail(StatusCode::INTERNAL_SERVER_ERROR, "hashing failed".into());
    };
    let creds = Credentials {
        user: req.user.trim().to_string(),
        hash: hash.to_string(),
    };
    tracing::info!(user = creds.user, "API: admin account created");
    let text = toml::to_string(&creds).unwrap_or_default();
    if let Err(e) =
        helio_transport::identity::write_private(&credentials_path(dir), text.as_bytes())
    {
        return fail(StatusCode::INTERNAL_SERVER_ERROR, e.to_string());
    }
    Ok(())
}

/// The admin account's user name and password, rate-limited.
fn check_password(web: &Web, req: &Login) -> Result<(), Box<Response>> {
    {
        let mut f = web.failures.lock();
        f.retain(|t| t.elapsed() < Duration::from_secs(60));
        if f.len() >= 10 {
            return Err(Box::new(
                (
                    StatusCode::TOO_MANY_REQUESTS,
                    Json(json!({"error": "too many attempts; wait a minute"})),
                )
                    .into_response(),
            ));
        }
    }
    let Some(creds) = load_credentials(&web.host.data_dir) else {
        return Err(Box::new(
            (StatusCode::CONFLICT, Json(json!({"error": "not set up"}))).into_response(),
        ));
    };
    let ok = req.user == creds.user
        && PasswordHash::new(&creds.hash)
            .map(|h| {
                Argon2::default()
                    .verify_password(req.password.as_bytes(), &h)
                    .is_ok()
            })
            .unwrap_or(false);
    if !ok {
        web.failures.lock().push(Instant::now());
        tracing::warn!(user = req.user, "API: wrong user name or password");
        return Err(Box::new(
            (
                StatusCode::UNAUTHORIZED,
                Json(json!({"error": "wrong user name or password"})),
            )
                .into_response(),
        ));
    }
    tracing::info!(user = req.user, "API: signed in");
    Ok(())
}

#[derive(Deserialize)]
struct AppLogin {
    user: String,
    password: String,
    /// Who asks ("Helio Host on gaming-pc"), to tell tokens apart.
    #[serde(default)]
    name: String,
    /// Create the admin account first (the first sign-in).
    #[serde(default)]
    setup: bool,
}

/// Helio Host's app signs in once: an app token for it to keep.
async fn app_token(State(web): State<Shared>, Json(req): Json<AppLogin>) -> Response {
    let login = Login {
        user: req.user,
        password: req.password,
    };
    let checked = if req.setup {
        setup(&web, &login)
    } else {
        check_password(&web, &login)
    };
    if let Err(r) = checked {
        return *r;
    }
    let t = token();
    let created_unix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let name = if req.name.trim().is_empty() {
        "Helio Host app".to_string()
    } else {
        req.name.trim().to_string()
    };
    let saved = {
        let mut tokens = web.app_tokens.lock();
        tracing::info!(name, "API: app token issued");
        tokens.tokens.push(AppToken {
            name,
            sha256: sha256_hex(&t),
            created_unix,
        });
        save_app_tokens(&web.host.data_dir, &tokens)
    };
    match saved {
        Ok(()) => Json(json!({"token": t})).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": e.to_string()})),
        )
            .into_response(),
    }
}

/// Helio Host's app signs out: its token stops working.
async fn drop_app_token(State(web): State<Shared>, headers: HeaderMap) -> Response {
    require_auth!(web, headers);
    let Some(t) = bearer_of(&headers) else {
        return Json(json!({"ok": false})).into_response();
    };
    let hash = sha256_hex(&t);
    let mut tokens = web.app_tokens.lock();
    let before = tokens.tokens.len();
    tokens.tokens.retain(|a| !same(&a.sha256, &hash));
    if tokens.tokens.len() != before {
        tracing::info!("API: app token revoked (signed out)");
        let _ = save_app_tokens(&web.host.data_dir, &tokens);
    }
    Json(json!({"ok": true})).into_response()
}

async fn status(State(web): State<Shared>, headers: HeaderMap) -> Response {
    require_auth!(web, headers);
    let h = &web.host;
    let c = h.config.read().clone();
    // Out of the statement below: it locks the client list too, and a lock
    // taken twice in one statement is a deadlock.
    let (clients, pending) = {
        let list = h.clients.lock();
        (list.list().len(), h.pairing.list(&list))
    };
    Json(json!({
        "name": c.name,
        "id": h.endpoint.identity().public().short_id(),
        "version": env!("CARGO_PKG_VERSION"),
        "port": h.endpoint.local_port(),
        "session": *h.status.lock(),
        "clients": clients,
        "pending": pending,
        "tunnels": h.endpoint.peers().iter().filter(|p| p.is_established()).count(),
        "internet": c.internet_access,
        "public": h.stun.public().iter().map(|a| a.to_string()).collect::<Vec<_>>(),
        "port_mapping": *h.port_mapping.lock(),
        "grants": grants(),
        "restart": h.restart_pending().map(|w| w.word()),
        "started_unix": h.started_unix,
    }))
    .into_response()
}

/// What macOS lets this host do, for the app's Hosting page: Screen
/// Recording (asked of the process: it changes when the host starts again)
/// and Accessibility (live). Elsewhere there is nothing to grant: null.
fn grants() -> serde_json::Value {
    #[cfg(target_os = "macos")]
    {
        json!({
            "screen": crate::permissions::screen_capture_allowed(),
            "accessibility": helio_input::macos::trusted(),
        })
    }
    #[cfg(not(target_os = "macos"))]
    {
        serde_json::Value::Null
    }
}

async fn pending(State(web): State<Shared>, headers: HeaderMap) -> Response {
    require_auth!(web, headers);
    let list = web.host.pairing.list(&web.host.clients.lock());
    Json(list).into_response()
}

#[derive(Deserialize)]
struct Pin {
    pin: String,
    /// What the client may do; absent: what it gets by default.
    #[serde(default)]
    permissions: Option<Permissions>,
}

async fn submit_pin(
    State(web): State<Shared>,
    headers: HeaderMap,
    UrlPath(id): UrlPath<u32>,
    Json(req): Json<Pin>,
) -> Response {
    require_auth!(web, headers);
    let host = web.host.clone();
    let result = tokio::task::spawn_blocking(move || {
        host.pairing.submit(&host, id, &req.pin, req.permissions)
    })
    .await;
    match &result {
        Ok(PinResult::Paired(name)) => {
            tracing::info!(request = id, client = name, "API: PIN accepted, paired")
        }
        Ok(PinResult::WrongPin) => tracing::warn!(request = id, "API: wrong PIN"),
        Ok(PinResult::NotFound) => {
            tracing::info!(request = id, "API: PIN for a request that is gone")
        }
        Ok(PinResult::TooManyAttempts) => {
            tracing::warn!(request = id, "API: too many wrong PINs")
        }
        Ok(PinResult::Failed(e)) => {
            tracing::warn!(request = id, error = e, "API: pairing failed")
        }
        Err(e) => tracing::warn!(request = id, error = %e, "API: pairing task failed"),
    }
    match result {
        Ok(PinResult::Paired(name)) => Json(json!({"ok": true, "client": name})).into_response(),
        Ok(PinResult::WrongPin) => (
            StatusCode::BAD_REQUEST,
            Json(
                json!({"error": "That PIN did not match the one the client is showing. \
                    Start pairing again on the client."}),
            ),
        )
            .into_response(),
        Ok(PinResult::NotFound) => (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "That request is gone (it expired or the client gave up)."})),
        )
            .into_response(),
        Ok(PinResult::TooManyAttempts) => (
            StatusCode::TOO_MANY_REQUESTS,
            Json(json!({"error": "Too many wrong PINs; wait a minute."})),
        )
            .into_response(),
        Ok(PinResult::Failed(e)) => {
            (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e}))).into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": e.to_string()})),
        )
            .into_response(),
    }
}

async fn decline(
    State(web): State<Shared>,
    headers: HeaderMap,
    UrlPath(id): UrlPath<u32>,
) -> Response {
    require_auth!(web, headers);
    tracing::info!(request = id, "API: pairing request declined");
    Json(json!({"ok": web.host.pairing.decline(id)})).into_response()
}

async fn clients(State(web): State<Shared>, headers: HeaderMap) -> Response {
    require_auth!(web, headers);
    let list: Vec<_> = web
        .host
        .clients
        .lock()
        .list()
        .iter()
        .map(|c| {
            let id = c.public().map(|p| p.short_id()).unwrap_or_default();
            let online = c
                .public()
                .and_then(|p| web.host.endpoint.peer_by_key(&p.x25519))
                .is_some_and(|p| p.is_established());
            json!({
                "name": c.name, "id": id, "key": c.x25519, "paired_at": c.paired_at, "online": online,
                "agent": c.agent, "permissions": c.permissions,
                "possible": Permissions::possible(c.agent),
            })
        })
        .collect();
    Json(list).into_response()
}

#[derive(Deserialize)]
struct Unpair {
    key: String,
}

async fn unpair(
    State(web): State<Shared>,
    headers: HeaderMap,
    Json(req): Json<Unpair>,
) -> Response {
    require_auth!(web, headers);
    let name = web
        .host
        .clients
        .lock()
        .list()
        .iter()
        .find(|c| c.x25519 == req.key)
        .map(|c| c.name.clone())
        .unwrap_or_default();
    tracing::info!(client = name, "API: unpair");
    match web.host.remove_client(&req.key) {
        Ok(found) => Json(json!({"ok": found})).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": e.to_string()})),
        )
            .into_response(),
    }
}

#[derive(Deserialize)]
struct SetPermissions {
    key: String,
    permissions: Permissions,
}

async fn set_permissions(
    State(web): State<Shared>,
    headers: HeaderMap,
    Json(req): Json<SetPermissions>,
) -> Response {
    require_auth!(web, headers);
    let found = web
        .host
        .clients
        .lock()
        .list()
        .iter()
        .find(|c| c.x25519 == req.key)
        .map(|c| (c.name.clone(), c.agent));
    let Some((name, agent)) = found else {
        return Json(json!({"ok": false})).into_response();
    };
    tracing::info!(client = name, permissions = ?req.permissions.fit(agent).names(), "API: permissions");
    match web.host.set_permissions(&req.key, req.permissions) {
        Ok(now) => Json(json!({"ok": now.is_some(), "permissions": now})).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": e.to_string()})),
        )
            .into_response(),
    }
}

async fn agent_log(State(web): State<Shared>, headers: HeaderMap) -> Response {
    require_auth!(web, headers);
    Json(web.host.agent_log()).into_response()
}

async fn agent_op(
    State(web): State<Shared>,
    headers: HeaderMap,
    UrlPath(op): UrlPath<String>,
) -> Response {
    use helio_proto::control::agent_control;
    require_auth!(web, headers);
    let op_name = op.clone();
    let op = match op.as_str() {
        "pause" => agent_control::PAUSE,
        "resume" => agent_control::RESUME,
        "handback" => agent_control::HAND_BACK,
        "stop" => agent_control::STOP,
        _ => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({"error": "no such action"})),
            )
                .into_response()
        }
    };
    tracing::info!(op = op_name, "API: agent control");
    web.host.agent_control(op);
    Json(json!({"ok": true})).into_response()
}

async fn get_config(State(web): State<Shared>, headers: HeaderMap) -> Response {
    require_auth!(web, headers);
    Json(web.host.config.read().clone()).into_response()
}

async fn put_config(
    State(web): State<Shared>,
    headers: HeaderMap,
    Json(new): Json<HostConfig>,
) -> Response {
    require_auth!(web, headers);
    let restart = {
        let old = web.host.config.read();
        // What changed, key by key (nothing in config.toml is secret).
        if let (Ok(serde_json::Value::Object(a)), Ok(serde_json::Value::Object(b))) =
            (serde_json::to_value(&*old), serde_json::to_value(&new))
        {
            for (k, v) in &b {
                if a.get(k) != Some(v) {
                    tracing::info!(key = k, from = %a.get(k).cloned().unwrap_or_default(), to = %v, "API: setting changed");
                }
            }
        }
        taken_at_start(&old, &new)
    };
    if let Err(e) = new.save(&web.host.data_dir) {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": e.to_string()})),
        )
            .into_response();
    }
    *web.host.config.write() = new;
    // Saved first: the host that starts again reads it.
    let when = restart.map(|why| web.host.request_restart(why));
    Json(json!({"ok": true, "restart": when.map(|w| w.word())})).into_response()
}

/// Why the host has to start again for `new` to apply over `old`, if it
/// does: what it takes in only when it starts (`restart`). The rest applies
/// at once or at the next session.
fn taken_at_start(old: &HostConfig, new: &HostConfig) -> Option<&'static str> {
    if old.name != new.name {
        Some("the name changed")
    } else if old.port != new.port || old.pairing_port != new.pairing_port {
        Some("a port changed")
    } else if old.api_port != new.api_port {
        Some("the API's port changed")
    } else if old.nvidia_max_power != new.nvidia_max_power
        || old.nvidia_dxgi_present != new.nvidia_dxgi_present
    {
        Some("an NVIDIA setting changed")
    } else {
        None
    }
}

/// Restart the host: now, or when the session that runs ends. `helio host
/// restart` asks for it.
async fn restart(State(web): State<Shared>, headers: HeaderMap) -> Response {
    require_auth!(web, headers);
    let when = web.host.request_restart("asked for");
    Json(json!({"ok": true, "restart": when.word()})).into_response()
}

/// Whether macOS has allowed Screen Recording since this host asked, which
/// the host cannot learn by asking again itself (`permissions`); allowed,
/// the host restarts to have it. Helio's window asks when it comes back to
/// the front (from System Settings). Elsewhere there is nothing to grant.
async fn check_grants(State(web): State<Shared>, headers: HeaderMap) -> Response {
    require_auth!(web, headers);
    #[cfg(target_os = "macos")]
    {
        let host = web.host.clone();
        let when = tokio::task::spawn_blocking(move || {
            crate::permissions::follow_grant(|why| host.request_restart(why))
        })
        .await
        .ok()
        .flatten();
        Json(json!({"ok": true, "restart": when.map(|w| w.word())})).into_response()
    }
    #[cfg(not(target_os = "macos"))]
    {
        Json(json!({"ok": true, "restart": null})).into_response()
    }
}

async fn end_session(State(web): State<Shared>, headers: HeaderMap) -> Response {
    require_auth!(web, headers);
    tracing::info!("API: end the session");
    web.host.end_session();
    Json(json!({"ok": true})).into_response()
}

async fn logs(State(web): State<Shared>, headers: HeaderMap) -> Response {
    require_auth!(web, headers);
    let dir = web.host.data_dir.join("logs");
    let newest = std::fs::read_dir(&dir)
        .ok()
        .and_then(|rd| {
            rd.filter_map(Result::ok)
                .filter(|e| {
                    e.file_name()
                        .to_string_lossy()
                        .starts_with("helio-host.log")
                })
                .max_by_key(|e| e.metadata().and_then(|m| m.modified()).ok())
        })
        .map(|e| e.path());
    let text = newest
        .and_then(|p| std::fs::read_to_string(p).ok())
        .unwrap_or_default();
    let tail: Vec<&str> = text.lines().rev().take(400).collect();
    let tail: Vec<&str> = tail.into_iter().rev().collect();
    (
        [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
        tail.join("\n"),
    )
        .into_response()
}

fn certificate(dir: &Path, name: &str) -> Result<(Vec<u8>, Vec<u8>), String> {
    let (cert_path, key_path) = (dir.join("web-cert.pem"), dir.join("web-key.pem"));
    if let (Ok(c), Ok(k)) = (std::fs::read(&cert_path), std::fs::read(&key_path)) {
        return Ok((c, k));
    }
    let names = vec![
        "localhost".to_string(),
        name.to_lowercase(),
        format!("{}.local", name.to_lowercase()),
    ];
    let cert = rcgen::generate_simple_self_signed(names).map_err(|e| e.to_string())?;
    let (c, k) = (
        cert.cert.pem().into_bytes(),
        cert.signing_key.serialize_pem().into_bytes(),
    );
    std::fs::write(&cert_path, &c).map_err(|e| e.to_string())?;
    // Helio Host's window pins this certificate, and is not elevated.
    crate::private::make_public(&cert_path);
    helio_transport::identity::write_private(&key_path, &k).map_err(|e| e.to_string())?;
    Ok((c, k))
}

/// Serve the API until the process exits. Call on its own thread.
pub fn serve(host: Arc<Host>) {
    let (port, name) = {
        let c = host.config.read();
        (c.api_port, c.name.clone())
    };
    let _ = rustls::crypto::ring::default_provider().install_default();
    let (cert, key) = match certificate(&host.data_dir, &name) {
        Ok(v) => v,
        Err(e) => {
            tracing::error!(error = %e, "the API's certificate");
            return;
        }
    };
    let local_token = write_local_token(&host.data_dir);
    let app_tokens = Mutex::new(load_app_tokens(&host.data_dir));
    let web = Arc::new(Web {
        host,
        failures: Mutex::new(Vec::new()),
        local_token,
        app_tokens,
    });
    let app = Router::new()
        .route("/api/state", get(state))
        .route("/api/app-token", post(app_token).delete(drop_app_token))
        .route("/api/status", get(status))
        .route("/api/pairing", get(pending))
        .route("/api/pairing/{id}", post(submit_pin).delete(decline))
        .route("/api/clients", get(clients))
        .route("/api/clients/remove", post(unpair))
        .route("/api/clients/permissions", post(set_permissions))
        .route("/api/agent/log", get(agent_log))
        .route("/api/agent/{op}", post(agent_op))
        .route("/api/config", get(get_config).put(put_config))
        .route("/api/restart", post(restart))
        .route("/api/grants/check", post(check_grants))
        .route("/api/session", delete(end_session))
        .route("/api/logs", get(logs))
        .with_state(web);

    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
    {
        Ok(r) => r,
        Err(e) => {
            tracing::error!(error = %e, "the API's runtime");
            return;
        }
    };
    runtime.block_on(async move {
        let tls = match axum_server::tls_rustls::RustlsConfig::from_pem(cert, key).await {
            Ok(t) => t,
            Err(e) => {
                tracing::error!(error = %e, "the API's TLS");
                return;
            }
        };
        // This computer only, over IPv4 and IPv6 both: `localhost` is
        // either, depending on the system.
        let serve = |addr: SocketAddr| {
            let (tls, app) = (tls.clone(), app.clone());
            async move {
                if let Err(e) = axum_server::bind_rustls(addr, tls)
                    .serve(app.into_make_service())
                    .await
                {
                    tracing::warn!(%addr, error = %e, "the API stopped answering here");
                }
            }
        };
        tracing::info!(
            port,
            "API at https://localhost:{port}, for this computer only"
        );
        tokio::join!(
            serve(SocketAddr::from((std::net::Ipv4Addr::LOCALHOST, port))),
            serve(SocketAddr::from((std::net::Ipv6Addr::LOCALHOST, port))),
        );
    });
}

#[cfg(test)]
mod tests {
    use super::taken_at_start;
    use crate::config::HostConfig;

    #[test]
    fn a_new_name_port_or_nvidia_setting_needs_the_host_to_start_again() {
        let old = HostConfig::default();
        let changed = [
            HostConfig {
                name: format!("{} 2", old.name),
                ..old.clone()
            },
            HostConfig {
                port: old.port + 1,
                ..old.clone()
            },
            HostConfig {
                pairing_port: old.pairing_port + 1,
                ..old.clone()
            },
            HostConfig {
                api_port: old.api_port + 1,
                ..old.clone()
            },
            HostConfig {
                nvidia_max_power: !old.nvidia_max_power,
                ..old.clone()
            },
            HostConfig {
                nvidia_dxgi_present: !old.nvidia_dxgi_present,
                ..old.clone()
            },
        ];
        for new in &changed {
            assert!(taken_at_start(&old, new).is_some());
        }
    }

    #[test]
    fn settings_a_running_host_follows_need_no_restart() {
        let old = HostConfig::default();
        let new = HostConfig {
            internet_access: !old.internet_access,
            port_mapping: !old.port_mapping,
            allow_takeover: !old.allow_takeover,
            keep_host_displays: !old.keep_host_displays,
            max_bitrate_kbps: 50_000,
            clipboard: !old.clipboard,
            ..old.clone()
        };
        assert_eq!(taken_at_start(&old, &new), None);
        assert_eq!(taken_at_start(&old, &old), None);
    }
}
