//! `helio`: the command-line client. The same streaming code as the app.
//!
//!   helio identity                              this client's public keys
//!   helio discover                              hosts on the local network
//!   helio pair HOST[:PORT]                      pair (shows a PIN to type on the host)
//!   helio pair-agent HOST[:PORT]                pair this device's AI agent (its own key)
//!   helio watch NAME [stream flags]             watch the AI agent working on NAME
//!   helio hosts                                 paired hosts
//!   helio add-host NAME ADDR X25519 MLKEM       pair by hand (see `helio-host identity`)
//!   helio remove-host NAME
//!   helio wake NAME                             Wake-on-LAN
//!   helio stream NAME [--size WxH] [--fps N] [--mbps N] [--codec h264|hevc|av1]
//!                    [--windowed] [--no-vsync] [--frame-pacing] [--stats] [--cmd-is-win]
//!                    [--mute-in-background] [--audio-channels 2|6|8] [--steam]
//!                    [--no-audio] [--host-audio] [--wan-only] [--keep-host-displays]
//!                    [--via ADDR] [--no-clipboard] [--hdr] [--yuv444]
//!
//! `HELIO_TEST_INPUT` scripts input into a stream (see `script`).

#[cfg(any(target_os = "macos", windows, target_os = "linux"))]
mod script;
#[cfg(any(target_os = "macos", windows, target_os = "linux"))]
mod stream;

use std::path::Path;
use std::process::ExitCode;

fn usage() -> ExitCode {
    eprintln!(
        "usage: helio identity | discover | pair HOST[:PORT] | pair-agent HOST[:PORT] | hosts | remove-host NAME | wake NAME\n\
            \x20      helio watch NAME [stream flags]\n\
            \x20      helio add-host NAME ADDR X25519 MLKEM\n\
            \x20      helio stream NAME [--size WxH] [--fps N] [--mbps N] [--codec h264|hevc|av1]\n\
            \x20                       [--windowed] [--no-vsync] [--frame-pacing] [--stats] [--cmd-is-win]\n\
            \x20                       [--mute-in-background] [--audio-channels 2|6|8] [--steam]\n\
            \x20                       [--no-audio] [--host-audio] [--wan-only] [--keep-host-displays] [--via ADDR]\n\
            \x20                       [--no-clipboard] [--hdr] [--yuv444]"
    );
    ExitCode::FAILURE
}

/// Run the command `args` (without the program's name) and say how it went.
pub fn main(args: &[String]) -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                "info,mainline=error,helio_client=info,helio_transport=info,helio_decode=info"
                    .into()
            }),
        )
        .init();
    let dir = helio_client::store::data_dir();
    match args.first().map(String::as_str) {
        Some("identity") => identity(&dir),
        Some("discover") => discover(&dir),
        Some("pair") => match args.get(1) {
            Some(spec) => pair(&dir, spec),
            None => usage(),
        },
        Some("pair-agent") => match args.get(1) {
            Some(spec) => pair_agent(&dir, spec),
            None => usage(),
        },
        Some("hosts") => {
            for h in helio_client::store::Hosts::load(&dir).list() {
                println!("{}\t{}", h.name, h.address);
            }
            ExitCode::SUCCESS
        }
        Some("add-host") => add_host(&dir, args),
        Some("wake") => match args.get(1) {
            Some(name) => wake(&dir, name),
            None => usage(),
        },
        Some("remove-host") => match args
            .get(1)
            .map(|n| helio_client::store::Hosts::load(&dir).remove(n))
        {
            Some(Ok(true)) => ExitCode::SUCCESS,
            Some(Ok(false)) => fail("no such host"),
            Some(Err(e)) => fail(e),
            None => usage(),
        },
        #[cfg(any(target_os = "macos", windows, target_os = "linux"))]
        Some("stream") => match args.get(1) {
            Some(name) => stream::run(name, &args[2..]),
            None => usage(),
        },
        #[cfg(any(target_os = "macos", windows, target_os = "linux"))]
        Some("watch") => match args.get(1) {
            Some(name) => {
                let mut flags = args[2..].to_vec();
                flags.push("--watch".into());
                stream::run(name, &flags)
            }
            None => usage(),
        },
        _ => usage(),
    }
}

