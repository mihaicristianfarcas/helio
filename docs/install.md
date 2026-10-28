# Installing

pingpong is one app, **Ping**: the client, on every computer you sit at,
with the host, **Pong**, inside it, off until you turn hosting on. Install
it on both computers: the one you stream and the one you stream to. Every
system has it ready to [download](#from-the-website) from
[pingpong.mihaicristian.dev](https://pingpong.mihaicristian.dev/#install);
on a Mac it also installs with [Homebrew](#with-homebrew-macos), and
everywhere it builds from this repository with Rust.

| | macOS 14+ | Windows 10/11 | Linux |
|---|---|---|---|
| Ping | [Download](#on-macos), [Homebrew](#with-homebrew-macos) or [build](#ping-on-macos) | [Download](#on-windows) or [build](#ping-on-windows) | [Download](#on-linux) or [build](#ping-on-linux) |
| To host | [Hosting on macOS](#hosting-on-macos) | [Hosting on Windows](#hosting-on-windows) | [Hosting on Linux](#hosting-on-linux) |

Then [pair them](#pairing).

## From the website

[pingpong.mihaicristian.dev](https://pingpong.mihaicristian.dev/#install)
asks what your computer runs and gives you its download: the newest build
of the `main` branch, made by the release workflow
([development.md](development.md#releases)). Each is the whole app, the
host included. The files keep their addresses from one build to the next,
for scripts, under `https://download.pingpong.mihaicristian.dev/latest/`:

| macOS, Apple silicon | macOS, Intel | Windows, x86_64 | Linux, x86_64 |
|---|---|---|---|
| `Ping-macos-arm64.zip` | `Ping-macos-x86_64.zip` | `Ping-windows-x86_64.zip` | `Ping-linux-x86_64.tar.gz` |

`latest/SHA256SUMS` has their checksums, and `latest.json` says which
version and commit they are.

### On macOS

Open the zip and move Ping to Applications. It is signed with a Developer
ID and notarized by Apple, so macOS opens it as any app from the internet,
asking once whether you want to. To host, turn hosting on in Ping (setup
asks, or **Settings → Hosting**), as in [Hosting on macOS](#hosting-on-macos).

### On Windows

The programs are not code-signed yet, so Windows asks before it first runs
them, and names their publisher as unknown.

Extract the zip and double-click **Install Ping.cmd**. Windows asks whether
to run it. It installs Ping for you, with no administrator needed: Ping and
the host's engine (`pong.exe`) in `%LOCALAPPDATA%\Programs\Ping`, Ping in
the Start menu and in **Settings > Apps > Installed apps**, which
uninstalls it. Then it opens Ping. Ping also runs from the extracted folder
as it is: open **Ping.exe** there (it needs FFmpeg's DLLs and `pong.exe`
beside it).

To host, see what [Hosting on Windows](#hosting-on-windows) lists first
(an NVIDIA GPU, SudoVDA), then turn hosting on in Ping: Windows asks
whether Ping may make changes as an administrator, once to turn it on and
once to turn it off.

SmartScreen judges an unsigned program by the file itself, so it may ask
again after an update: each build is a new file. On a PC where **Smart
App Control** is on (some new installations of Windows 11), the programs
do not run at all: it blocks unsigned programs it does not know, with no
way to allow one. They run there only with Smart App Control turned off
(Windows Security > App & browser control > Smart App Control settings),
which is for whoever owns the PC to decide.

### On Linux

The programs are built on Ubuntu 24.04 and use the system's libraries as
it has them (FFmpeg 6, PipeWire): they run on Ubuntu 24.04 and the
distributions built on it. Elsewhere, [build from source](#building-from-source).

```sh
tar xzf Ping-VERSION-linux-x86_64.tar.gz
Ping-VERSION-linux-x86_64/install.sh
```

`install.sh` puts Ping (`ping-app`) and the host's engine (`pong`) in
`~/.local/bin`, Ping's launcher entry with its icon, and the host's user
service, installed but not turned on (see
[Hosting on Linux](#hosting-on-linux)). It names any library the system is
missing. Run from a newer download, it updates them, and restarts the
host if hosting is on. `install.sh --uninstall` turns hosting off and
removes what it installed.

## With Homebrew (macOS)

This repository is a Homebrew tap: its cask installs the app from the
latest release (Apple silicon and Intel).

```sh
brew tap mihaicristianfarcas/pingpong https://github.com/mihaicristianfarcas/pingpong
brew install --cask ping
```

To host, turn hosting on in Ping (setup asks, or **Settings → Hosting**):
the host then runs whenever you are logged in, and Ping's icon is in the
menu bar. macOS asks for Screen Recording and Accessibility for **Pong
Host**, the host inside Ping, the first time (see
[Hosting on macOS](#hosting-on-macos)).

`brew upgrade --cask ping` (or a plain `brew upgrade`) updates it, and so
does **Install and Restart** in Ping, which runs that command for you
([usage.md](usage.md#updates)). Either way hosting stays as it was: the
host sees the new app and restarts on it 15 s later. The app is signed
with a Developer ID and notarized by Apple.

## Building from source

Every system needs **Rust 1.96 or newer** (`rustup` from
[rustup.rs](https://rustup.rs) is the easy way) and git. The first build
downloads and compiles GPUI (Zed's UI framework) and a few hundred other
crates; expect several minutes.

```sh
git clone https://github.com/mihaicristianfarcas/pingpong
cd pingpong
```

What else each system needs:

- **macOS**: the Xcode Command Line Tools (`xcode-select --install`). Full
  Xcode is optional: with its Metal toolchain the app bundles carry their
  shaders compiled.
- **Windows**: the MSVC toolchain (Visual Studio Build Tools with "Desktop
  development with C++") and the Rust `x86_64-pc-windows-msvc` target, which
  `rustup` installs by default. The client also needs FFmpeg and LLVM (see
  [Ping on Windows](#ping-on-windows)).
- **Linux** (Ubuntu 24.04 package names; other distributions have
  equivalents; the host builds with FFmpeg 6, as on 24.04, and with 26.04's
  FFmpeg 8):

  ```sh
  sudo apt install build-essential pkg-config clang libclang-dev \
      libavcodec-dev libavutil-dev libswscale-dev libva-dev \
      libasound2-dev libpulse-dev libudev-dev \
      libx11-dev libx11-xcb-dev libxkbcommon-dev libxkbcommon-x11-dev libwayland-dev \
      libxcursor-dev libxrandr-dev libxi-dev libfontconfig-dev libfreetype-dev libxcb1-dev \
      libpipewire-0.3-dev libspa-0.2-dev libdbus-1-dev
  ```

  (`tools/linux/Dockerfile` is the authoritative list; it is what the
  project is built and tested with.)

## Ping on macOS

```sh
tools/build-ping-app              # builds target/Ping.app, the host inside it
tools/build-ping-app --install    # and copies it to /Applications
```

The first time Ping starts, macOS asks whether it may find devices on the
local network: allow it, or Ping cannot see or reach hosts on your LAN.

The build is signed ad hoc, or with a local development identity if you
created one (`tools/dev-signing-identity`, see
[development.md](development.md#macos-signing)).

## Hosting on macOS

The host comes inside the app: `Ping.app/Contents/Helpers/Pong Host.app`, a
helper app of its own. Turn hosting on in Ping (setup asks, or **Settings →
Hosting**): Ping runs the helper's `pong enable`, which copies the helper
to `~/Library/Application Support/Pong/Pong Host.app` and writes a
LaunchAgent that starts that copy at login and keeps it running
(`~/Library/LaunchAgents/dev.pingpong.Pong.plist`; see
[usage.md](usage.md#hosting-on-and-off)). Turning hosting off removes both. On first start the host asks for
**Screen Recording** (to capture) and **Accessibility** (to use the
keyboard and mouse); grant both to **Pong Host** in System Settings >
Privacy & Security. Ping itself never needs them. Without Accessibility,
clients can watch but not control.

A Ping run from `target/debug` or `target/release` uses the helper in the
`target/Ping.app` that `tools/build-ping-app` made. To run the host from a
terminal instead: `cargo run --release -p pong -- host` (the terminal then
needs those permissions). Details and limits:
[platforms/macos.md](platforms/macos.md).

## Ping on Windows

Ping's Windows build decodes with FFmpeg, so it needs:

- an **FFmpeg 8 shared build** with `include\`, `lib\` and `bin\` — for
  example BtbN's `ffmpeg-n8.1-latest-win64-lgpl-shared` from
  [github.com/BtbN/FFmpeg-Builds](https://github.com/BtbN/FFmpeg-Builds/releases);
- **LLVM** (for `libclang`, which generates the FFmpeg bindings), in
  `C:\Program Files\LLVM` or wherever `LIBCLANG_PATH` points.

```powershell
powershell -ExecutionPolicy Bypass -File tools\build-ping-win.ps1 -FFmpeg C:\path\to\ffmpeg
powershell -ExecutionPolicy Bypass -File tools\build-ping-win.ps1 -FFmpeg C:\path\to\ffmpeg -Install
```

(`-FFmpeg` can be left out when `FFMPEG_DIR` is set.) The first leaves
`target\Ping\Ping.exe` beside FFmpeg's DLLs and the host's engine,
`pong.exe`; `-Install` then installs them as a download's **Install
Ping.cmd** does (`tools\install-ping.ps1`): in
`%LOCALAPPDATA%\Programs\Ping`, the Start menu and Settings > Apps.
Windows Firewall may ask to let Ping on the network the first time: allow
private networks.

## Hosting on Windows

First, on the host PC:

1. An **NVIDIA GPU** and a current driver (Pong encodes with NVENC).
   Without one, Pong encodes H.264 in software, with Media Foundation's
   encoder (part of Windows): no HDR, 4:4:4, HEVC or AV1.
2. **SudoVDA**, the virtual display driver: installed by
   [Apollo](https://github.com/ClassicOldSong/Apollo), or from
   [SudoMaker/SudoVDA](https://github.com/SudoMaker/SudoVDA). If Apollo is
   installed, stop its service while Pong runs (they share the driver).
   Without it, Pong streams the PC's main display as it is.

   [platforms/windows.md](platforms/windows.md#without-nvenc-or-sudovda)
   says what a host without them can do.
3. Optional: **[ViGEmBus](https://github.com/nefarius/ViGEmBus)**, for
   controllers; **Steam** (its *Steam Streaming Speakers* device keeps the
   sound on the client only, as Sunshine does).

Then turn hosting on in Ping, or run `pong enable` from Ping's folder in an
**administrator** PowerShell:

```powershell
& "$env:LOCALAPPDATA\Programs\Ping\pong.exe" enable    # or target\Ping\pong.exe from a build
```

`pong enable` copies the host to `C:\Program Files\Pong`, and installs and
starts **PongService** from there (it starts at boot, before anyone signs
in, and adds Windows Firewall rules for Pong). Ping's own updates run it
again while hosting is on; after a build of your own, turn hosting off and
on (or run `pong enable` again) to put the new engine in place. `pong
disable` (as administrator) removes the service, its firewall rules and the
host's copy in `C:\Program Files\Pong`.

Pong's settings and state are in `C:\ProgramData\Pong`. Details:
[platforms/windows.md](platforms/windows.md).

## Ping on Linux

```sh
cargo build --release -p ping-app          # the app: target/release/ping-app
cargo build --release -p pong              # the host's engine, on its own
cargo build --release -p ping-core --bin ping   # the CLI, if you want it
```

Build the host's engine **on its own**, not in one `cargo` invocation with
the app: GPUI and Pong's desktop-portal code ask one dependency for
different async runtimes, and one build cannot have both.

Run `target/release/ping-app`, or install the app for your user as a
download's `install.sh` does: Ping with its launcher entry and icon, the
host's engine beside it, and the host's user service (not turned on):

```sh
tools/linux/install-apps
```

Details: [platforms/linux.md](platforms/linux.md).

## Hosting on Linux

Turn hosting on in Ping (or run `~/.local/bin/pong enable`: `systemctl
--user enable --now pong`). Pong runs in your desktop session, as you,
started at each login. Under Wayland, the desktop asks once whether Pong
may share the screen (on GNOME, switch on **Allow Remote Interaction** for
the keyboard and mouse). Ping's This computer pages show the host, and its
hosting switch runs `pong enable` and `pong disable` for you, finding
`pong` beside Ping or on the `PATH`. Data and logs are in `~/.config/pong`.
Details: [platforms/linux.md](platforms/linux.md).

## Pairing

1. Open Ping. Hosts on your local network appear on the **Hosts** page;
   others can be added by address (**Add Host**, or ⌘N / Ctrl+N).
2. Click the host: Ping shows a four-digit PIN.
3. On the host, open Ping (**This computer → Devices**) or its web UI
   (`https://HOST:47802`, whose certificate is self-signed) and type the
   PIN.

The first time you open the web UI it asks you to create its admin account.
Pairing is done once, on the local network; afterwards Ping connects from
anywhere ([networking.md](networking.md)). Next: [usage.md](usage.md).

## Uninstalling

| | |
|---|---|
| Homebrew | `brew uninstall --cask ping`: it quits Ping, and the host, finding the app gone, turns hosting off itself 15 s later (its LaunchAgent and copy removed); `--zap` stops it at once and also deletes the data folders, Ping's and the host's, and Ping's start at login |
| macOS | Turn hosting off in Ping (or run `pong disable` from inside it: `"/Applications/Ping.app/Contents/Helpers/Pong Host.app/Contents/MacOS/pong" disable`), quit Ping, then delete Ping.app. Deleted with hosting on, the host notices and, 15 s later, turns hosting off itself. Your data is in `~/Library/Application Support/Ping` and `~/Library/Application Support/Pong` (the host's identity and pairings) |
| Windows | **Settings > Apps > Installed apps**, **Ping**, **Uninstall** (or `install.ps1 -Uninstall` in `%LOCALAPPDATA%\Programs\Ping`). While hosting is on, it turns it off first (`pong disable`: PongService, its firewall rules and the host's copy), and Windows asks for an administrator. It leaves your hosts and settings, and the host's identity and pairings: delete `%APPDATA%\Ping` and `C:\ProgramData\Pong` too to remove them. A copy run from its extracted folder: turn hosting off in it, then delete the folder |
| Linux | `install.sh --uninstall` from the download, or `tools/linux/install-apps --uninstall`: it turns hosting off first (`pong disable`), then removes the programs, the host's unit and the launcher entry. Then `~/.config/ping` and `~/.config/pong` |

Unpair a device on the other side too (Ping: the host's menu > Unpair; the host:
**This computer → Devices**).
