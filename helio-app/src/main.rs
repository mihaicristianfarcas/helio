//! Helio, the one app: play your other computers from this one (the host
//! list, pairing, streaming, AI agents), and, with hosting on, let your
//! devices play this one (This computer: the host's pages, its icon in the
//! menu bar or notification area). One app for macOS, Windows and Linux,
//! drawn with GPUI in asides' look (helio-ui). Streams open in their own
//! window, from helio-client; the host runs on its own (`helio-host`), started and
//! stopped by the hosting switch (`host::switch`).
//!
//! `--background` (at login, with Open at login): no window, just the
//! host's icon, when hosting is on; nothing at all, and the app quits, when
//! it is off.
//!
//! The app's program is also `helio`, the command line (helio-cli), so that
//! the command comes with Helio: started under that name
//! (`Contents/Resources/bin/helio` in the Mac app, the link Settings makes
//! on Linux), or with `cli` first (`bin\helio.exe` on Windows,
//! `helio-console`), it runs the command and nothing of the app
//! ([`command_line`]). Settings > General puts it on the PATH (`command`).

// No console window behind the app on Windows.
#![cfg_attr(windows, windows_subsystem = "windows")]

mod agents;
mod app;
mod chat;
mod command;
mod host;
mod hosting;
mod hosts;
mod model;
mod notify;
mod platform;
mod prefs;
mod settings;
mod setup;

use std::process::ExitCode;
use std::rc::Rc;

use gpui::{actions, App, KeyBinding, Menu, MenuItem};
use helio_ui::menus;
use helio_update::{Channel, Checker};

actions!(
    helio,
    [
        OpenSettings,
        ShowHosts,
        ShowAgents,
        ShowGeneral,
        ShowVideo,
        ShowAudio,
        ShowInput,
        ShowAgentSetup,
        ShowHostingSettings,
        ShowUpdates,
        ShowThisComputer,
        ShowDevices,
        Refresh,
        AddHost,
        Dismiss,
    ]
);

/// The command `helio` was given, when the app's program was started as
/// `helio`: under that name, exactly (the app's own is `Helio`, or
/// `helio-app`), or as `PROGRAM cli ARGS`. `args` is the whole command
/// line, the program first.
fn command_line(args: &[String]) -> Option<&[String]> {
    let (program, rest) = args.split_first()?;
    let name = std::path::Path::new(program).file_stem()?;
    if name == "helio" {
        Some(rest)
    } else {
        rest.split_first()
            .filter(|(first, _)| *first == "cli")
            .map(|(_, rest)| rest)
    }
}

