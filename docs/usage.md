# Using Helio

Once [installed and paired](install.md), streaming is a click. This page is
the rest: the shortcuts, the settings on both sides, the clipboard, waking a
host, reaching it from away, and what to check when something does not work.

## Streaming

Click a paired host on Helio's **Hosts** page. The stream opens in a window of
its own — full screen by default — at the resolution, frame rate and bitrate
set under **Video**; the host sets up a display in exactly that mode for the
session. Helio's own window stays open beside it. The card's **…** menu (or a
right click) has more: **Steam Big Picture** (open Steam's Big Picture on
the host, and close it after), **Wake**, the host's **web UI**, **Unpair**.

While the stream has the keyboard and mouse, every key goes to the host —
system shortcuts included — except Moonlight's chords, **Ctrl+Alt+Shift**
and a key (Ctrl+Option+Shift on a Mac keyboard):

| Keys | Does |
|---|---|
| Ctrl+Alt+Shift+Q | Stop streaming |
| Ctrl+Alt+Shift+Z | Release the mouse and keyboard, or capture them again |
| Ctrl+Alt+Shift+X | Switch between full screen and a window |
| Ctrl+Alt+Shift+D | Minimise the stream |
| Ctrl+Alt+Shift+S | Show or hide the statistics |
| Ctrl+Alt+Shift+M | Switch the mouse between game mode (relative motion) and remote desktop mode (positions) |
| Ctrl+Alt+Shift+V | Type this computer's clipboard on the host |
| Ctrl+Alt+Shift+T | Watching an AI agent: take over its keyboard and mouse, or hand them back |

**A held key repeats** on the host as it does on your keyboard: Helio sends
the key once and tells the host your keyboard's repeat delay and rate (System
Settings > Keyboard on a Mac, the Keyboard control panel on Windows), and a
Windows or Mac host repeats the key at that pace until you let go, as
Sunshine does with its own fixed timings. A Linux host's desktop repeats
keys itself.

**The mouse works as in Moonlight.** The pointer you see is the host's
own, drawn into the picture by the host where its screen shows it: hidden
when a game hides it, wherever a game, the Steam overlay or a controller
puts it. Helio sends the mouse's relative motion, which is what games read,
and the host's pointer follows it with the host's own pointer speed.
Ctrl+Alt+Shift+M switches to sending positions instead (Moonlight's
"remote desktop" mouse), which suits the desktop and breaks most games;
press it again to go back.

