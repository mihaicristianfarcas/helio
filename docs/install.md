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

`helio`, the command line ([cli.md](cli.md)), comes inside the app on every
system, but no install puts it on your `PATH`: to type it in a terminal,
choose **Install** under **Command line** in Helio's **Settings → General**.
Uninstalling Helio takes it away again.

## From the website

[helio.mihaicristian.dev](https://helio.mihaicristian.dev/#install)
asks what your computer runs and gives you its download: the newest build
of the `main` branch, made by the release workflow
([development.md](development.md#releases)). Each is the whole app, the
host included. The files keep their addresses from one build to the next,
for scripts, under `https://download.helio.mihaicristian.dev/latest/`:

| macOS, Apple silicon | macOS, Intel | Windows, x86_64 | Linux, x86_64 |
|---|---|---|---|
| `Helio-macos-arm64.zip` | `Helio-macos-x86_64.zip` | `Helio-windows-x86_64-setup.exe` | `Helio-linux-x86_64.tar.gz` |

`latest/SHA256SUMS` has their checksums, and `latest.json` says which
version and commit they are. On Windows, `Helio-windows-x86_64.zip` holds
the same programs as the installer, unpacked; a copy run from it does not
update itself (Helio's own update runs the installer).

### On macOS

Open the zip and move Helio to Applications. It is signed with a Developer
ID and notarized by Apple, so macOS opens it as any app from the internet,
asking once whether you want to. To host, turn hosting on in Helio (setup
asks, or **Settings → Hosting**), as in [Hosting on macOS](#hosting-on-macos).

### On Windows

Download `Helio-windows-x86_64-setup.exe` and open it. It installs Helio
for you, with no administrator needed: Helio and the host's engine
(`helio-host.exe`) in `%LOCALAPPDATA%\Programs\Helio`, with `helio`, the
command line, in its `bin` folder; Helio in the Start menu and in
**Settings > Apps > Installed apps**, which uninstalls it. Hosting stays
off until you turn it on. Installed again over Helio, the installer
updates it in place.

The installer, its uninstaller and every program and DLL it installs are
signed with Helio's code-signing certificate (a Certum open-source
certificate), and each signature is timestamped, so it stays valid after
the certificate expires. **SmartScreen** may still say that it "prevented
an unrecognized app from starting" for a while: it trusts a publisher by
its reputation, which a certificate gathers as people download and run
what it signed. Choose **More info**: it names the publisher, and then
**Run anyway** runs the installer. If it says **Unknown publisher**, the
file is not one Helio released: delete it. **Smart App Control**, on a PC
where it is on, lets Helio run: it allows programs with a valid signature.

The first time Helio looks for hosts or connects, **Windows Firewall** asks
whether to let Helio on the network. Allow private networks. Windows asks
this once, for any program, since a program installed without an
administrator cannot add a firewall rule. The host's own rules come with
turning hosting on.

To host, turn hosting on in Helio ([Hosting on Windows](#hosting-on-windows)
says what the host uses): Windows asks whether Helio may make changes as an
administrator, once to turn it on and once to turn it off. Turning it on
also installs the drivers the host uses that the PC lacks, SudoVDA and
ViGEmBus; turning it off removes them again.

To check the download before you run it, compare its SHA-256 with the one
the site publishes (`latest/SHA256SUMS`) or GitHub shows beside the file on
the release page:

```powershell
Get-FileHash $HOME\Downloads\Helio-*-windows-x86_64-setup.exe -Algorithm SHA256
(Get-AuthenticodeSignature $HOME\Downloads\Helio-*-windows-x86_64-setup.exe).SignerCertificate.Subject
```

The second line names who signed it; `Status` (without the
`.SignerCertificate…`) is `Valid` for a signature Windows trusts.

### On Linux

The programs are built on Ubuntu 24.04 and use the system's libraries as
it has them (FFmpeg 6, PipeWire): they run on Ubuntu 24.04 and the
distributions built on it. Elsewhere, [build from source](#building-from-source).

```sh
tar xzf Helio-VERSION-linux-x86_64.tar.gz
Helio-VERSION-linux-x86_64/install.sh
```

`install.sh` puts Helio (`helio-app`) and the host's engine (`helio-host`)
in `~/.local/bin`, Helio's launcher entry with its icon, the host's own
entry (hidden from the launcher: KDE gives the host a virtual display only
through it), and the host's user service, installed but not turned on (see
[Hosting on Linux](#hosting-on-linux)). The libraries the programs need
that the system lacks, it installs with apt, which asks for your password
through `sudo`; run without a terminal (as Helio's own update runs it), it
names them instead. Run from a newer download, it updates them, and
restarts the host if hosting is on. `install.sh --uninstall` turns hosting
off and removes what it installed.

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
host restarts on the new app (15 s after a `brew upgrade` you run
yourself, when it sees the app replaced). The app is signed with a
Developer ID and notarized by Apple.

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
Hosting**, or `helio host enable` in a terminal): Helio runs the helper's
`helio-host enable`, which copies the helper
to `~/Library/Application Support/Helio Host/Helio Host.app` and writes a
LaunchAgent that starts that copy at login and keeps it running
(`~/Library/LaunchAgents/dev.mihaicristian.helio.host.plist`; see
[usage.md](usage.md#hosting-on-and-off)). Turning hosting off removes both. On first start the host asks for
**Screen Recording** (to capture) and **Accessibility** (to use the
keyboard and mouse); grant both to **Helio Host** in System Settings >
Privacy & Security. Helio itself never needs them. Without Accessibility,
clients can watch but not control. The host needs nothing installed
beside Helio: its virtual display, capture, encoder and sound tap are
macOS's own.

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
`helio-host.exe`; `-Install` then makes the release's installer from them
(`tools\windows\helio.iss`, which needs
[Inno Setup](https://jrsoftware.org/isinfo.php) 6.6 or newer) and runs it:
`%LOCALAPPDATA%\Programs\Helio`, the Start menu and Settings > Apps, as a
download installs them, unsigned. Windows Firewall may ask to let Helio on
the network the first time: allow private networks.

## Hosting on Windows

First, on the host PC:

1. An **NVIDIA GPU** and a current driver (Helio Host encodes with NVENC).
   Without one, Helio Host encodes H.264 in software, with Media Foundation's
   encoder (part of Windows): no HDR, 4:4:4, HEVC or AV1.
2. **SudoVDA**, the virtual display driver, and **ViGEmBus**, the
   controllers' driver: turning hosting on installs each one the PC does
   not have, and turning it off removes what it installed
   ([platforms/windows.md](platforms/windows.md#the-drivers-hosting-installs)).
   One already there, as Apollo's SudoVDA, is used as it is and left. SudoVDA
   is built for x64 PCs: on Windows on Arm, Helio Host streams the main
   display as it is
   ([platforms/windows.md](platforms/windows.md#without-nvenc-or-sudovda)).
   If Apollo runs, stop its service while Helio Host runs (they share the
   driver).
3. Optional: **Steam** (its *Steam Streaming Speakers* device keeps the
   sound on the client only, as Sunshine does). It is Steam's own, so Helio
   does not install it.

Then turn hosting on in Helio, or in a terminal with `helio` installed
(Windows asks for an administrator, as it does for the switch):

```powershell
helio host enable    # or target\Helio\bin\helio.exe host enable from a build
```

Turning hosting on copies the host's engine to `C:\Program Files\Helio Host`,
installs the drivers the PC lacks, and installs and starts **HelioHost**
from there (it starts at boot, before anyone signs in, and adds Windows
Firewall rules for Helio Host). What it says names what became of each
driver. Helio's own
updates put the new engine there while hosting is on; after a build of your
own, turn hosting off and on (or run `helio host enable` again) to put the
new engine in place. Turning it off (`helio host disable`) removes the service, its
firewall rules, the drivers it installed and the host's copy in
`C:\Program Files\Helio Host`.

Helio Host's settings and state are in `C:\ProgramData\Helio Host`. Details:
[platforms/windows.md](platforms/windows.md).

## Helio on Linux

```sh
cargo build --release -p helio-app         # the app: target/release/helio-app
cargo build --release -p helio-host        # the host's engine, on its own
cargo build --release -p helio-cli          # `helio` on its own (the app is `helio` too)
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

Turn hosting on in Helio (or run `helio host enable`: `systemctl --user
enable --now helio-host`). Helio Host runs in your desktop session, as you,
started at each login. On GNOME and KDE under Wayland, each session gets a
virtual display at the device's own size and rate, made by the desktop
itself, with nothing to answer; on GNOME it is the whole desktop for the
session, your own monitors off, unless **Keep this PC's monitors on while
streaming** is on. Elsewhere under Wayland, the desktop asks once whether
Helio Host may share the screen (switch on remote interaction, where the
dialog offers it, for the keyboard and mouse). Helio's This computer pages show the host. Its
hosting switch, as `helio host enable` and `disable` do, runs the host's
program, `helio-host`, which it finds beside Helio or on the `PATH`. Data and logs are in `~/.config/helio-host`.
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
| Homebrew | `brew uninstall --cask helio`: it quits Helio, `helio` goes with the app, and the host, finding the app gone, turns hosting off itself 15 s later (its LaunchAgent and copy removed); `--zap` stops it at once and also deletes the data folders, Helio's and the host's, Helio's start at login and `/etc/paths.d/helio` (the line that put `helio` on the `PATH`) |
| macOS | Turn hosting off in Helio (or run `helio host disable`), quit Helio, then delete Helio.app: `helio` goes with it. Deleted with hosting on, the host notices and, 15 s later, turns hosting off itself. Your data is in `~/Library/Application Support/Helio` and `~/Library/Application Support/Helio Host` (the host's identity and pairings), and `/etc/paths.d/helio` names the folder `helio` was in, if you installed it (`sudo rm /etc/paths.d/helio`) |
| Windows | **Settings > Apps > Installed apps**, **Helio**, **Uninstall** (or `unins000.exe` in `%LOCALAPPDATA%\Programs\Helio`). While hosting is on, it turns it off first (HelioHost, its firewall rules, the drivers hosting installed and the host's copy go), and Windows asks for an administrator; if you say no, Helio stays installed. It takes `helio` off your `PATH`, if you put it there, and Helio's start at login. It leaves your hosts and settings, the host's identity and pairings, and the firewall rule Windows made when it asked about Helio (only an administrator can remove it, and it lets nothing in once Helio is gone): delete `%APPDATA%\Helio` and `C:\ProgramData\Helio Host` too to remove them. A copy run from the extracted zip: turn hosting off in it, then delete the folder |
| Linux | `install.sh --uninstall` from the download, or `tools/linux/install-apps --uninstall`: it turns hosting off first, then removes the programs, the `helio` link (if you installed it), the host's unit and the launcher entries. Then `~/.config/helio` and `~/.config/helio-host` |

Unpair a device on the other side too (Helio: the host's menu > Unpair; the host:
**This computer → Devices**).
