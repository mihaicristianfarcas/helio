//! `helio host`: this computer as a host.
//!
//! The switch (`enable`, `disable`), `status` and `restart` are the host's
//! program's own (`helio-host`, found and run by `engine`, as the app's
//! hosting switch runs it): only it knows where it runs from on each
//! system. On Windows the switch needs an administrator, and as in the app,
//! a switch refused for want of one (exit code 740) is asked again through
//! UAC.
//!
//! The rest read and change the host's data (helio-host-data) in this
//! process: its identity, its paired clients and what each may do. On
//! Windows the host's data folder is readable by administrators only, so
//! those need an administrator's terminal there: without one they say so
//! and exit with 740, before they touch the folder (only an administrator
//! may create it: `private`). A running host reads its clients only when
//! it starts, so a change to them restarts it (`helio-host restart`: now,
//! or when its session ends), as `restart` does; the host's own API
//! needs the token only an administrator may read there too.

use std::io::Write;
use std::path::Path;
use std::process::ExitCode;

use helio_host_data::clients::Clients;
use helio_host_data::private;
use helio_proto::permission::Permissions;
use helio_transport::PublicIdentity;

use crate::args::HostCommand;
use crate::engine::{self, Switched};
use crate::style::{self, Look, To};

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
        HostCommand::Restart => with_data("restart", |_| restart()),
        HostCommand::Pair {
            pin,
            device,
            permissions,
        } => with_data("pair", |_| pair(&pin, device.as_deref(), permissions)),
    }
}

/// Print `e` and fail.
fn fail(e: impl std::fmt::Display) -> ExitCode {
    style::error(e);
    ExitCode::FAILURE
}

/// What a switch came to, its first clause (up to `:`) as what worked:
/// "Hosting is on: the host runs from boot, ...".
fn switched(said: &str) {
    let (done, rest) = said.split_once(':').unwrap_or((said, ""));
    let rest = if rest.is_empty() {
        String::new()
    } else {
        format!(":{rest}")
    };
    println!("{}{rest}", style::to(To::Out, Look::Good, done));
}

/// A change to the host's data: what it came to, and the running host
/// restarted to read it.
fn changed(what: &str) {
    println!("{what}");
    let Some(program) = engine::host_program() else {
        println!("{}", style::to(To::Out, Look::Quiet, AT_START));
        return;
    };
    match engine::command(&program).arg("restart").output() {
        Ok(out) if out.status.success() => {
            let said = String::from_utf8_lossy(&out.stdout);
            println!("{}", style::to(To::Out, Look::Quiet, said.trim()));
        }
        // No host runs: the one that starts reads the change.
        Ok(out) if out.status.code() == Some(i32::from(NOT_RUNNING)) => {}
        Ok(out) => {
            style::warn(engine::said(&out.stderr, &out.stdout));
            println!("{}", style::to(To::Out, Look::Quiet, AT_START));
        }
        Err(e) => {
            style::warn(format!("{}: {e}", program.display()));
            println!("{}", style::to(To::Out, Look::Quiet, AT_START));
        }
    }
}

/// `helio host restart`: what the host's program says. It exits with
/// [`NOT_RUNNING`] when no host runs, which is a failure here.
fn restart() -> ExitCode {
    ask_host(&["restart".to_string()])
}

/// `helio host pair PIN`: the host's program gives the PIN to the running
/// host (helio-host's `ask`).
fn pair(pin: &str, device: Option<&str>, permissions: Option<Permissions>) -> ExitCode {
    let mut args = vec!["pair".to_string(), pin.to_string()];
    if let Some(d) = device {
        args.extend(["--device".to_string(), d.to_string()]);
    }
    if let Some(p) = permissions {
        let names = p.names().join(",");
        let list = if names.is_empty() {
            "none".into()
        } else {
            names
        };
        args.extend(["--permissions".to_string(), list]);
    }
    ask_host(&args)
}