**Screenshots.** Your computer's screenshot shortcuts stay your
computer's while you stream, and the tool they open gets the pointer:
- **Mac:** ⌘⇧3, ⌘⇧4, ⌘⇧5 and tools on ⌘⇧ shortcuts such as CleanShot X.
  Pressing ⌘⇧ gives the pointer to the Mac (a tool that comes up over the
  stream's pointer can neither show nor move its crosshair); a key that
  reaches the stream with them gives it back at once. When the tool is
  done, the stream takes the pointer back as soon as you move it over the
  stream, press a key or click.
- **Windows:** Print Screen with any modifiers, Win+Shift+S and
  Win+Shift+R (the Snipping Tool) go to Windows rather than to the host.
  The tool takes the focus, and the stream takes the pointer back when it
  has the focus again, or at a click.

To use the tool's own window afterwards (the screenshot's thumbnail,
CleanShot X's overlay), release the pointer first with Ctrl+Alt+Shift+Z
(Ctrl+Option+Shift+Z on a Mac).

**Controllers** plug in on the host as Xbox 360 pads (Windows, with
ViGEmBus), one per controller, with rumble; the Xbox/Guide button reaches
the host too. With **Controller as a mouse** on (Input settings), holding
Start switches the controller to driving the mouse, as in Moonlight.

**Taking over.** One person streams a host at a time. A second client asking
for the host takes over the session (the first is told) unless the host's
**Let a client take over a session** is off, in which case it is refused.

### The statistics

Ctrl+Alt+Shift+S (or `helio stream --stats`) shows Moonlight's figures —
incoming, decoded and rendered frame rates, bitrate, frames dropped,
network round trip, host processing, decode and render times — and one more:
**host capture to screen**, the time from the host capturing a frame to it
being on your display, measured with the two machines' clocks related
through the stream itself. A note in the corner warns while the network is
losing frames or lagging (**Connection warnings**).

## Helio's settings

Changes to the streaming settings apply to the next stream. ⌘, (Ctrl+, on
Windows and Linux) opens them.

| Page | Setting | What it does |
|---|---|---|
| General | Open at login | With hosting on, Helio's icon from the moment you log in, without its window (a LaunchAgent on a Mac, your `Run` key on Windows; not on Linux). With hosting off, nothing starts |
| | The helio command | **Install** puts `helio`, the command line, on the `PATH` of terminals opened from then on; **Remove** takes it off. How each system does it, and what uninstalling Helio does to it: [cli.md](cli.md) |
| | Set Up Again | Setup once more: what this computer is for, what it must allow, how to pair |
| Hosting | Let my devices play this computer | The hosting switch ([Hosting on and off](#hosting-on-and-off)), with what it needs: on a Mac, Screen Recording and Accessibility for the host; on Windows, whether there are NVIDIA graphics, and the drivers turning hosting on installs (SudoVDA, ViGEmBus) |
| Updates | Look for updates | New releases (the default for a packaged release), releases and main (the default for a build from a checkout: also commits on `main` newer than the build's), or off. See [Updates](#updates) |
| | Check for Updates | Asks GitHub now, whatever the setting |
| Video | Resolution | This display's native size (the default), the scaled desktop size (Mac), or a custom size. Each is fitted to the nearest standard aspect ratio (16:10, 16:9, 21:9...): a size between them makes games draw a smaller picture with strips around it, which a Windows host does not refresh. Below a MacBook's notch that is 16:10, 3024 × 1890 on a 14" |
| | Frame rate | Frames per second the host sends; up to this display's maximum |
| | Video codec | Automatic (HEVC when both ends can), AV1 (on a Mac that decodes it, an M3 or later: AV1 when the host's GPU encodes it, else HEVC or H.264), HEVC, or H.264 |
| | Display mode | Full screen or a window |
| | HDR | Stream in HDR (HEVC Main10, BT.2020, PQ), as Moonlight's "HDR": offered on a Mac whose display shows HDR, from a host that can (NVIDIA on Windows, macOS 15 or later). Off by default, as in Moonlight |
| | YUV 4:4:4 | Colour at full resolution: text without coloured fringes, for about a fifth more bitrate; offered where this computer decodes it (Apple silicon), from a host that encodes it (NVIDIA on Windows). Off by default, as in Moonlight |
| | Bitrate | Automatic (Moonlight's table for the resolution and frame rate), or a fixed figure |
| | V-Sync | Off: frames shown the moment they are decoded, tearing allowed |
| | Frame pacing | One frame per display refresh (Moonlight's pacer): smoother, about a refresh more latency. Off by default, as in Moonlight |
| | Performance statistics | The statistics from the start of each stream |
| | Connection warnings | The note in the corner |
| | Import from Moonlight | Copies Moonlight's own settings (its preferences on macOS, the registry on Windows, its `.conf` on Linux; never its keys) |
| Audio | Stream audio, Channels | Play the host's sound here: stereo, 5.1 or 7.1 (a Mac host has surround only with a surround output: see [platforms/macos.md](platforms/macos.md#sound-in-surround)) |
| | Play on the host too | Otherwise the host's speakers stay quiet during a session (Windows host with Steam Streaming Speakers) |
| | Mute in the background | Silence the stream while its window is not in front |
| Input | Use ⌘ as the Windows key | On a Mac: Command reaches a Windows host as the Windows key (a Mac host always gets Command) |
| | Share the clipboard | See [below](#sharing-the-clipboard) |
| | Controller as a mouse | Hold Start to drive the mouse with a controller |
| | Swap mouse buttons | The left button clicks right on the host, and the right one left (Moonlight's option) |
| | Reverse scrolling | The wheel and the trackpad scroll the host the other way (Moonlight's option) |

Settings are kept in `settings.toml` in Helio's data folder, beside the paired
hosts (`hosts.toml`) and this device's keys:

| System | Helio's data folder |
|---|---|
| macOS | `~/Library/Application Support/Helio` |
| Windows | `%APPDATA%\Helio` |
| Linux | `~/.config/helio` (`$XDG_CONFIG_HOME/helio`) |

`HELIO_DATA_DIR` points Helio (and `helio`, `helio-agent`) elsewhere.

## Hosting on and off

Hosting is one switch, and it is off until you turn it on: in Helio, in
**Settings → Hosting** (or in setup, by answering that your devices will
play this computer). Off, nothing of the host runs on
your computer: no port is open for streaming, pairing or the web UI,
nothing is published for finding the host from the internet, and it is not
announced on the local network. Helio, the client, opens none of these.

From a terminal it is `helio host`, which runs what the switch runs
([cli.md](cli.md#helio-host--this-computer-as-a-host)):

```sh
helio host enable   # on: the host starts now, and every time after
helio host disable  # off: the host stops, and no longer starts
helio host status   # off, starting, on or failed (with why)
```

Both run the host's own program, `helio-host`, which comes with Helio.
Turning hosting on or off does this on each system:

| | On (`helio host enable`) | Off (`helio host disable`) |
|---|---|---|
| macOS | Copies the app bundle the host's program is in (`Helio.app/Contents/Helpers/Helio Host.app`) to `~/Library/Application Support/Helio Host/Helio Host.app`, and writes the LaunchAgent `~/Library/LaunchAgents/dev.mihaicristian.helio.host.plist`, which runs the host from that copy whenever you are logged in, and starts it now. The host's program outside an app bundle refuses: macOS's Screen Recording and Accessibility permissions belong to that helper app, not to Helio (and for Screen Recording, only to a copy outside Helio.app: [platforms/macos.md](platforms/macos.md#hosting-from-helio-and-its-menu-bar-icon)) | Stops the host, deletes the LaunchAgent and the copy |
| Windows | As administrator: copies `helio-host.exe` to `C:\Program Files\Helio Host` (only administrators can change that folder), checks its signature, installs the drivers the PC lacks (SudoVDA, ViGEmBus: [platforms/windows.md](platforms/windows.md#the-drivers-hosting-installs)), and runs it as **HelioHost**, which starts at boot, before anyone signs in, with Windows Firewall rules for it | As administrator: stops and deletes HelioHost, its firewall rules, the drivers it installed and the copy in `C:\Program Files\Helio Host` |
| Linux | `systemctl --user enable --now helio-host`: the host runs in your desktop session, now and at each login (the unit comes with the install, not turned on) | `systemctl --user disable --now helio-host` |

On Windows the service runs as SYSTEM, the only account that can show and
control the sign-in screen and UAC prompts. That is why it runs only from
its own copy in Program Files, never from a folder you (or a program you
run) can write to, and why turning hosting on or off asks for an
administrator. Without one, the host's program changes nothing and exits
with code 740, and Helio (the switch, or `helio host`) asks Windows to run
it again elevated (the UAC prompt). In a release, turning hosting on
refuses an engine that is unsigned, was changed after signing, or is
signed by anyone else; a build of your own only warns.

On a Mac the host asks for Screen Recording and Accessibility with macOS's
own prompts as it first starts, as **Helio Host**: System Settings lists it,
not Helio, under both. Helio's Hosting page says, for each, whether
macOS allows it, with a button to its pane of System Settings, and notices
when it is allowed. After Screen Recording is turned on there, macOS asks
to quit and reopen the host.

Quitting Helio never stops the host: only the switch does. Uninstalling Helio
turns hosting off first ([install.md](install.md#uninstalling)). Your
pairings and settings stay in the data folder either way.

## This computer and the web UI

With hosting on, Helio's **This computer** pages and **Settings → Hosting**
and the host's web UI (`https://HOST:47802`) do the same things: see and
end the session, pair and unpair devices, set what each may do, change the
host's settings, read its log. This computer is for the host's own screen; the web UI for a
headless host or another computer. On Windows, Helio signs in to the host
once with the web UI's admin account.

**Helio's icon** sits in the menu bar on a Mac and in the taskbar's
notification area on Windows while hosting is on, with or without the
window open. Its menu says what the host is doing (ready, streaming to
whom, starting, stopped), opens the window, names a device asking to pair
(a notification says so too), and says when a newer Helio exists. With the
icon there, closing the window leaves the app running behind it (on a Mac,
without a Dock tile until the window is open again); **Quit Helio** in its
menu quits the app. Either way the host keeps running: it is a service of
its own (HelioHost, the LaunchAgent, or the user unit). Linux has no
single tray to put an icon in, so there closing the window quits Helio, and
the host runs on.

| Page | Setting | Default | `config.toml` |
|---|---|---|---|
| General | Name (as clients see it) | the computer's name | `name` |
| | Let a client take over a session (a device also needs **Take over** in its permissions) | on | `allow_takeover` |
| | Keep this PC's monitors on while streaming | off: the virtual display is the whole desktop (Windows, macOS, GNOME; on KDE it extends the desktop either way) | `keep_host_displays` |
| | Share the clipboard with clients | on | `clipboard` |
| Video | NVENC preset | P1 (fastest), as Sunshine | `nvenc_preset` |
| | Two-pass encoding | on | `nvenc_two_pass` |
| | Full GPU power for Helio Host (NVIDIA, after a restart) | on | `nvidia_max_power` |
| | OpenGL and Vulkan through DXGI (NVIDIA, system-wide while Helio Host runs, after a restart) | on | `nvidia_dxgi_present` |
| | Allow HEVC, Allow AV1 | on | `allow_hevc`, `allow_av1` |
| | Allow HDR, Allow YUV 4:4:4 (for a client that asks) | on | `allow_hdr`, `allow_yuv444` |
| | Maximum bitrate, maximum frame rate | the client's choice | `max_bitrate_kbps`, `max_fps` |
| | Send pacing | 800 Mbit/s | `pace_mbps` |
| | Adapt the bitrate to the network | on | `adaptive_bitrate` |
| Network | Allow streaming over the internet (off: the host is on the local network only, and not on the DHT at all) | on | `internet_access` |
| | Ask the router to forward the port | on | `port_mapping` |
| | Ports: tunnel (UDP), pairing, web UI | 47800, 47801, 47802 | `port`, `pairing_port`, `web_port` |
| AI agents | Allow paired AI agents | on | `agents` |
| | Hold after local input | 10 s | `agent_local_input_hold_secs` |

The groups are the web UI's. In Helio, with hosting on, each setting is on
the settings page of its kind, after Helio's own: the name, taking over and
the Network group in **Settings → Hosting**, under the switch; keeping the
monitors on and the Video group in **Settings → Video** ("Streaming from
this PC", "What devices may ask of this PC"); the clipboard in **Settings →
Input**; the AI agents group in **Settings → Agent setup**. A computer that
only plays shows none of them. A changed name or port applies after the
host restarts (the page offers to).
Helio Host's data folder holds `config.toml`, its identity, the paired clients,
the web UI's certificate and account, and `logs/`:

| System | Helio Host's data folder |
|---|---|
| Windows | `C:\ProgramData\Helio Host` |
| macOS | `~/Library/Application Support/Helio Host` |
| Linux | `~/.config/helio-host` |

`HELIO_HOST_DATA_DIR` points Helio Host elsewhere.

## What each device may do

Each paired device has permissions on the host, as Apollo gives each of its
clients: whether it may see the screen, which input the host takes from it,
which way the clipboard goes, and more. Set them in Helio, This computer →
**Devices** (the button beside each device) or in the web UI under
**Clients**. A change applies at once, to a stream that is running too.

| Permission | Lets the device | Name (`helio host permissions`) |
|---|---|---|
| See the screen | Stream the screen and sound. Off: the device stays paired, but is turned away | `view` |
| Start apps | Open an app with the stream, such as Steam Big Picture | `launch` |
| Take over | Stream while another device does, ending that stream (when **Let a client take over a session** is on) | `take_over` |
| Keyboard | Type, and press keys and shortcuts | `keyboard` |
| Mouse | Move the pointer, click and scroll | `mouse` |
| Controllers | Play with game controllers (on a Windows host) | `controller` |
| Copy from this computer | What is copied on the host can be pasted on the device | `clipboard_read` |
| Paste to this computer | What is copied on the device can be pasted on the host | `clipboard_write` |
| Watch AI agents | Watch an agent's session, pause or stop it, and take over from it with the keyboard or mouse | `watch` |

An AI agent can have **See the screen**, **Keyboard** and **Mouse**, and one
of its own: **Act while nobody watches** (`unwatched`). Without it, the
agent waits until a person watches its session
([ai-agents.md](ai-agents.md#the-rules-the-host-holds-agents-to)).

**When a device pairs**, the PIN step offers a choice: **Everything**, **See
and control** (keyboard, mouse and controllers, nothing else) or **See
only**; for an agent, **See and control**, **Only while watched** or **See
only**. It starts from what the device gets otherwise: the first device
paired with the host may do everything, and later ones see only, as Apollo
gives its first client every permission and later ones only watching.
Choose **Everything** for a device of your own. An agent may see and act.

What the device hears:

- Helio says what the host holds back as a stream starts, and when it
  changes: "gaming-pc ignores your keyboard, mouse and controllers: this
  device may not use them there."
- A device that may not stream, start the app it asked for, or watch an
  agent is refused with the reason. A stream whose **See the screen** is
  taken away ends, and says why.
- Input the device may not send is dropped by the host as it arrives; a
  key it holds down when the keyboard is taken away is let go.
- The clipboard goes only the ways the device may: its copies are not even
  read on the side that may not send them. Sharing that was off when a
  stream started comes on with the next stream.

From a terminal: `helio host permissions X25519 view,keyboard,mouse`
([cli.md](cli.md#helio-host--this-computer-as-a-host)); a running host takes
that change at its next start.

## Updates

Helio says when there is something newer than the copy that runs: a line at
the foot of the sidebar ("Helio 1.0.1 is available"), and, while hosting is
on, its icon's menu. Clicking it says what it is, with
**Install and Restart**: the app downloads the release from GitHub, checks
it, installs it and opens again. Nothing is downloaded or installed until
you choose to. For a build from a checkout it lists the commits on `main`
it does not have instead.

How each system installs it:

| | The app | The host, while hosting is on |
|---|---|---|
| macOS | The app is replaced where it is (Applications), the host inside it too | Restarts at once, on the new app's helper |
| macOS, installed with Homebrew | `brew upgrade --cask helio`, run for you | Restarts at once, on the new app's helper |
| Windows | The release's installer runs again over Helio, silently and for you only (no administrator), then Helio opens again; Settings > Apps shows the new version | Windows asks for an administrator's permission when the update starts; the new engine then replaces the host's copy in `C:\Program Files\Helio Host`, and HelioHost restarts on it. Declined, nothing is installed. A PC that does not host is never asked |
| Linux | The new release's `install.sh` (into `~/.local/bin`) | The host's user service restarts on the new engine |

The download is checked against the SHA-256 GitHub records for the
release's file, then for who made it: on a Mac the new app must be signed
with the same team's Developer ID as the one it replaces, and on Windows
the installer with Helio's code-signing certificate. A
copy built from source, or one not installed the way the downloads
install it (a Windows copy unpacked by hand), does not update itself:
download those updates yourself. Your hosts, pairings and settings, the
host's identity and its paired devices, and whether hosting is on all
stay as they were. A stream that runs ends when the app restarts (and
while hosting is on, streams to this computer stop while the host
restarts). If an update does not install, the app says why when it opens
again; nothing of the copy that runs is changed, and **Try Again** or the
release's page are left.

They ask GitHub's public API, without an account: a few seconds after
starting when the last answer is more than a day old, and once a day while
they run. What they send is what any HTTPS request shows (this computer's
address) and the version in the `User-Agent`. **Look for updates: Off**
stops it; **Check for Updates** (in the app's menu, and in General) still
asks once. The last answer is kept in `update.toml` in the app's data
folder.

## Sharing the clipboard

Copy on one side, paste on the other: text, images, and files and folders,
both ways, for as long as your stream runs. Helio asks for it (**Share the
clipboard**, on by default; `helio stream --no-clipboard` for one stream),
and the host agrees unless its **Share the clipboard with clients** is off,
the ways the device's permissions allow (**Copy from this computer**,
**Paste to this computer**; see
[What each device may do](#what-each-device-may-do)).

- Each end watches its own clipboard and sends each new copy: files if there
  are some, else text, else an image. Helio also sends what is on its
  clipboard as the stream starts, so what you copied before connecting
  pastes on the host. Files arrive in a temporary folder
  (`Helio Clipboard` or `Helio Host Clipboard`, replaced by the next copy) and are
  put on the clipboard as files, so pasting in Finder or Explorer copies
  them from there.
- Copies travel inside the tunnel, chunked and acknowledged, under a rate
  cap of half the stream's bitrate so the video goes first; up to 256 MB.
- **Password managers' copies stay where they were made**: anything marked
  concealed is never sent.
- AI agents' sessions and people watching them never share the clipboard.

Ctrl+Alt+Shift+V still types the clipboard's text on the host, as Moonlight
does, for places that do not take a paste.

## Wake-on-LAN

Helio Host announces its network adapters on the local network, and Helio
remembers them, so an offline host can be woken: **Wake** in the host's
menu, a click on an offline host, or `helio wake NAME`. The host must be on a
wired connection with Wake-on-LAN enabled: in its firmware (often called
"Resume by PCI-E device" or "Wake on LAN"; with ErP off), and in the network
adapter's settings (power-saving features such as Energy-Efficient Ethernet
off). Helio must have seen the host on the local network at least once.

## Away from home

Nothing to set up: with **Allow streaming over the internet** on (the
default), a paired Helio finds the host through the DHT and connects directly
through both NATs, from anywhere. The first connection of a session can take
a few seconds; while Helio is open it keeps the path warm. If both networks
use symmetric NAT, forward UDP 47800 to the host on its router. How it
works: [networking.md](networking.md).

The host's card says **Online** away from home too. For up to a minute and
a half after Helio starts or your computer changes networks, it can say
**Checking…** while the host opens its path to you; a click streams all the
same.

## When something does not work

- **Helio finds no hosts (macOS).** Allow Helio on the local network: System
  Settings > Privacy & Security > Local Network.
- **Windows asks about the firewall.** Allow Helio (and `helio.exe`) on private
  networks. The host's firewall rules are added when hosting is turned on.
- **"The host did not answer."** Is hosting on there (Helio's **Settings →
  Hosting** on the host; on Windows, `HelioHost` in Services)? Is this device still paired (the host's
  **Devices**)?
- **The host ignores your keyboard or mouse**, and Helio says so: this
  device may not use them there. In Helio on the host, under **This computer
  → Devices**, give it **Keyboard** and **Mouse** (or choose **Everything**).
- **"… does not let this device stream."** Its **See the screen**
  permission is off: turn it on in Helio on the host, under **This computer → Devices**.
- **"… did not answer this device's AI agent."** The agent is paired on its
  own, apart from Helio: if Helio still streams from the host, Helio Host no longer
  has the agent. Pair it again with **Allow…** on Helio's **Agents** page.
- **"The host could not create its virtual display"** (Windows): SudoVDA did
  not install (turn hosting off and on: `helio host enable` says what
  Windows answered; on Windows on Arm it never does), or Apollo's service is
  holding it — stop ApolloService.
- **A KDE host streams its screen scaled, not a display of the device's
  size**: KWin did not give Helio Host its virtual output. It does only for
  a `helio-host` named in a desktop entry that asks for it, which Helio's
  `install.sh` puts in `~/.local/share/applications`: install with it, and
  host with the `helio-host` it installed.
- **A Mac host ignores the keyboard and mouse**, or streams nothing: give
  Helio Host Accessibility and Screen Recording in System Settings > Privacy
  & Security, then turn hosting off and on.
- **A Linux host under Wayland shows a dialog every time** (a desktop other
  than GNOME and KDE), or ignores input:
  answer the desktop's sharing dialog with **Allow Remote Interaction** on;
  the restore token then saves asking again.
- **Stutter every 20–30 s on a Mac on Wi-Fi**: AWDL (AirDrop, Continuity) is
  a common cause; see [benchmarks.md](benchmarks.md#long-sessions-and-loss).
- **Logs** say what happened (never a PIN, password, key or token):
  [logs.md](logs.md) says where they are, how to turn on debug logging, and
  what to send with a bug report.

The command line (`helio`, and `helio-agent` for AI agents) is in [cli.md](cli.md).
