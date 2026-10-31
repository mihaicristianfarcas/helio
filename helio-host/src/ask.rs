//! Asking the host that runs on this computer, from a terminal: to restart
//! (`helio-host restart`), and to take a pairing PIN (`helio-host pair`), as
//! Helio's This computer pages do. `helio host restart` and `helio host
//! pair` run these (helio-cli's `host`), and its `add-client`,
//! `remove-client` and `permissions` run `restart` after their change.
//!
//! Both go through the host's API (`web::probe`), signed in with the token
//! the host writes for its own computer. On Windows only administrators may
//! read it: from another terminal they exit with [`NEEDS_ELEVATION`].
//!
//! `pair` is how a host nobody sits at takes a device: the API answers this
//! computer only, so over SSH, `helio pair HOST` on the device shows the
//! PIN and `helio host pair PIN` on the host gives it.

use std::path::Path;
use std::process::ExitCode;

use helio_proto::permission::Permissions;
use serde_json::{json, Value};

use crate::hosting::NEEDS_ELEVATION;
use crate::web::probe;

/// The exit code when no host runs to ask: `helio host` changing the host's
/// data then has nothing to say (a host reads it when it starts).
pub const NOT_RUNNING: u8 = 3;

/// The token the host keeps for its own computer, or the exit code that
/// says why there is none.
fn local_token(dir: &Path, command: &str) -> Result<String, ExitCode> {
    match std::fs::read_to_string(dir.join("local-token")) {
        Ok(t) => Ok(t.trim().to_string()),
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
            eprintln!(
                "helio-host {command} needs an administrator: run it again from an elevated \
                    terminal (exit code {NEEDS_ELEVATION}; nothing was changed)"
            );
            std::process::exit(NEEDS_ELEVATION);
        }
        Err(_) => {
            eprintln!("Helio Host is not running: it has not started on this computer yet.");
            Err(ExitCode::from(NOT_RUNNING))
        }
    }
}

/// The host's answer to `METHOD path`, or the exit code that says why
/// there is none.
fn call(
    dir: &Path,
    command: &str,
    method: &str,
    path: &str,
    body: Option<&Value>,
) -> Result<(u16, Value), ExitCode> {
    let token = local_token(dir, command)?;
    let port = probe::api_port(dir);
    let body = body.map(Value::to_string);
    let Some((code, text)) = probe::call(dir, port, method, path, &token, body.as_deref()) else {
        eprintln!("Helio Host is not running (nothing answers its API on port {port}).");
        return Err(ExitCode::from(NOT_RUNNING));
    };
    if code == 401 {
        eprintln!(
            "Helio Host did not take this computer's token (it makes a new one each time it \
                starts; try again)."
        );
        return Err(ExitCode::FAILURE);
    }
    Ok((code, serde_json::from_str(&text).unwrap_or(Value::Null)))
}

/// `helio-host restart`: the running host starts again, now or when its
/// session ends (`restart`).
pub fn restart(dir: &Path) -> ExitCode {
    match call(dir, "restart", "POST", "/api/restart", Some(&json!({}))) {
        Ok((200, answer)) => {
            println!(
                "{}",
                restart_said(answer.get("restart").and_then(Value::as_str))
            );
            ExitCode::SUCCESS
        }
        Ok((code, answer)) => {
            eprintln!("Helio Host did not restart: {}", error_of(code, &answer));
            ExitCode::FAILURE
        }
        Err(code) => code,
    }
}

/// What `helio-host restart` says of the host's answer (`restart::When`'s
/// words).
fn restart_said(when: Option<&str>) -> &'static str {
    match when {
        Some("after_session") => {
            "Helio Host restarts when the session that runs ends: a device streams now."
        }
        _ => "Helio Host is restarting.",
    }
}

/// What `helio-host pair` was asked: the PIN, which device (when several
/// ask), and what it may do (None: what pairing gives).
#[derive(Debug, PartialEq)]
struct PairAsk {
    pin: String,
    device: Option<String>,
    permissions: Option<Permissions>,
}

/// `PIN [--device NAME] [--permissions LIST]`, as helio-cli passes them on.
fn pair_ask(args: &[String]) -> Result<PairAsk, String> {
    let mut words = args.iter();
    let pin = words
        .next()
        .filter(|p| !p.starts_with("--"))
        .ok_or("helio-host pair PIN [--device NAME] [--permissions LIST]")?
        .clone();
    let mut ask = PairAsk {
        pin,
        device: None,
        permissions: None,
    };
    while let Some(flag) = words.next() {
        let value = words.next().ok_or(format!("{flag} needs a value"))?;
        match flag.as_str() {
            "--device" => ask.device = Some(value.clone()),
            "--permissions" => ask.permissions = Some(Permissions::parse(value)?),
            other => return Err(format!("no option {other:?}")),
        }
    }
    Ok(ask)
}

