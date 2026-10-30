# The window (GPUI)

One app, Helio, with one window, drawn with
[GPUI](https://github.com/zed-industries/zed/tree/main/crates/gpui), Zed's UI
framework. Everyone gets the client; hosting is a switch in it, off by
default. Its look is asides' (`asides-overlay`'s `controls.rs`, which started
from `helio-ui`): glass the window blurs what is behind it through, warm
grays in dark mode and white in light mode, one hairline weight, a glass pill
behind what is selected that slides to the next, settings as cards of rows
that each say what they do, colour kept for status (green live, amber
attention, red danger), and asides' soft blue (`#c2d6f6`) for the one main
action on a page. Light and dark follow the system
(`HELIO_APPEARANCE=light|dark` overrides it).

- **`helio-ui`**: the theme (`Theme`, `Ink`, `Radius`, `Type`,
  `Metrics`), the glass (`glass`), how things move (`motion`: what opens
  fades in 140 ms, rising a few points; a width eases in 100 ms; both eased
  out, cubic; `Glide`, the sliding pill), the keyboard (`focus`: Tab order,
  the ring shown only after a key moved the focus, arrows in groups, the
  scroll bar), the window's parts (`chrome`: sidebar, rows, pages), one
  line-icon family (24×24, 1.75 pt round strokes), the controls (buttons,
  icon buttons, switch, segmented control, select menu, slider, stepper,
  text field with IME, menus, sheets, cards and setting rows, choice rows,
  chips, status dots, spinner, keycaps, stats, notices), and Markdown
  rendering for the agents' words. Also what the app needs of the desktop:
  the menu bar a Mac app has (`menus`), the tray icon (`tray`), one running
  copy per data folder (`instance`), starting at login (`login`), the About
  panel and the Dock (`desktop`), and the update check's notice, sheet and
  settings (`updates`, over `helio-update`).
- **`helio-app`**: the app. `host/` is This computer: the host's pages, a
  client of its web API (`api.rs`, `worker.rs`, `link.rs`), the hosting
  switch (`switch.rs`) and the host's icon (`tray.rs`).

## The glass, on each system

| System | Window | Surfaces |
|---|---|---|
| macOS | `WindowBackgroundAppearance::Blurred`: an `NSVisualEffectView` under GPUI's view blurs what is behind the window; GPUI's Metal pipelines blend alpha "over" (`glass::blend_alpha_over`), so what fades in over the glass never hides the blur first | The theme's tints over the blur, exactly asides' |
| Windows 11 (22H2, build 22621, and later) | The system's acrylic backdrop (`DWMSBT_TRANSIENTWINDOW`): GPUI is asked for Mica, which makes it draw the window see-through over a system backdrop, then `glass::dress` makes that backdrop acrylic. GPUI's own blur, the older acrylic, thins out where the window meets the screen's edge, until what is behind shows nearly sharp there | The same tints |
| Windows 10 (1809 and later), Windows 11 21H2 | `Blurred`: GPUI's acrylic (`SetWindowCompositionAttribute`) | The same tints |
| Linux, Wayland, where the compositor offers `org_kde_kwin_blur_manager` (KWin with its blur, compositing with OpenGL) | `Blurred`: KDE's `org_kde_kwin_blur`. The app asks the compositor's registry once, at the start | The same tints |
| Linux, elsewhere (GNOME, X11, KWin drawing with QPainter) | Opaque: GPUI has no blur there, and a "blurred" window would only be see-through | The same colours, solid (`Theme::solid_surfaces`) |

No GPUI shadows on glass: under a translucent element a shadow fills under
it too, and turns it near black. Menus, tooltips and sheets are solid, and
keep a soft shadow.

The stream's window is not GPUI's and has none of this: it stays opaque,
with no blur and no animation, and nothing here runs as a frame is
presented. `blend_alpha_over` changes only pipelines that add alpha; the
stream's renderer draws video without blending, and its overlays blend
alpha "over" already.

## The window

The title bar is the window's own strip along the top, over the glass:
where nothing is on it, it moves the window. On a Mac the traffic lights sit
over the sidebar; on Windows the app draws minimize, maximize and close at
the top right in Windows' own glyphs (`chrome::caption_buttons`), and
Windows handles them as its own (snap layouts on maximize, the double-click
maximizing), since its title bar would sit opaque and dark above the glass.
Linux keeps the system's title bar.

