//! Ping, the one app: play your other computers from this one (the host
//! list, pairing, streaming, AI agents), and, with hosting on, let your
//! devices play this one (This computer: the host's pages, its icon in the
//! menu bar or notification area). One app for macOS, Windows and Linux,
//! drawn with GPUI in asides' look (pingpong-ui). Streams open in their own
//! window, from ping-core; the host runs on its own (`pong`), started and
//! stopped by the hosting switch (`host::switch`).
//!
//! `--background` (at login, with Open at login): no window, just the
//! host's icon, when hosting is on; nothing at all, and the app quits, when
//! it is off.

// No console window behind the app on Windows.
#![cfg_attr(windows, windows_subsystem = "windows")]

mod agents;
mod app;
mod chat;
mod host;
mod hosting;
mod hosts;
mod model;
mod notify;
mod platform;
mod prefs;
mod settings;
mod setup;

use std::rc::Rc;

use gpui::{actions, App, KeyBinding, Menu, MenuItem};
use pingpong_ui::menus;
use pingpong_update::{Channel, Checker, Program};

actions!(
    ping,
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

fn main() {
    // `Ping mcp`: this device's agent as an MCP server (stdin/stdout), for
    // Claude Code, Codex, Claude Desktop and the rest.
    if std::env::args().nth(1).as_deref() == Some("mcp") {
        let args: Vec<String> = std::env::args().skip(2).collect();
        let code = ping_agent::mcp::main(&args);
        std::process::exit(if code == std::process::ExitCode::SUCCESS {
            0
        } else {
            1
        });
    }
    // Linux: the stream's own process (see ping_core::linux).
    #[cfg(target_os = "linux")]
    if std::env::args().nth(1).as_deref() == Some("--ping-stream") {
        ping_core::linux::child_main();
    }
    // Started by an update: the old copy quits first.
    pingpong_update::install::after_update();
    let background = std::env::args().skip(1).any(|a| a == "--background");
    // One copy of the app: a second start shows the first one's window. A
    // check (PING_UI_DEMO) is not the app started again, and runs beside it.
    // Before the log is opened: opening it moves the last runs' logs aside,
    // the running copy's among them.
    let (show, mut shows) = futures::channel::mpsc::unbounded::<()>();
    let _instance = if std::env::var_os("PING_UI_DEMO").is_some() {
        None
    } else {
        match pingpong_ui::instance::claim(
            &ping_core::store::data_dir(),
            "ping",
            !background,
            move || {
                let _ = show.unbounded_send(());
            },
        ) {
            Some(instance) => Some(instance),
            None => {
                eprintln!("Ping is running already: it was asked to show itself");
                return;
            }
        }
    };
    ping_core::logging::init();
    let build = pingpong_update::Build::this();
    tracing::info!(
        version = build.version,
        commit = build.short_commit(),
        background,
        "Ping app"
    );
    // Join the DHT now, and keep hosts' internet addresses fresh and a path
    // warm to each while the app is open: connecting from outside the host's
    // network then takes a click, not seconds.
    ping_core::wan::warm_up();
    ping_core::wan::presence(ping_core::store::data_dir());

    let application = gpui_platform::application().with_assets(pingpong_ui::Assets);
    // The Dock icon brings the window back (it hides while a stream has
    // the screen).
    application.on_reopen(|cx| app::open(app::Show::Window, cx));
    application.run(move |cx: &mut App| {
        pingpong_ui::init(cx);
        // Notifications (a device asking to pair) are from the app.
        cx.set_app_identity(host::tray::APP_ID, "Ping");
        bind_keys(cx);
        let dir = ping_core::store::data_dir();
        let prefs = prefs::Prefs::load(&dir);
        // The update check is the app's: the window and the host's icon
        // both show what it found. A check (PING_UI_DEMO) asks GitHub
        // nothing on its own: what it shows is made up, or asked for.
        let channel = if std::env::var_os("PING_UI_DEMO").is_some() {
            Channel::Off
        } else {
            prefs.update_channel()
        };
        let (news, mut update_news) = futures::channel::mpsc::unbounded::<()>();
        let updates = Rc::new(Checker::start(
            Program::Ping,
            build,
            dir,
            channel,
            move || {
                let _ = news.unbounded_send(());
            },
        ));
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
                pingpong_ui::desktop::hide_from_dock();
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
}

fn bind_keys(cx: &mut App) {
    let cmd = if cfg!(target_os = "macos") {
        "cmd"
    } else {
        "ctrl"
    };
    let build = pingpong_update::Build::this();
    menus::init(
        menus::AppMenus {
            name: "Ping",
            version: build.version.to_string(),
            build: if build.release {
                String::new()
            } else {
                build.short_commit().to_string()
            },
            help: "docs/usage.md",
            log: ping_core::logging::logs_dir().join("ping.log"),
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
        KeyBinding::new("escape", Dismiss, Some("PingApp")),
    ]);
    // The menu bar, as a Mac app has it: the app's own menu, then File,
    // Edit, View, Window and Help.
    cx.set_menus([
        menus::app_menu("Ping", Some(MenuItem::action("Settings…", OpenSettings))),
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
        menus::help_menu("Ping"),
    ]);
}