/// Print `e` and fail.
fn fail(e: impl std::fmt::Display) -> ExitCode {
    eprintln!("{e}");
    ExitCode::FAILURE
}

fn identity(dir: &Path) -> ExitCode {
    match helio_client::store::identity(dir) {
        Ok(id) => {
            let (x, m) = id.public().to_b64();
            println!("{x} {m}");
            ExitCode::SUCCESS
        }
        Err(e) => fail(e),
    }
}

fn discover(dir: &Path) -> ExitCode {
    match helio_client::pair::discover(dir, std::time::Duration::from_secs(3)) {
        Ok(found) if found.is_empty() => {
            eprintln!(
                "no hosts found on the local network (add one by address with `helio pair \
                    HOST`)"
            );
            ExitCode::SUCCESS
        }
        Ok(found) => {
            for f in found {
                let state = if f.paired { "paired" } else { "not paired" };
                println!(
                    "{}\t{}\t{}\t{}\thttps://{}:{}",
                    f.name,
                    f.os,
                    f.pairing_addr(),
                    state,
                    f.address,
                    f.web_port
                );
            }
            ExitCode::SUCCESS
        }
        Err(e) => fail(e),
    }
}

/// Where to type the PIN, while pairing waits for it.
fn show_pin(pin: &str) {
    eprintln!("\nOn the host, open its web UI (Pair a device) and enter this PIN:\n\n    {pin}\n");
}

fn pair(dir: &Path, spec: &str) -> ExitCode {
    let addr = match helio_client::pair::resolve(spec) {
        Ok(a) => a,
        Err(e) => return fail(e),
    };
    let pin = helio_pairing::pair::new_pin();
    let name = helio_client::pair::client_name();
    eprintln!("Pairing with {addr} as \"{name}\"...");
    match helio_client::pair::pair(dir, addr, &name, &pin, || show_pin(&pin)) {
        Ok(host) => {
            println!(
                "Paired with {} ({}). Stream with: helio stream {}",
                host.name, host.address, host.name
            );
            ExitCode::SUCCESS
        }
        Err(e) => fail(format!("Pairing failed: {e}")),
    }
}

fn pair_agent(dir: &Path, spec: &str) -> ExitCode {
    let addr = match helio_client::pair::resolve(spec) {
        Ok(a) => a,
        Err(e) => return fail(e),
    };
    let pin = helio_pairing::pair::new_pin();
    eprintln!("Pairing this device's AI agent with {addr}...");
    match helio_client::pair::pair_agent(dir, addr, &pin, &Default::default(), || show_pin(&pin)) {
        Ok(host) => {
            println!(
                "The agent is paired with {}. Agents reach it through `Helio mcp` or \
                    `helio-agent`.",
                host.name
            );
            ExitCode::SUCCESS
        }
        Err(e) => fail(format!("Pairing failed: {e}")),
    }
}

fn add_host(dir: &Path, args: &[String]) -> ExitCode {
    let (Some(name), Some(addr), Some(x), Some(m)) =
        (args.get(1), args.get(2), args.get(3), args.get(4))
    else {
        return usage();
    };
    if helio_transport::PublicIdentity::from_b64(x, m).is_err() {
        return fail("malformed host keys");
    }
    let host = helio_client::store::KnownHost::by_hand(name, addr, x, m);
    match helio_client::store::Hosts::load(dir).upsert(host) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => fail(e),
    }
}

fn wake(dir: &Path, name: &str) -> ExitCode {
    let hosts = helio_client::store::Hosts::load(dir);
    let Some(host) = hosts.find(name) else {
        return fail(format!("no paired host named {name}; see `helio hosts`"));
    };
    match helio_client::wake::wake(host) {
        Ok(n) => {
            println!("sent {n} wake packets to {}", host.name);
            ExitCode::SUCCESS
        }
        Err(e) => fail(e),
    }
}