/// Windows: the app's program is a window program, which Windows starts
/// without a console. Run as `helio` from a terminal (by `bin\helio.exe`),
/// it takes that terminal's console: Ctrl-C then reaches it (`helio stream`
/// ends properly), and what it runs (`helio-host`) prints there. Its own
/// output goes where the launcher's does already: the launcher hands its
/// standard handles on, and taking the console keeps them.
#[cfg(windows)]
fn take_console() {
    use windows::Win32::System::Console::{AttachConsole, ATTACH_PARENT_PROCESS};
    // SAFETY: no arguments but a constant; failing (no parent console: the
    // command run from Explorer) leaves the process as it was.
    let _ = unsafe { AttachConsole(ATTACH_PARENT_PROCESS) };
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    if let Some(command) = command_line(&args) {
        #[cfg(windows)]
        take_console();
        return helio_cli::main(command);
    }
    // `Helio mcp`: this device's agent as an MCP server (stdin/stdout), for
    // Claude Code, Codex, Claude Desktop and the rest.
    if args.get(1).map(String::as_str) == Some("mcp") {
        return helio_agent::mcp::main(&args[2..]);
    }
    // Linux: the stream's own process (see helio_client::linux).
    #[cfg(target_os = "linux")]
    if std::env::args().nth(1).as_deref() == Some("--helio-stream") {
        helio_client::linux::child_main();
    }
    // Started by an update: the old copy quits first.
    helio_update::install::after_update();
    let background = std::env::args().skip(1).any(|a| a == "--background");
    // One copy of the app: a second start shows the first one's window. A
    // check (HELIO_UI_DEMO) is not the app started again, and runs beside it.
    // Before the log is opened: opening it moves the last runs' logs aside,
    // the running copy's among them.
    let (show, mut shows) = futures::channel::mpsc::unbounded::<()>();
    let _instance = if std::env::var_os("HELIO_UI_DEMO").is_some() {
        None
    } else {
        match helio_ui::instance::claim(
            &helio_client::store::data_dir(),
            "helio",
            !background,
            move || {
                let _ = show.unbounded_send(());
            },
        ) {
            Some(instance) => Some(instance),
            None => {
                eprintln!("Helio is running already: it was asked to show itself");
                return ExitCode::SUCCESS;
            }
        }
    };
    helio_client::logging::init();
    let build = helio_update::Build::this();
    tracing::info!(
        version = build.version,
        commit = build.short_commit(),
        background,
        "Helio app"
    );
    // Join the DHT now, and keep hosts' internet addresses fresh and a path
    // warm to each while the app is open: connecting from outside the host's
    // network then takes a click, not seconds.
    helio_client::wan::warm_up();
    helio_client::wan::presence(helio_client::store::data_dir());

    let application = gpui_platform::application().with_assets(app::Assets);
    // The Dock icon brings the window back (it hides while a stream has
    // the screen).
    application.on_reopen(|cx| app::open(app::Show::Window, cx));
    application.run(move |cx: &mut App| {
        helio_ui::init(cx);
        // Notifications (a device asking to pair) are from the app.
        cx.set_app_identity(host::tray::APP_ID, "Helio");
        bind_keys(cx);
        let dir = helio_client::store::data_dir();
        let prefs = prefs::Prefs::load(&dir);
        // The update check is the app's: the window and the host's icon
        // both show what it found. A check (HELIO_UI_DEMO) asks GitHub
        // nothing on its own: what it shows is made up, or asked for.
        let channel = if std::env::var_os("HELIO_UI_DEMO").is_some() {
            Channel::Off
        } else {
            prefs.update_channel()
        };
        let (news, mut update_news) = futures::channel::mpsc::unbounded::<()>();
        let updates = Rc::new(Checker::start(build, dir, channel, move || {
            let _ = news.unbounded_send(());
        }));
        let hosting = host::switch::Hosting::start(cx);
        app::start(hosting.clone(), updates.clone(), cx);
        host::tray::start(hosting.clone(), updates, cx);
        // Closing the window quits, unless the host's icon stays (hosting
        // on): the app goes on behind it.
        cx.on_window_closed(|cx, _| {
            if !cx.windows().is_empty() {
                return;
            }
            let icon = host::tray::HostIcon::shown(cx);
            tracing::info!(stays = icon, "the window closed");
            if icon {
                // The host's icon is what is left of the app: no Dock tile
                // without a window.
                helio_ui::desktop::hide_from_dock();
            } else {
                cx.quit();
            }
        })
        .detach();
        // The update check's news and a second copy asking this one to show
        // itself: both arrive from other threads.
        cx.spawn(async move |cx| {
            use futures::StreamExt;
            while update_news.next().await.is_some() {
                cx.update(|cx| {
                    host::tray::HostIcon::refresh(cx);
                    app::update_news(cx);
                });
            }
        })
        .detach();
        cx.spawn(async move |cx| {
            use futures::StreamExt;
            while shows.next().await.is_some() {
                cx.update(|cx| app::open(app::Show::Window, cx));
            }
        })
        .detach();
        if background {
            app::stay_in_background(hosting, cx);
        } else {
            app::open(app::Show::Window, cx);
        }
    });
    ExitCode::SUCCESS
}

