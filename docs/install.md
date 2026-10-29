# Installing

Helio is one app, **Helio**: the client, on every computer you sit at,
with the host, **Helio Host**, inside it, off until you turn hosting on. Install
it on both computers: the one you stream and the one you stream to. Every
system has it ready to [download](#from-the-website) from
[helio.mihaicristian.dev](https://helio.mihaicristian.dev/#install);
on a Mac it also installs with [Homebrew](#with-homebrew-macos), and
everywhere it builds from this repository with Rust.

| | macOS 14+ | Windows 10/11 | Linux |
|---|---|---|---|
| Helio | [Download](#on-macos), [Homebrew](#with-homebrew-macos) or [build](#helio-on-macos) | [Download](#on-windows) or [build](#helio-on-windows) | [Download](#on-linux) or [build](#helio-on-linux) |
| To host | [Hosting on macOS](#hosting-on-macos) | [Hosting on Windows](#hosting-on-windows) | [Hosting on Linux](#hosting-on-linux) |

Then [pair them](#pairing).

## From the website

[helio.mihaicristian.dev](https://helio.mihaicristian.dev/#install)
asks what your computer runs and gives you its download: the newest build
of the `main` branch, made by the release workflow
([development.md](development.md#releases)). Each is the whole app, the
host included. The files keep their addresses from one build to the next,
for scripts, under `https://download.helio.mihaicristian.dev/latest/`:

| macOS, Apple silicon | macOS, Intel | Windows, x86_64 | Linux, x86_64 |
|---|---|---|---|
| `Helio-macos-arm64.zip` | `Helio-macos-x86_64.zip` | `Helio-windows-x86_64.zip` | `Helio-linux-x86_64.tar.gz` |

`latest/SHA256SUMS` has their checksums, and `latest.json` says which
version and commit they are.

### On macOS

Open the zip and move Helio to Applications. It is signed with a Developer
ID and notarized by Apple, so macOS opens it as any app from the internet,
asking once whether you want to. To host, turn hosting on in Helio (setup
asks, or **Settings → Hosting**), as in [Hosting on macOS](#hosting-on-macos).

### On Windows

The programs are not code-signed yet, so Windows asks before it first runs
them, and names their publisher as unknown.

Extract the zip and double-click **Install Helio.cmd**. Windows asks whether
to run it. It installs Helio for you, with no administrator needed: Helio and
the host's engine (`helio-host.exe`) in `%LOCALAPPDATA%\Programs\Helio`, Helio in
the Start menu and in **Settings > Apps > Installed apps**, which
uninstalls it. Then it opens Helio. Helio also runs from the extracted folder
as it is: open **Helio.exe** there (it needs FFmpeg's DLLs and `helio-host.exe`
beside it).

To host, see what [Hosting on Windows](#hosting-on-windows) lists first
(an NVIDIA GPU, SudoVDA), then turn hosting on in Helio: Windows asks
whether Helio may make changes as an administrator, once to turn it on and
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
tar xzf Helio-VERSION-linux-x86_64.tar.gz
Helio-VERSION-linux-x86_64/install.sh
```

`install.sh` puts Helio (`helio-app`) and the host's engine (`helio-host`) in
`~/.local/bin`, Helio's launcher entry with its icon, and the host's user
service, installed but not turned on (see
[Hosting on Linux](#hosting-on-linux)). It names any library the system is
missing. Run from a newer download, it updates them, and restarts the
host if hosting is on. `install.sh --uninstall` turns hosting off and
removes what it installed.

## With Homebrew (macOS)

This repository is a Homebrew tap: its cask installs the app from the
latest release (Apple silicon and Intel).

```sh
brew tap mihaicristianfarcas/helio https://github.com/mihaicristianfarcas/helio
brew install --cask helio
```

To host, turn hosting on in Helio (setup asks, or **Settings → Hosting**):
the host then runs whenever you are logged in, and Helio's icon is in the
menu bar. macOS asks for Screen Recording and Accessibility for **Helio
Host**, the host inside Helio, the first time (see
[Hosting on macOS](#hosting-on-macos)).

`brew upgrade --cask helio` (or a plain `brew upgrade`) updates it, and so
does **Install and Restart** in Helio, which runs that command for you
([usage.md](usage.md#updates)). Either way hosting stays as it was: the
host sees the new app and restarts on it 15 s later. The app is signed
with a Developer ID and notarized by Apple.

## Building from source

Every system needs **Rust 1.96 or newer** (`rustup` from
[rustup.rs](https://rustup.rs) is the easy way) and git. The first build
downloads and compiles GPUI (Zed's UI framework) and a few hundred other
crates; expect several minutes.

```sh
git clone https://github.com/mihaicristianfarcas/helio
cd helio
```

What else each system needs:

- **macOS**: the Xcode Command Line Tools (`xcode-select --install`). Full
  Xcode is optional: with its Metal toolchain the app bundles carry their
  shaders compiled.
- **Windows**: the MSVC toolchain (Visual Studio Build Tools with "Desktop
  development with C++") and the Rust `x86_64-pc-windows-msvc` target, which
  `rustup` installs by default. The client also needs FFmpeg and LLVM (see
  [Helio on Windows](#helio-on-windows)).
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

## Helio on macOS

```sh
tools/build-helio-app             # builds target/Helio.app, the host inside it
tools/build-helio-app --install   # and copies it to /Applications
```

The first time Helio starts, macOS asks whether it may find devices on the
local network: allow it, or Helio cannot see or reach hosts on your LAN.

The build is signed ad hoc, or with a local development identity if you
created one (`tools/dev-signing-identity`, see
[development.md](development.md#macos-signing)).

## Hosting on macOS

The host comes inside the app: `Helio.app/Contents/Helpers/Helio Host.app`, a
helper app of its own. Turn hosting on in Helio (setup asks, or **Settings →
Hosting**): Helio runs the helper's `helio-host enable`, which copies the helper
to `~/Library/Application Support/Helio Host/Helio Host.app` and writes a
LaunchAgent that starts that copy at login and keeps it running
(`~/Library/LaunchAgents/dev.mihaicristian.helio.host.plist`; see
[usage.md](usage.md#hosting-on-and-off)). Turning hosting off removes both. On first start the host asks for
**Screen Recording** (to capture) and **Accessibility** (to use the
keyboard and mouse); grant both to **Helio Host** in System Settings >
Privacy & Security. Helio itself never needs them. Without Accessibility,
clients can watch but not control.

A Helio run from `target/debug` or `target/release` uses the helper in the
`target/Helio.app` that `tools/build-helio-app` made. To run the host from a
terminal instead: `cargo run --release -p helio-host -- host` (the terminal then
needs those permissions). Details and limits:
[platforms/macos.md](platforms/macos.md).

## Helio on Windows

Helio's Windows build decodes with FFmpeg, so it needs:

- an **FFmpeg 8 shared build** with `include\`, `lib\` and `bin\` — for
  example BtbN's `ffmpeg-n8.1-latest-win64-lgpl-shared` from
  [github.com/BtbN/FFmpeg-Builds](https://github.com/BtbN/FFmpeg-Builds/releases);
- **LLVM** (for `libclang`, which generates the FFmpeg bindings), in
  `C:\Program Files\LLVM` or wherever `LIBCLANG_PATH` points.

```powershell
powershell -ExecutionPolicy Bypass -File tools\build-helio-win.ps1 -FFmpeg C:\path\to\ffmpeg
powershell -ExecutionPolicy Bypass -File tools\build-helio-win.ps1 -FFmpeg C:\path\to\ffmpeg -Install
```

(`-FFmpeg` can be left out when `FFMPEG_DIR` is set.) The first leaves
`target\Helio\Helio.exe` beside FFmpeg's DLLs and the host's engine,
`helio-host.exe`; `-Install` then installs them as a download's **Install
Helio.cmd** does (`tools\install-helio.ps1`): in
`%LOCALAPPDATA%\Programs\Helio`, the Start menu and Settings > Apps.
Windows Firewall may ask to let Helio on the network the first time: allow
private networks.

## Hosting on Windows

First, on the host PC:

1. An **NVIDIA GPU** and a current driver (Helio Host encodes with NVENC).
   Without one, Helio Host encodes H.264 in software, with Media Foundation's
   encoder (part of Windows): no HDR, 4:4:4, HEVC or AV1.
2. **SudoVDA**, the virtual display driver: installed by
   [Apollo](https://github.com/ClassicOldSong/Apollo), or from
   [SudoMaker/SudoVDA](https://github.com/SudoMaker/SudoVDA). If Apollo is
   installed, stop its service while Helio Host runs (they share the driver).
   Without it, Helio Host streams the PC's main display as it is.

   [platforms/windows.md](platforms/windows.md#without-nvenc-or-sudovda)
   says what a host without them can do.
3. Optional: **[ViGEmBus](https://github.com/nefarius/ViGEmBus)**, for
   controllers; **Steam** (its *Steam Streaming Speakers* device keeps the
   sound on the client only, as Sunshine does).

Then turn hosting on in Helio, or run `helio-host enable` from Helio's folder in an
**administrator** PowerShell:

```powershell
& "$env:LOCALAPPDATA\Programs\Helio\helio-host.exe" enable  # or target\Helio\helio-host.exe from a build
```

`helio-host enable` copies the host to `C:\Program Files\Helio Host`, and installs and
starts **HelioHost** from there (it starts at boot, before anyone signs
in, and adds Windows Firewall rules for Helio Host). Helio's own updates run it
again while hosting is on; after a build of your own, turn hosting off and
on (or run `helio-host enable` again) to put the new engine in place. `helio-host
disable` (as administrator) removes the service, its firewall rules and the
host's copy in `C:\Program Files\Helio Host`.

Helio Host's settings and state are in `C:\ProgramData\Helio Host`. Details:
[platforms/windows.md](platforms/windows.md).

## Helio on Linux

```sh
cargo build --release -p helio-app         # the app: target/release/helio-app
cargo build --release -p helio-host        # the host's engine, on its own
cargo build --release -p helio-cli          # the CLI, if you want it
```

Build the host's engine **on its own**, not in one `cargo` invocation with
the app: GPUI and Helio Host's desktop-portal code ask one dependency for
different async runtimes, and one build cannot have both.

Run `target/release/helio-app`, or install the app for your user as a
download's `install.sh` does: Helio with its launcher entry and icon, the
host's engine beside it, and the host's user service (not turned on):

```sh
tools/linux/install-apps
```

Details: [platforms/linux.md](platforms/linux.md).

## Hosting on Linux

Turn hosting on in Helio (or run `~/.local/bin/helio-host enable`: `systemctl
--user enable --now helio-host`). Helio Host runs in your desktop session, as you,
started at each login. Under Wayland, the desktop asks once whether Helio Host
may share the screen (on GNOME, switch on **Allow Remote Interaction** for
the keyboard and mouse). Helio's This computer pages show the host, and its
hosting switch runs `helio-host enable` and `helio-host disable` for you, finding
`helio-host` beside Helio or on the `PATH`. Data and logs are in `~/.config/helio-host`.
Details: [platforms/linux.md](platforms/linux.md).

## Pairing

1. Open Helio. Hosts on your local network appear on the **Hosts** page;
   others can be added by address (**Add Host**, or ⌘N / Ctrl+N).
2. Click the host: Helio shows a four-digit PIN.
3. On the host, open Helio (**This computer → Devices**) or its web UI
   (`https://HOST:47802`, whose certificate is self-signed) and type the
   PIN.

The first time you open the web UI it asks you to create its admin account.
Pairing is done once, on the local network; afterwards Helio connects from
anywhere ([networking.md](networking.md)). Next: [usage.md](usage.md).

## Uninstalling

| | |
|---|---|
| Homebrew | `brew uninstall --cask helio`: it quits Helio, and the host, finding the app gone, turns hosting off itself 15 s later (its LaunchAgent and copy removed); `--zap` stops it at once and also deletes the data folders, Helio's and the host's, and Helio's start at login |
| macOS | Turn hosting off in Helio (or run `helio-host disable` from inside it: `"/Applications/Helio.app/Contents/Helpers/Helio Host.app/Contents/MacOS/helio-host" disable`), quit Helio, then delete Helio.app. Deleted with hosting on, the host notices and, 15 s later, turns hosting off itself. Your data is in `~/Library/Application Support/Helio` and `~/Library/Application Support/Helio Host` (the host's identity and pairings) |
| Windows | **Settings > Apps > Installed apps**, **Helio**, **Uninstall** (or `install.ps1 -Uninstall` in `%LOCALAPPDATA%\Programs\Helio`). While hosting is on, it turns it off first (`helio-host disable`: HelioHost, its firewall rules and the host's copy), and Windows asks for an administrator. It leaves your hosts and settings, and the host's identity and pairings: delete `%APPDATA%\Helio` and `C:\ProgramData\Helio Host` too to remove them. A copy run from its extracted folder: turn hosting off in it, then delete the folder |
| Linux | `install.sh --uninstall` from the download, or `tools/linux/install-apps --uninstall`: it turns hosting off first (`helio-host disable`), then removes the programs, the host's unit and the launcher entry. Then `~/.config/helio` and `~/.config/helio-host` |

Unpair a device on the other side too (Helio: the host's menu > Unpair; the host:
**This computer → Devices**).
