//! `helio`: the command line. Streaming and pairing with the client's own
//! code, the app's (`helio stream` opens a window of the platform's own,
//! not the app's), and this computer as a host (`helio host`, `host`).
//! `helio --help` lists every command (`args::HELP`), and docs/cli.md says
//! more of each.
//!
//! A library, with the `helio` bin a line over it, so that the app can be
//! `helio` too: Helio's own program runs [`main`] when it is started as
//! `helio`, which is how installing Helio puts the command on the PATH.
//!
//! `HELIO_TEST_INPUT` scripts input into a stream (see `script`).

mod args;
pub mod engine;
mod host;
#[cfg(any(target_os = "macos", windows, target_os = "linux"))]
mod script;
#[cfg(any(target_os = "macos", windows, target_os = "linux"))]
mod stream;

use std::path::Path;
use std::process::ExitCode;

use args::{Command, Usage};

/// Run the command `args` (without the program's name) and say how it went.
pub fn main(args: &[String]) -> ExitCode {
    let command = match args::parse(args) {
        Ok(command) => command,
        Err(usage) => return misread(usage),
    };
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                "info,mainline=error,helio_client=info,helio_transport=info,helio_decode=info"
                    .into()
            }),
        )
        .init();
    let dir = helio_client::store::data_dir();
    match command {
        Command::Help { host } => {
            print!("{}", if host { args::HOST_HELP } else { args::HELP });
            ExitCode::SUCCESS
        }
        Command::Identity => identity(&dir),
        Command::Discover => discover(&dir),
        Command::Pair { address } => pair(&dir, &address),
        Command::PairAgent { address } => pair_agent(&dir, &address),
        Command::Hosts => {
            for h in helio_client::store::Hosts::load(&dir).list() {
                println!("{}\t{}", h.name, h.address);
            }
            ExitCode::SUCCESS
        }
        Command::AddHost {
            name,
            address,
            x25519,
            mlkem,
        } => add_host(&dir, &name, &address, &x25519, &mlkem),
        Command::RemoveHost { name } => {
            match helio_client::store::Hosts::load(&dir).remove(&name) {
                Ok(true) => ExitCode::SUCCESS,
                Ok(false) => fail(format!("no paired host named {name}; see `helio hosts`")),
                Err(e) => fail(e),
            }
        }
        Command::Wake { name } => wake(&dir, &name),
        #[cfg(any(target_os = "macos", windows, target_os = "linux"))]
        Command::Stream { name, flags, watch } => stream::run(&name, &flags, watch),
        #[cfg(not(any(target_os = "macos", windows, target_os = "linux")))]
        Command::Stream { .. } => fail("streams need macOS, Windows or Linux"),
        Command::Host(command) => host::run(command),
    }
}

/// A command line `helio` does not take: why, and the help (on stderr: it
/// is not the output asked for).
fn misread(usage: Usage) -> ExitCode {
    let help = if usage.host {
        "helio host --help"
    } else {
        "helio --help"
    };
    match usage.why {
        Some(why) => eprintln!("{why}\n(`{help}` lists the commands)"),
        None => eprint!(
            "{}",
            if usage.host {
                args::HOST_HELP
            } else {
                args::HELP
            }
        ),
    }
    ExitCode::FAILURE
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

fn add_host(dir: &Path, name: &str, address: &str, x25519: &str, mlkem: &str) -> ExitCode {
    if helio_transport::PublicIdentity::from_b64(x25519, mlkem).is_err() {
        return fail("malformed host keys");
    }
    let host = helio_client::store::KnownHost::by_hand(name, address, x25519, mlkem);
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