fn bind_keys(cx: &mut App) {
    let cmd = if cfg!(target_os = "macos") {
        "cmd"
    } else {
        "ctrl"
    };
    let build = helio_update::Build::this();
    menus::init(
        menus::AppMenus {
            name: "Helio",
            version: build.version.to_string(),
            build: if build.release {
                String::new()
            } else {
                build.short_commit().to_string()
            },
            help: "docs/usage.md",
            log: helio_client::logging::logs_dir().join("helio.log"),
        },
        cx,
    );
    cx.bind_keys([
        KeyBinding::new(&format!("{cmd}-,"), OpenSettings, None),
        KeyBinding::new(&format!("{cmd}-r"), Refresh, None),
        KeyBinding::new(&format!("{cmd}-n"), AddHost, None),
        KeyBinding::new(&format!("{cmd}-1"), ShowHosts, None),
        KeyBinding::new(&format!("{cmd}-2"), ShowThisComputer, None),
        KeyBinding::new(&format!("{cmd}-3"), ShowAgents, None),
        KeyBinding::new("escape", Dismiss, Some("HelioApp")),
    ]);
    // The menu bar, as a Mac app has it: the app's own menu, then File,
    // Edit, View, Window and Help.
    cx.set_menus([
        menus::app_menu("Helio", Some(MenuItem::action("Settings…", OpenSettings))),
        Menu::new("File").items([
            MenuItem::action("Add Host…", AddHost),
            MenuItem::action("Look for Hosts Again", Refresh),
            MenuItem::separator(),
            MenuItem::action("Close Window", menus::CloseWindow),
        ]),
        menus::edit_menu(),
        Menu::new("View").items([
            MenuItem::action("Hosts", ShowHosts),
            MenuItem::action("This Computer", ShowThisComputer),
            MenuItem::action("Devices", ShowDevices),
            MenuItem::action("Agents", ShowAgents),
            MenuItem::separator(),
            MenuItem::action("General", ShowGeneral),
            MenuItem::action("Video", ShowVideo),
            MenuItem::action("Audio", ShowAudio),
            MenuItem::action("Input", ShowInput),
            MenuItem::action("Agent Setup", ShowAgentSetup),
            MenuItem::action("Hosting", ShowHostingSettings),
            MenuItem::action("Updates", ShowUpdates),
        ]),
        menus::window_menu(),
        menus::help_menu("Helio"),
    ]);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(words: &[&str]) -> Vec<String> {
        words.iter().map(|w| w.to_string()).collect()
    }

    #[test]
    fn the_app_is_helio_under_that_name_or_with_cli_first() {
        let named = line(&["/opt/homebrew/bin/helio", "host", "status"]);
        assert_eq!(command_line(&named), Some(&named[1..]));
        let bare = line(&["helio"]);
        assert_eq!(command_line(&bare), Some(&bare[1..]));
        let windows = line(&[
            r"C:\Users\o\AppData\Local\Programs\Helio\Helio.exe",
            "cli",
            "hosts",
        ]);
        assert_eq!(command_line(&windows), Some(&windows[2..]));
        let linux = line(&["/home/o/.local/bin/helio-app", "cli", "--help"]);
        assert_eq!(command_line(&linux), Some(&linux[2..]));
    }

    #[test]
    fn the_app_started_as_itself_is_the_app() {
        for words in [
            &["/Applications/Helio.app/Contents/MacOS/Helio"][..],
            &[
                "/Applications/Helio.app/Contents/MacOS/Helio",
                "--background",
            ],
            &["helio-app", "mcp"],
            &["helio-app", "--helio-stream", "KEY", "{}"],
            &["Helio.exe", "--after-update", "1234"],
            // `cli` later on is an argument, not the command line.
            &["Helio", "mcp", "cli"],
        ] {
            assert_eq!(command_line(&line(words)), None, "{words:?}");
        }
    }
}