A sidebar on the glass, as asides': **Hosts**; **This computer**; **Agents**
(**New Session**, the Agents page) and the open **Sessions**; the
**Settings** pages; at its foot the version,
or "Looking for hosts…", and above it a line when a newer Helio exists. The
pages are one stop for Tab, the arrows going up and down them, each shown as
the keyboard comes to it; each page fades in as it is picked, as the pill
slides to it. ⌘1, ⌘2 and ⌘3 go to Hosts, This computer and Agents, ⌘,
to Settings (Ctrl on Windows and Linux).

On a Mac the menu bar is a Mac app's: **Helio** (About, Check for Updates,
Settings, Services, Hide, Quit), **File** (Add Host ⌘N, Look for Hosts
Again ⌘R, Close Window ⌘W), **Edit** (cut, copy and paste in the text
fields), **View** (the pages), **Window** and **Help** (the documentation,
the logs, reporting an issue).

- **Hosts**: a card per host with its status, this computer's own host left
  out. Hovering says what a click does (Stream, Wake, Pair); the card's "…"
  or a right click has the rest (Steam Big Picture, Wake, the web UI,
  Unpair). Under the cards, what a click streams at, and how to change it.
- **This computer**: while hosting is off, one page (**Overview**) that says
  so, what hosting is, and leads to Settings → Hosting, where the switch
  is. While it is on, the host's pages:
  - **Overview**: a pairing request as a banner when there is one; the
    session (who, the mode, End Session; an agent's with Pause, Hand Back,
    Stop) with its numbers (frames, bitrate, host latency, round trip,
    loss, recoveries); this host (ID, tunnel port, internet address, the
    router's port mapping, devices, version); the agent's recent actions;
    Sign Out when the window signed in with an account (Windows).
  - **Devices**: pairing requests first, each with its PIN field (Return
    pairs) and what the device may do once paired; then the paired devices
    and agents, each with a button that names what it may do and opens the
    sheet that sets each permission, and unpairable.
  - **Logs**: the last 400 lines of the host's log, following its end.
- **Settings**: Helio's own pages, **Hosting** with the switch and what the
  system must allow for it. While hosting is on, the host's own settings
  follow Helio's on the page of their kind (`HostPages::settings_for`): its
  name, taking over and network on Hosting, its encoder and limits on
  Video, its clipboard on Input, what agents may do on it on Agent setup.
  They save as they change (text fields on Return); encoder settings a Mac
  host does not have (NVENC, AV1) are not shown. Each setting is in one
  place, and a computer that only plays has none of the host's.

Every page has the same column (`Metrics::PAGE`), and what is on it fills
the column. On every system a page's actions (Add Host, Copy) sit beside
its title, not in the strip along the top: on Windows the caption buttons
end that strip, and on Linux the system's title bar is above it.
- **Agents** starts agent sessions; **Sessions** lists them and your own
  desktop streams ([ai-agents.md](ai-agents.md#2-work-with-it-sessions)).
- **Settings**, each row naming a setting and saying in a line what it does:
  **General** (opening at login, setting up again), **Video**, **Audio**,
  **Input**, **Agent setup**, **Hosting** (the switch, what it needs, This
  computer a click away) and **Updates** ([usage.md](usage.md#updates)).
- **A stream** opens in a window of its own (see [usage.md](usage.md)).
  A stream window that goes away without ending its stream is noticed
  within a quarter of a second, and the stream ends with it.

### Setup

The first time the app opens (and from Settings → General → **Set Up
Again**), setup takes the whole window, in asides' setup style: its steps
along the top, the step, and Back, Skip and Continue at the bottom.

1. **Welcome**: Set Up, or Skip and keep the defaults.
2. **What do you want to do on this computer?** Play my other computers
   from here (the client only), Let my devices play this one, or Both.
3. **Permissions**, for hosting only: the switch, and what the system must
   allow, each said in a line before it is asked. On a Mac, Screen
   Recording and Accessibility for the host (Helio Host), which macOS asks for as
   the host starts, each with a button to its pane of System Settings and
   marked Allowed as soon as the host says so. On Windows, the administrator
   UAC asks for once, whether there are NVIDIA graphics (else it streams
   in software H.264), and SudoVDA (else the main display, scaled) and
   ViGEmBus (else no controllers), which turning hosting on installs.
4. **Pairing**: in a line, how to pair from the other computer.
5. **Done**.

The step reached is saved as it is reached (`setup_step` in the settings),
so setup goes on from there after a restart. Skipped, the defaults stay
(play only, hosting as it was), and each permission is asked for when it is
first needed. Playing only never asks for the screen, the input, or an
administrator.

### Hosting, and the app's icon

The switch runs the host's own commands: `helio-host enable`, `helio-host disable` and
`helio-host status --json`, found and run by `helio_cli::engine`, as `helio host`
runs them ([cli.md](cli.md#helio-host-the-hosts-program)). The host is `Helio Host.app` on a
Mac, inside this app's bundle under `Contents/Helpers` (for a build run
from `target/`, inside `target/Helio.app`), `helio-host.exe` beside the app or in
`C:\Program Files\Helio Host` on Windows, and `helio-host` beside the app or on the
`PATH` on Linux. On Windows,
when `helio-host` says it needs an administrator (exit code 740), the app runs it
again through UAC and says it waits for Windows meanwhile. The state is
asked again when something says it may have changed (the app starting, a
switch made, the host's API coming or going, the window coming forward),
never on a timer.

While hosting is on, the app shows the host's icon, in the menu bar on a Mac
and the notification area on Windows: what the host is doing, a device
asking to pair (also a system notification, unless the window is in front),
Open Helio, Check for Updates, Quit Helio. With the icon there, closing the
window leaves the app running behind it (on a Mac, without a Dock tile until
the window opens again); with hosting off, closing the window quits.
Quitting the app never stops the host. Linux has no tray: closing the window
quits there, and the host runs on.

With the window open the host is asked every second; with only the icon
looking, every three seconds, and not for its devices or the agent's log.

### How This computer reaches the host

Over the host's web API, HTTPS on localhost, trusting only the certificate
the host made for itself (`web-cert.pem` in its data folder), with a bearer
token:

- **The local token.** The host writes a fresh one at every start
  (`local-token` in its data folder), readable only by its own user on a
  Mac or Linux, where the host runs as that user. The app uses it and asks
  nothing.
- **An app token, on Windows.** The host is a service there, and its token
  is readable by SYSTEM and Administrators only, so an app that is not
  elevated **signs in once** with the web UI's admin account (creating it
  if there is none yet). It gets an app token, which it keeps in the user's
  profile (`%APPDATA%\Helio Host\app-token`) and the host keeps hashed
  (`app-tokens.toml`). Signing out drops it on both sides.

`POST /api/app-token` (user, password, optional `setup` to create the
account, a name) and `DELETE /api/app-token` are the API's only
app-specific endpoints; every other endpoint takes the bearer token as it
takes the web UI's cookie. `/api/status` says, on a Mac, what macOS allows
the host (`grants`: Screen Recording, Accessibility).

## Logs

Each program writes its own, and never a PIN, password, key or token:
[logs.md](logs.md) says where, and how to turn on debug logging. The app
keeps its own under `helio_client::logging::logs_dir()`; This computer's
**Logs** page reads the host's through its API.

## GPUI

GPUI is a git dependency pinned to one Zed revision (in the workspace
`Cargo.toml`). Its Metal shaders are compiled when the app starts (the
`runtime-shaders` feature, on by default), so a build needs only the Command
Line Tools; the app-bundle scripts compile them ahead of time when Xcode's
Metal toolchain is installed. `vendor/ztracing*` are no-op stand-ins
(Apache-2.0) for Zed's GPL-licensed profiling shim, which GPUI names but does
not use. On Linux GPUI needs fontconfig, freetype, xkbcommon, Wayland and
xcb development packages (see [install.md](install.md#building-from-source)).

## Checking it without clicking

`HELIO_UI_DEMO` drives the window through a list of steps and can save it as
a PNG (on a Mac, `screencapture` of the window), so a change to the UI can
be checked, and compared with before, without a person:

```sh
HELIO_UI_DEMO=settings=video,snapshot=/tmp/video.png,quit target/debug/helio-app
HELIO_UI_DEMO=agents=sample,snapshot=/tmp/session.png,quit target/debug/helio-app
HELIO_UI_DEMO=setup=use,snapshot=/tmp/setup.png,quit target/debug/helio-app
HELIO_UI_DEMO=hosting=on,host-sample,host=devices,snapshot=/tmp/devices.png,quit target/debug/helio-app
HELIO_UI_DEMO=hosting=on,host-sample,host=devices,permissions=macbook,open=permissions-preset,snapshot=/tmp/menu.png,quit target/debug/helio-app
```

The steps: `settings[=general|video|audio|input|agents|hosting|updates]`,
`agents[=setup|sample|sample-live|sample-ask]`, `chat=MESSAGE` (to the open
session, or a new one on the first host the agent may use), `login`,
`pair=HOST:PORT`, `add`, `alone` (hosts found on the network are left
out: the Hosts page as it is with none), `hosts-sample` (made-up hosts
instead of the real ones, one card of each state), `size=WxH` (the window that size,
in points, to see a long page whole), `menu=NAME`, `open=ID` (the select
with that id, open: `fps`, `codec`, `permissions-preset`), `unpair=NAME`,
`stream=NAME`, `desktop` (that stream's page, while it runs),
`stop-after=SECS`, `wait=SECS`, `close-login`, `vanish-login`, `step=N`,
`panel-end`, `update` and `update-main` (the update check says there is a
newer release, or newer commits on main), `installs` (this copy installs
updates, as a release does), `installing` and `install-failed` (an
installation under way, or one that did not take), `install` (installs the
latest release, for real, as **Install and Restart** does), `updates` (its
sheet), `menus` (the menu bar and the host's icon's menu, to the log),
`snapshot=PATH`, `close` (the window closes as its button closes it; with
hosting on, the app stays behind its icon), `quit`,
`command=on|off|elsewhere|taken|unavailable|busy` (General's Command line
row in that state, made up: its button then changes nothing on the
system), `command-install` and `command-remove` (its button pressed, for
real: `helio` on or off the PATH).

Setup and hosting: `setup[=welcome|use|permissions|pairing|done]` (setup
open at that step), `hosting=off|on|starting|failed|switching|elevating`
(hosting made up, and the switch runs nothing), `switch=on|off` (the switch
turned, for real), `hosting-page` (This computer with hosting off).

This computer's host: `host=overview|devices|logs` (its pages; its
settings are on the settings pages, `settings=hosting|video|input|agents`), `host-sample` (a made-up session, request, devices and
settings), `host-signin`, `host-setup`, `host-offline` (the real host made
signed out, without an account yet, or not answering),
`signin-as=USER:PASSWORD`, `permissions=NAME` (the permissions sheet of the
paired device called NAME). `HELIO_HOST_DATA_DIR` points the app at a test host's
data folder; `HELIO_HOST_APP_URL`, `HELIO_HOST_APP_CERT` and `HELIO_HOST_APP_TOKEN` at another
host altogether; `HELIO_HOST_PROGRAM` at the `helio-host` the switch runs.
`HELIO_UI_DEMO_SCREEN=PATH` gives the sample sessions a screenshot to show.
`tools/capture-docs-images` makes the README's pictures this way;
`tools/demo-gif/record` makes its GIF, from a real stream.

With `HELIO_UI_DEMO` set, the update check asks GitHub nothing on its own
(`update` makes up its answer), the app runs beside a copy that is already
running rather than showing it, and the window names Helio and Helio Host
without their version, so a screenshot does not date with the next release.

Use an empty `HELIO_DATA_DIR` for checks, so they neither read nor change
your own hosts and settings (`onboarded = true` in its `settings.toml` skips
setup, which an empty folder opens on), and a throwaway `HELIO_HOST_DATA_DIR` with
`internet_access = false` and `port_mapping = false` in its `config.toml`
before turning the switch on for real. An occluded window is not redrawn, so
the demo driver brings its window to the front first. A snapshot is the
window alone, with nothing behind its glass: dark pages come out lighter
than on screen. To compare with asides, capture the screen where the window
is (`screencapture -R`) over the same desktop.