/// Which pairing request the PIN is for, out of `pending` (the API's
/// `/api/pairing`): the one `device` names, or the only one.
fn request_for(pending: &[Value], device: Option<&str>) -> Result<(u64, String), String> {
    let name_of = |p: &Value| {
        p.get("client_name")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string()
    };
    let asking: Vec<&Value> = pending
        .iter()
        .filter(|p| device.is_none_or(|d| name_of(p).eq_ignore_ascii_case(d)))
        .collect();
    match (asking.as_slice(), device) {
        ([one], _) => one
            .get("id")
            .and_then(Value::as_u64)
            .map(|id| (id, name_of(one)))
            .ok_or_else(|| "the host's answer was not understood".to_string()),
        ([], None) => Err(
            "No device is asking to pair. Start pairing on it first (`helio pair \
            HOST`, or Add Host in Helio), then give the PIN it shows."
                .into(),
        ),
        ([], Some(d)) => Err(format!(
            "No device called {d:?} is asking to pair{}.",
            asking_now(pending, name_of)
        )),
        (_, _) => Err(format!(
            "More than one device is asking to pair{}: say which with --device NAME.",
            asking_now(pending, name_of)
        )),
    }
}

/// ": NAME, NAME" for the devices asking, or nothing.
fn asking_now(pending: &[Value], name_of: impl Fn(&Value) -> String) -> String {
    let names: Vec<String> = pending.iter().map(name_of).collect();
    if names.is_empty() {
        String::new()
    } else {
        format!(" (asking: {})", names.join(", "))
    }
}

/// `helio-host pair PIN [--device NAME] [--permissions LIST]`: give the PIN a
/// device shows to the host, as Helio's This computer → Devices does.
pub fn pair(dir: &Path, args: &[String]) -> ExitCode {
    let ask = match pair_ask(args) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::FAILURE;
        }
    };
    let pending = match call(dir, "pair", "GET", "/api/pairing", None) {
        Ok((200, Value::Array(list))) => list,
        Ok((code, answer)) => {
            eprintln!(
                "Helio Host did not list its pairing requests: {}",
                error_of(code, &answer)
            );
            return ExitCode::FAILURE;
        }
        Err(code) => return code,
    };
    let (id, name) = match request_for(&pending, ask.device.as_deref()) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::FAILURE;
        }
    };
    let mut body = json!({ "pin": ask.pin });
    if let Some(p) = ask.permissions {
        body["permissions"] = json!(p);
    }
    match call(
        dir,
        "pair",
        "POST",
        &format!("/api/pairing/{id}"),
        Some(&body),
    ) {
        Ok((200, answer)) => {
            let client = answer
                .get("client")
                .and_then(Value::as_str)
                .unwrap_or(&name);
            println!("Paired {client}. It can connect now.");
            ExitCode::SUCCESS
        }
        Ok((code, answer)) => {
            eprintln!("{}", error_of(code, &answer));
            ExitCode::FAILURE
        }
        Err(code) => code,
    }
}

/// The host's own words for a refusal, or its status code.
fn error_of(code: u16, answer: &Value) -> String {
    answer
        .get("error")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| format!("it answered {code}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(line: &str) -> Vec<String> {
        line.split_whitespace().map(str::to_string).collect()
    }

    #[test]
    fn a_pin_alone_is_for_whoever_asks_with_what_pairing_gives() {
        assert_eq!(
            pair_ask(&words("4821")),
            Ok(PairAsk {
                pin: "4821".into(),
                device: None,
                permissions: None,
            })
        );
    }

    #[test]
    fn the_device_and_its_permissions_can_be_named() {
        let ask = pair_ask(&words("4821 --device phone --permissions see-only")).unwrap();
        assert_eq!(ask.device.as_deref(), Some("phone"));
        assert_eq!(
            ask.permissions,
            Some(Permissions::parse("see-only").unwrap())
        );
    }

    #[test]
    fn a_missing_pin_or_value_is_refused() {
        assert!(pair_ask(&[]).is_err());
        assert!(pair_ask(&words("--device phone")).is_err());
        assert!(pair_ask(&words("4821 --device")).is_err());
        assert!(pair_ask(&words("4821 --colour blue")).is_err());
    }

    fn asking(names: &[&str]) -> Vec<Value> {
        names
            .iter()
            .enumerate()
            .map(|(i, n)| json!({"id": i as u64 + 7, "client_name": n}))
            .collect()
    }

    #[test]
    fn one_device_asking_takes_the_pin() {
        assert_eq!(
            request_for(&asking(&["macbook"]), None),
            Ok((7, "macbook".into()))
        );
    }

    #[test]
    fn with_several_asking_the_device_must_be_named() {
        let pending = asking(&["macbook", "phone"]);
        assert!(request_for(&pending, None)
            .unwrap_err()
            .contains("asking: macbook, phone"));
        assert_eq!(
            request_for(&pending, Some("Phone")),
            Ok((8, "phone".into()))
        );
    }

    #[test]
    fn with_nobody_asking_the_pin_goes_nowhere() {
        assert!(request_for(&[], None)
            .unwrap_err()
            .starts_with("No device is asking"));
        assert!(request_for(&asking(&["macbook"]), Some("phone")).is_err());
    }
}
