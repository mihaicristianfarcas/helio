//! `helio host`: this computer as a host.
//!
//! The switch (`enable`, `disable`) and `status` are the host's program's
//! own (`helio-host`, found and run by `engine`, as the app's hosting
//! switch runs it): only it knows where it runs from on each system. On
//! Windows the switch needs an administrator, and as in the app, a switch
//! refused for want of one (exit code 740) is asked again through UAC.
//!
//! The rest read and change the host's data (helio-host-data) in this
//! process: its identity, its paired clients and what each may do. On
//! Windows the host's data folder is readable by administrators only, so
//! those need an administrator's terminal there: without one they say so
//! and exit with 740, before they touch the folder (only an administrator
//! may create it: `private`).

use std::io::Write;
use std::path::Path;
use std::process::ExitCode;

use helio_host_data::clients::Clients;
use helio_host_data::private;
use helio_proto::permission::Permissions;
use helio_transport::PublicIdentity;

use crate::args::HostCommand;
use crate::engine::{self, Switched};

pub fn run(command: HostCommand) -> ExitCode {
    match command {
        HostCommand::Enable => switch(true),
        HostCommand::Disable => switch(false),
        HostCommand::Status { json } => status(json),
        HostCommand::Identity => with_data("identity", identity),
        HostCommand::Clients => with_data("clients", list_clients),
        HostCommand::Permissions { x25519, set } => {
            with_data("permissions", |dir| permissions(dir, &x25519, set))
        }
        HostCommand::AddClient {
            name,
            x25519,
            mlkem,
            agent,
            permissions,
        } => with_data("add-client", |dir| {
            add_client(dir, &name, &x25519, &mlkem, agent, permissions)
        }),
        HostCommand::RemoveClient { x25519 } => {
            with_data("remove-client", |dir| remove_client(dir, &x25519))
        }
    }
}

/// Print `e` and fail.
fn fail(e: impl std::fmt::Display) -> ExitCode {
    eprintln!("{e}");
    ExitCode::FAILURE
}

/// The exit code a program gave, as this one's.
fn exit_code(code: Option<i32>) -> ExitCode {
    code.and_then(|c| u8::try_from(c).ok())
        .map(ExitCode::from)
        .unwrap_or(ExitCode::FAILURE)
}

/// `helio host enable` (`on`) or `disable`.
fn switch(on: bool) -> ExitCode {
    let Some(program) = engine::host_program() else {
        return fail(engine::no_program());
    };
    match engine::switch(&program, on) {
        Ok(said) => {
            println!("{said}");
            ExitCode::SUCCESS
        }
        Err(Switched::NeedsElevation) => {
            eprintln!(
                "Turning hosting {} needs an administrator: Windows asks.",
                if on { "on" } else { "off" }
            );
            match engine::switch_elevated(&program, on) {
                // What the elevated program printed is in a console of its
                // own, which no one sees.
                Ok(()) if on => {
                    println!("Hosting is on: the host runs from boot, before anyone signs in.");
                    ExitCode::SUCCESS
                }
                Ok(()) => {
                    println!("Hosting is off: the host is stopped, and no longer starts.");
                    ExitCode::SUCCESS
                }
                Err(e) => fail(e),
            }
        }
        Err(e) => fail(e),
    }
}

/// `helio host status [--json]`: what the host's program says.
fn status(json: bool) -> ExitCode {
    let Some(program) = engine::host_program() else {
        return fail(engine::no_program());
    };
    let mut command = engine::command(&program);
    command.arg("status");
    if json {
        command.arg("--json");
    }
    match command.output() {
        Ok(out) => {
            let _ = std::io::stdout().write_all(&out.stdout);
            let _ = std::io::stderr().write_all(&out.stderr);
            exit_code(out.status.code())
        }
        Err(e) => fail(format!("{}: {e}", program.display())),
    }
}

