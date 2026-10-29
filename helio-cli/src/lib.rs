//! `helio`: the command line. Streaming and pairing with the client's own
//! code, the app's (`helio stream` opens a window of the platform's own,
//! not the app's), and this computer as a host (`helio host`, `host`).
//! `helio --help` lists every command (`help`), and docs/cli.md says more
//! of each. In a terminal it is in colour, its lists in columns; piped, it
//! is plain text, as programs read it (`style`).
//!
//! A library, with the `helio` bin a line over it, so that the app can be
//! `helio` too: Helio's own program runs [`main`] when it is started as
//! `helio`, so the command comes with Helio (whose Settings > General puts
//! it on the PATH).
//!
//! `HELIO_TEST_INPUT` scripts input into a stream (see `script`).

mod args;
pub mod engine;
mod help;
mod host;
#[cfg(any(target_os = "macos", windows, target_os = "linux"))]
mod script;
#[cfg(any(target_os = "macos", windows, target_os = "linux"))]
mod stream;
mod style;

use std::path::Path;
use std::process::ExitCode;

use args::{Command, Usage};
use style::{Look, To};

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
        .with_ansi(style::colour(To::Out))
        .init();
    let dir = helio_client::store::data_dir();
    match command {
        Command::Help { host } => {
            print!("{}", help::text(host, style::colour(To::Out)));
            ExitCode::SUCCESS
        }
        Command::Identity => identity(&dir),
        Command::Discover => discover(&dir),
        Command::Pair { address } => pair(&dir, &address),
        Command::PairAgent { address } => pair_agent(&dir, &address),
        Command::Hosts => hosts(&dir),
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
        Some(why) => {
            style::error(why);
            eprintln!(
                "{}",
                style::to(
                    To::Err,
                    Look::Quiet,
                    &format!("(`{help}` lists the commands)")
                )
            );
        }
        None => eprint!("{}", help::text(usage.host, style::colour(To::Err))),
    }
    ExitCode::FAILURE
}

/// Print `e` and fail.
fn fail(e: impl std::fmt::Display) -> ExitCode {
    style::error(e);
    ExitCode::FAILURE
}

/// A list on stdout: in columns under `header` in a terminal, else a line
/// of tab-separated cells per row, as programs read it.
pub(crate) fn list(
    header: &[&str],
    rows: &[Vec<String>],
    look: impl Fn(usize, &str) -> Option<Look>,
) {
    if style::terminal(To::Out) {
        print!(
            "{}",
            style::table(header, rows, style::colour(To::Out), look)
        );
    } else {
        for row in rows {
            println!("{}", row.join("\t"));
        }
    }
}

fn hosts(dir: &Path) -> ExitCode {
    let rows: Vec<Vec<String>> = helio_client::store::Hosts::load(dir)
        .list()
        .iter()
        .map(|h| vec![h.name.clone(), h.address.clone()])
        .collect();
    if rows.is_empty() && style::terminal(To::Out) {
        eprintln!(
            "{}",
            style::to(
                To::Err,
                Look::Quiet,
                "no paired hosts yet: `helio discover` finds them, `helio pair HOST` pairs one"
            )
        );
        return ExitCode::SUCCESS;
    }
    list(&["NAME", "ADDRESS"], &rows, |c, _| {
        (c == 0).then_some(Look::Strong)
    });
    ExitCode::SUCCESS
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
            let rows: Vec<Vec<String>> = found
                .iter()
                .map(|f| {
                    let state = if f.paired { "paired" } else { "not paired" };
                    vec![
                        f.name.clone(),
                        f.os.clone(),
                        f.pairing_addr().to_string(),
                        state.into(),
                        format!("https://{}:{}", f.address, f.web_port),
                    ]
                })
                .collect();
            list(
                &["NAME", "SYSTEM", "PAIR AT", "STATE", "WEB UI"],
                &rows,
                |c, cell| match (c, cell) {
                    (0, _) => Some(Look::Strong),
                    (3, "paired") => Some(Look::Good),
                    (3, _) => Some(Look::Quiet),
                    _ => None,
                },
            );
            ExitCode::SUCCESS
        }
        Err(e) => fail(e),
    }
}

/// Where to type the PIN, while pairing waits for it.
fn show_pin(pin: &str) {
    eprintln!(
        "\nOn the host, open its web UI (Pair a device) and enter this PIN:\n\n    {}\n",
        style::to(To::Err, Look::Strong, pin)
    );
}

fn pair(dir: &Path, spec: &str) -> ExitCode {
    let addr = match helio_client::pair::resolve(spec) {
        Ok(a) => a,
        Err(e) => return fail(e),
    };
    let pin = helio_pairing::pair::new_pin();
    let name = helio_client::pair::client_name();
    eprintln!(
        "{}",
        style::to(
            To::Err,
            Look::Quiet,
            &format!("Pairing with {addr} as \"{name}\"...")
        )
    );
    match helio_client::pair::pair(dir, addr, &name, &pin, || show_pin(&pin)) {
        Ok(host) => {
            println!(
                "{} with {} ({}). Stream with: {}",
                style::to(To::Out, Look::Good, "Paired"),
                host.name,
                host.address,
                style::to(
                    To::Out,
                    Look::Literal,
                    &format!("helio stream {}", host.name)
                )
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
    eprintln!(
        "{}",
        style::to(
            To::Err,
            Look::Quiet,
            &format!("Pairing this device's AI agent with {addr}...")
        )
    );
    match helio_client::pair::pair_agent(dir, addr, &pin, &Default::default(), || show_pin(&pin)) {
        Ok(host) => {
            println!(
                "{} with {}. Agents reach it through `Helio mcp` or `helio-agent`.",
                style::to(To::Out, Look::Good, "The agent is paired"),
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
            println!(
                "{} {n} wake packets to {}",
                style::to(To::Out, Look::Good, "Sent"),
                host.name
            );
            ExitCode::SUCCESS
        }
        Err(e) => fail(e),
    }
}