/// Run the host's program with `args`, which asks the running host, and
/// pass on what it says; any exit but 0 is a failure here.
fn ask_host(args: &[String]) -> ExitCode {
    let Some(program) = engine::host_program() else {
        return fail(engine::no_program());
    };
    match engine::command(&program).args(args).output() {
        Ok(out) => {
            let _ = std::io::stdout().write_all(&out.stdout);
            let _ = std::io::stderr().write_all(&out.stderr);
            match out.status.code() {
                Some(0) => ExitCode::SUCCESS,
                _ => ExitCode::FAILURE,
            }
        }
        Err(e) => fail(format!("{}: {e}", program.display())),
    }
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
            switched(&said);
            ExitCode::SUCCESS
        }
        Err(Switched::NeedsElevation) => {
            let asking = format!(
                "Turning hosting {} needs an administrator: Windows asks.",
                if on { "on" } else { "off" }
            );
            eprintln!("{}", style::to(To::Err, Look::Quiet, &asking));
            match engine::switch_elevated(&program, on) {
                // What the elevated program printed is in a console of its
                // own, which no one sees.
                Ok(()) if on => {
                    switched("Hosting is on: the host runs from boot, before anyone signs in.");
                    ExitCode::SUCCESS
                }
                Ok(()) => {
                    switched("Hosting is off: the host is stopped, and no longer starts.");
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
            let said = String::from_utf8_lossy(&out.stdout);
            if json || !style::colour(To::Out) {
                let _ = std::io::stdout().write_all(&out.stdout);
            } else {
                print!("{}", state_in_colour(&said));
            }
            let _ = std::io::stderr().write_all(&out.stderr);
            exit_code(out.status.code())
        }
        Err(e) => fail(format!("{}: {e}", program.display())),
    }
}

/// `helio-host status`'s line with its state in the colour of it: "on" (and
/// "on, but ..."), "starting", "failed: ...", "off".
fn state_in_colour(said: &str) -> String {
    let end = said
        .find(|c: char| !c.is_ascii_alphabetic())
        .unwrap_or(said.len());
    let (state, rest) = said.split_at(end);
    let look = match state {
        "on" if rest.starts_with(", but") => Look::Warn,
        "on" => Look::Good,
        "starting" => Look::Warn,
        "failed" => Look::Bad,
        _ => Look::Quiet,
    };
    format!("{}{rest}", style::paint(true, look, state))
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
    style::error(format!(
        "helio host {command} needs an administrator: the host's data ({}) is readable by \
            administrators only. Run it again in an administrator terminal.",
        dir.display()
    ));
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
    let rows: Vec<Vec<String>> = Clients::load(dir)
        .list()
        .iter()
        .map(|c| {
            let role = if c.agent { "agent" } else { "person" };
            vec![
                c.name.clone(),
                c.x25519.clone(),
                role.into(),
                spoken(c.permissions),
            ]
        })
        .collect();
    crate::list(&["NAME", "X25519", "WHO", "MAY"], &rows, |c, cell| {
        match (c, cell) {
            (0, _) => Some(Look::Strong),
            (3, "none") => Some(Look::Quiet),
            _ => None,
        }
    });
    ExitCode::SUCCESS
}

/// A change to the host's clients that the host could not be restarted
/// for: it reads them when it starts.
const AT_START: &str = "The host takes it when it next starts (helio host restart).";

/// `helio-host restart`'s exit code when no host runs (helio-host's
/// `hosting::NOT_RUNNING`).
const NOT_RUNNING: u8 = 3;

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
            changed(&spoken(now));
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
            changed(&format!(
                "{} {name} ({})",
                style::to(To::Out, Look::Good, "Paired"),
                spoken(c.permissions)
            ));
            ExitCode::SUCCESS
        }
        Err(e) => fail(e),
    }
}

fn remove_client(dir: &Path, x25519: &str) -> ExitCode {
    match Clients::load(dir).remove(x25519) {
        Ok(Some(c)) => {
            changed(&format!(
                "{} {}",
                style::to(To::Out, Look::Good, "Removed"),
                c.name
            ));
            ExitCode::SUCCESS
        }
        Ok(None) => fail("no such client (helio host clients lists them)"),
        Err(e) => fail(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_hosts_state_is_coloured_by_what_it_says_and_the_rest_left_as_it_is() {
        let painted = |look: Look, state: &str| style::paint(true, look, state);
        assert_eq!(
            state_in_colour("on; its API answers at https://localhost:47802\n"),
            format!(
                "{}; its API answers at https://localhost:47802\n",
                painted(Look::Good, "on")
            )
        );
        assert!(state_in_colour("on, but its API does not answer")
            .starts_with(&painted(Look::Warn, "on")));
        assert!(state_in_colour("starting\n").starts_with(&painted(Look::Warn, "starting")));
        assert_eq!(
            state_in_colour("failed: the host exited with code 3\n"),
            format!(
                "{}: the host exited with code 3\n",
                painted(Look::Bad, "failed")
            )
        );
        assert_eq!(
            state_in_colour("off\n"),
            format!("{}\n", painted(Look::Quiet, "off"))
        );
    }
}