/// Run `then` on the host's data folder, where this process may: see the
/// module's doc.
fn with_data(command: &str, then: impl FnOnce(&Path) -> ExitCode) -> ExitCode {
    let dir = helio_host_data::data_dir();
    refuse_without_administrator(&dir, command);
    if let Some(said) = private::secure_data_dir(&dir) {
        eprintln!("{said}");
    }
    then(&dir)
}

/// Windows, the host's own data folder and no administrator: say so, and
/// exit with 740 there and then (an `ExitCode` holds a byte, and 740 is not
/// one).
#[cfg(windows)]
fn refuse_without_administrator(dir: &Path, command: &str) {
    // A folder chosen with HELIO_HOST_DATA_DIR is its chooser's own.
    if std::env::var_os("HELIO_HOST_DATA_DIR").is_some() || private::elevated() {
        return;
    }
    eprintln!(
        "helio host {command} needs an administrator: the host's data ({}) is readable by \
            administrators only. Run it again in an administrator terminal.",
        dir.display()
    );
    std::process::exit(engine::NEEDS_ELEVATION);
}

/// Elsewhere the host's data is its user's own.
#[cfg(not(windows))]
fn refuse_without_administrator(_: &Path, _: &str) {}

fn identity(dir: &Path) -> ExitCode {
    match helio_transport::Identity::load_or_create(&helio_host_data::identity_path(dir)) {
        Ok(id) => {
            let (x, m) = id.public().to_b64();
            println!("x25519 = \"{x}\"\nmlkem = \"{m}\"");
            ExitCode::SUCCESS
        }
        Err(e) => fail(e),
    }
}

/// A set of permissions as `helio host` prints it.
fn spoken(p: Permissions) -> String {
    if p == Permissions::NONE {
        "none".into()
    } else {
        p.names().join(",")
    }
}

fn list_clients(dir: &Path) -> ExitCode {
    for c in Clients::load(dir).list() {
        let role = if c.agent { "agent" } else { "person" };
        println!(
            "{}\t{}\t{role}\t{}",
            c.name,
            c.x25519,
            spoken(c.permissions)
        );
    }
    ExitCode::SUCCESS
}

/// What a running host makes of a change to its clients: it reads them
/// when it starts.
const RESTART: &str = "a running host takes it at its next start (helio host disable, then enable)";

fn permissions(dir: &Path, x25519: &str, set: Option<Permissions>) -> ExitCode {
    let mut clients = Clients::load(dir);
    let Some(p) = set else {
        return match clients.list().iter().find(|c| c.x25519 == x25519) {
            Some(c) => {
                println!("{}", spoken(c.permissions));
                ExitCode::SUCCESS
            }
            None => fail("no such client (helio host clients lists them)"),
        };
    };
    match clients.set_permissions(x25519, p) {
        Ok(Some(now)) => {
            println!("{}; {RESTART}", spoken(now));
            ExitCode::SUCCESS
        }
        Ok(None) => fail("no such client (helio host clients lists them)"),
        Err(e) => fail(e),
    }
}

fn add_client(
    dir: &Path,
    name: &str,
    x25519: &str,
    mlkem: &str,
    agent: bool,
    chosen: Option<Permissions>,
) -> ExitCode {
    let public = match PublicIdentity::from_b64(x25519, mlkem) {
        Ok(p) => p,
        Err(e) => return fail(e),
    };
    match Clients::load(dir).add(name, &public, None, agent, chosen) {
        Ok(c) => {
            println!("paired {name} ({}); {RESTART}", spoken(c.permissions));
            ExitCode::SUCCESS
        }
        Err(e) => fail(e),
    }
}

fn remove_client(dir: &Path, x25519: &str) -> ExitCode {
    match Clients::load(dir).remove(x25519) {
        Ok(Some(c)) => {
            println!("removed {}; {RESTART}", c.name);
            ExitCode::SUCCESS
        }
        Ok(None) => fail("no such client (helio host clients lists them)"),
        Err(e) => fail(e),
    }
}
