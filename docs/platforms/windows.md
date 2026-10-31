# Windows

Helio Host on Windows is the host Helio was built around: what Sunshine is to
Moonlight, with Apollo's virtual display. Helio also runs on Windows 10 and 11.

## Hosting: Helio Host

### What it needs

| | Why |
|---|---|
| Windows 10 or 11, 64-bit | |
| An NVIDIA GPU with NVENC, and its driver | Encoding. NVENC is loaded from the driver (`nvEncodeAPI64.dll`) at runtime: no CUDA toolkit or Video Codec SDK is needed to build or run. Without it, Helio Host encodes H.264 in software ([below](#without-nvenc-or-sudovda)) |
| [SudoVDA](https://github.com/SudoMaker/SudoVDA) (SudoMaker Virtual Display Adapter) | A virtual display at each client's mode. Turning hosting on installs it if the PC has none, on x64 PCs (it is built for x64 only) ([below](#the-drivers-hosting-installs)). Helio Host and Apollo cannot stream at the same time (they share the driver). Without it, Helio Host streams the host's main display ([below](#without-nvenc-or-sudovda)) |
| [ViGEmBus](https://github.com/nefarius/ViGEmBus) | Controllers: each of the client's pads becomes a virtual Xbox 360 pad. Turning hosting on installs it if the PC has none ([below](#the-drivers-hosting-installs)) |
| Steam's **Steam Streaming Speakers** (optional) | Sound only on the client: during a session the host's default output moves to this virtual device, so the host's speakers stay quiet (Sunshine's approach, its `virtual_sink`). Without it, the sound plays on the host too. Steam's own: Helio does not install it |

### The drivers hosting installs

`helio-host enable` (the hosting switch), which runs as an administrator
anyway, installs the two drivers Helio Host uses that the PC lacks, before
it starts the service, as Apollo's installer installs them
(`windows_nsis.cmake`) — so a PC that only plays never gets either
(`helio-host/src/hosting/drivers.rs`):

1. **SudoVDA 1.10.9.289**, the build Apollo 0.4.6 ships, comes inside
   `helio-host.exe` ([vendor/sudovda](../../vendor/sudovda/README.md) says
   where from, with its hashes and licenses). It is written to
   `C:\Program Files\Helio Host\drivers\sudovda`; its publisher's
   certificate, `CN=sudovda@su.mk`, is added to the machine's **Trusted Root**
   and **Trusted Publishers** stores (Windows installs a user-mode driver
   signed by its author only from a publisher it trusts; the certificate is
   for code signing only); a root device with the hardware ID
   `Root\SudoMaker\SudoVDA` is made, and the driver installed on it — what
   Apollo's `install.bat` does with `nefconc`, here with SetupAPI. Not on
   Windows on Arm.
2. **ViGEmBus 1.22.0** is its author's signed installer, in `drivers\`
   beside `helio-host.exe` (`tools/windows/drivers.ps1` puts it in the
   package, pinned by its SHA-256). It is copied into the engine's folder,
   checked against the same SHA-256 there, where only administrators can
   change it, and run without a window (`/exenoui /qn /norestart`).

A driver already on the PC — Apollo's SudoVDA, a ViGEmBus another program
installed — is used as it is. What `enable` installed it writes down
(`drivers\installed.toml` in the engine's folder), and `helio-host
disable` removes exactly that once the service has stopped: SudoVDA's
device, its driver package and the certificate, and ViGEmBus's Windows
Installer product. So turning hosting off leaves the PC's drivers as they
were before hosting; a program installed meanwhile that relies on a
ViGEmBus Helio installed loses it then. Neither driver is needed to host:
one that does not install is a line in what `enable` says (and the Hosting
page says it is missing), not a failure.

Measured in a Windows 11 Arm virtual machine (UTM on an M4 Pro), from a
PC with neither driver: `enable` took 19 s, most of it ViGEmBus's
installer; `disable` 3 s, ViGEmBus's uninstall included. On an x64 PC
(Windows 11 25H2, NVIDIA RTX 3070 Ti) with ViGEmBus 1.21 already there and
no SudoVDA, `enable` installed SudoVDA in 3 s, which then made and removed
a 1920x1080 monitor; `disable` took its device, driver package and
certificate away again in 1 s, and left ViGEmBus as it was. With
Apollo's SudoVDA there, `enable` and `disable` left it as it was.

### How it runs

`HelioHost` (LocalSystem, automatic start) launches `helio-host host` into the
active console session with a copy of its SYSTEM token, and relaunches it
when the console session changes (sign-out, fast user switching, a reboot to
the sign-in screen) or the host exits: at once when the host stopped to
start again (a changed name, port or NVIDIA setting; it exits with code 75
for that), else 3 s later. Only SYSTEM can open the secure
desktop, so only a host running this way can capture and control UAC
prompts, the lock screen and the sign-in screen — the model Sunshine uses
(`helio-host/src/service.rs`).

Because it runs as SYSTEM, the service runs only from the host's own copy in
`C:\Program Files\Helio Host`, which `helio-host enable` makes (as administrator): it
copies `helio-host.exe` there, sets the folder so that only SYSTEM and
Administrators can change it, checks the copy's Authenticode signature, and
registers the service, quoted, on the copy (`helio-host/src/hosting/windows.rs`).
The service, and the host when it starts the clipboard helper as the
signed-in user, launch nothing else. Helio Host loads DLLs only from System32 and
from beside `helio-host.exe`, never from `PATH`; the DLLs `helio-host.exe` imports,
loaded before it runs, from System32 alone (`helio-host/build.rs`), so a
`dwmapi.dll` put beside a `helio-host.exe` in a user's folder is ignored even when
`helio-host enable` runs there elevated. `helio-host disable` removes the service, its
firewall rules and the copy.

State lives in `C:\ProgramData\Helio Host\`: `config.toml`, the host's identity,
paired clients, the API's certificate, tokens and admin account, and `logs\`. The host
makes that folder private when it starts (`helio-host/src/private.rs`): only
SYSTEM and Administrators can read it, apart from the two files Helio Host's
window needs (`config.toml`, `web-cert.pem`). It uses the folder only if
SYSTEM or Administrators own it; one another user made first is moved
aside as `C:\ProgramData\Helio Host.untrusted-…` (the service's log says so) and
the host starts afresh, so devices pair again. A `helio host` command run
without an administrator neither creates nor moves it: those that read it
say they need an administrator's terminal, and exit with 740.

`helio-host status` reports `failed` while HelioHost runs but its host does not
(the host exited; the service starts it again 3 s later), as it does for a
LaunchAgent or a unit that keeps restarting.
The engine comes with Helio, `helio-host.exe` beside `Helio.exe`; turning
hosting on, with the app's switch or `helio host enable`, installs it as
the service ([../install.md](../install.md#hosting-on-windows)).

### Hosting from Helio, and its icon in the notification area

Helio's hosting switch runs `helio-host.exe enable` (or `disable`) from beside
Helio or from `C:\Program Files\Helio Host`. Run as you, it needs an
administrator and exits with 740 having changed nothing; Helio then runs it
again through UAC (`ShellExecuteEx` with `runas`), says it waits for
Windows meanwhile, and that nothing changed if you choose No. `helio host
enable` and `disable` do the same from a terminal. Its Hosting
page says whether there are NVIDIA graphics (NVENC; else H.264 in software,
Media Foundation), SudoVDA (a virtual display; else the main display,
scaled) and ViGEmBus (controllers): with hosting off, that turning it on
installs them; with hosting on, that one missing did not install.

While hosting is on, Helio is also the host's icon in the taskbar's
notification area, as Sunshine's is: it runs as the signed-in user, not as
the service, so what its menu opens opens on that user's desktop. A click
on the icon opens the window; the right button has the menu (what the host
does, a device asking to pair, the window, an update, **Quit Helio**). A
device asking to pair is also a Windows notification. **Open at login** in
General starts Helio at sign-in as the icon only, from the user's `Run`
key (`Helio.exe --background`). If Explorer restarts, the icon comes back
with it (the app listens for `TaskbarCreated`).

### A session, on Windows

- **The display.** A SudoVDA virtual display is added in the client's exact
  mode, made primary with the display-configuration (CCD) API, and — unless
  the client or the settings keep them — the host's own monitors are turned
  off, so the virtual display is the whole desktop and windows and games
  open where the client can see them (Sunshine's "deactivate other
  displays", Apollo's default). That arrangement is saved in Windows'
  display database, as Sunshine saves it: Windows keeps one arrangement per
  set of connected monitors, and the set with the virtual display in it
  exists only during a session, so a game that makes Windows apply its
  saved arrangement (resetting the display mode, as starting Cyberpunk 2077
  most likely did) gets the session's, and your own arrangement is
  untouched. Helio Host also looks four times a second
  whether something put the host's monitors back, and takes them out again.
  A keepalive thread
  calls the driver every second: SudoVDA removes a monitor ~3 s after the
  last one. At the end the virtual display goes first, then the host's
  arrangement comes back. `helio-display/src/windows.rs` explains each of
  these, and why the obvious alternatives fail.
- **Capture.** DXGI Desktop Duplication, re-attached to the input desktop
  whenever duplication is lost, so the secure desktop is captured too. The
  process's GPU scheduling priority is raised (as Sunshine does) so a game
  saturating the GPU does not starve capture and encode.
- **Colour.** BGRA → NV12 in the host's own D3D11 shaders: BT.709, limited
  range, chroma sited left, the colour description written into the stream.
  For a client that asks for **HDR** (and an NVIDIA GPU that encodes
  10-bit), the virtual display is switched to HDR for the session (Windows
  11 24H2's HDR state, or advanced colour before it, as Sunshine's
  libdisplaydevice does), duplicated in FP16 (scRGB), and converted to
  BT.2020 with the PQ curve into P010; the pointer is drawn at the
  display's SDR white, and an SDR frame (a secure desktop) is placed
  there too. The client hears the display's light levels and SDR white
  (`HdrMetadata`). For **4:4:4**, BGRA → packed AYUV, chroma at full
  resolution. Not both at once: NVENC takes 10-bit 4:4:4 only from CUDA,
  not from a D3D11 texture, so HDR wins.
- **Encode.** NVENC P1, ultra-low-latency tuning, CBR with a single-frame
  VBV, two-pass at quarter resolution (a setting, on by default), an
  infinite GOP and five reference frames where the GPU supports them, so a
  lost frame is repaired by invalidating it as a reference rather than by a
  keyframe. HEVC Main10 (and AV1 10-bit) for HDR, `chromaFormatIDC` 3 for
  4:4:4 (H.264 High 4:4:4), where the GPU says it can
  (`NV_ENC_CAPS_SUPPORT_10BIT_ENCODE`, `..._YUV444_ENCODE`).
- **Sound.** WASAPI loopback of the session's virtual sink, stereo to 7.1,
  Opus as Sunshine configures it.
- **Input.** `SendInput`: keys by scancode, the mouse relative (the host's
  pointer speed applies, as with Sunshine) or as positions over the display,
  mapped from the desktop as it is at each move, so a game that changes the
  display's resolution does not move every click. Typed text (an agent's
  `type`, a pasted clipboard) goes one character every 15 ms: Windows 11's
  WinUI text fields garble faster Unicode keystrokes.
- **The pointer.** Desktop Duplication reports the pointer beside the
  desktop image, never in it: its place, whether it shows, and its shape.
  Helio Host draws it into a copy of the desktop on the GPU, as Sunshine does, so
  the client sees the pointer the host's screen shows, hidden whenever a
  game hides it. A pointer that moves over a still desktop makes a new
  frame.
- **Controllers.** ViGEm pads, one per client pad, with rumble sent back.
- **Clipboard.** A helper runs as the signed-in user for the session
  (`helio-host clipboard-agent`): SYSTEM sees only part of the user's clipboard,
  and must not read files on the user's behalf.
- **Awake.** The display and system are kept awake for the session: Windows
  powering the display off at its idle timeout would stop the capture.
- **While streaming**, as Sunshine asks of Windows (`misc.cpp`,
  `streaming_will_start`): a 1 ms timer resolution (otherwise waits with a
  timeout wake 15.6 ms apart), DWM's composition scheduled by MMCSS, every
  connected Wi-Fi adapter in media-streaming mode (no background scans),
  and Mouse Keys on when the host has no mouse attached, so Windows shows a
  pointer to draw. All put back when the session ends.
- **The NVIDIA driver**, as Sunshine sets it, through NVAPI's driver
  settings: a profile of Helio Host's own for `helio-host.exe` prefers maximum
  performance (the driver's adaptive clocks otherwise slow the encoder
  between frames), and OpenGL and Vulkan present through DXGI while Helio Host
  runs, so Desktop Duplication captures full-screen OpenGL and Vulkan games
  at their full frame rate. The second is the driver's global setting:
  Helio Host keeps what it was in `nvidia-undo.toml` and puts it back when it
  stops (or, after a crash, when it next starts). Both are settings, on by
  default.

### Starting a stream

From the tunnel up to the session started, 3024x1890@120:

| The host's own monitor | Virtual display on the desktop | Tunnel → session started |
|---|---|---|
| Awake | 153–181 ms | 0.87–1.5 s |
| Switched off at its button (still connected) | ~3.9 s | 5.2–6.0 s |
| Unplugged (headless) | 327–372 ms | 0.62–0.79 s |

An asleep or switched-off monitor is still on the cable, and Windows spends
~3.7 s on it whenever the display arrangement changes. Apollo, measured on
the same host, pays the same.

### Without NVENC or SudoVDA

A PC with no NVIDIA GPU, a virtual machine, or a Windows on Arm PC (SudoVDA
is built for x64 only) still hosts, with less:

- **Encoding: H.264 in software**, with Media Foundation's encoder, which
  comes with Windows (not on the N editions without the Media Feature
  Pack). Set up as NVENC is: the fastest speed, no B-frames, CBR with a
  single-frame buffer, low-latency mode, IDR only on request. No HEVC,
  AV1, HDR or 4:4:4, and no reference invalidation: a loss costs a
  keyframe, as on the Linux host's FFmpeg path. The log says
  `no NVENC; encoding with Media Foundation's H.264 encoder (software)`.
  Sunshine's software encoder is libx264; this one ships with Windows.
- **The display: the host's main display as it is**, scaled to the
  client's mode, as Sunshine streams a display and as the macOS host does
  without a virtual display. Its monitor stays on, so someone at the host
  sees the session, and an agent's session holds its input while someone
  uses the host rather than ending.
- **A GPU without D3D11 video support** (Microsoft's Basic Render Driver,
  in a virtual machine) is opened without it: nothing Helio Host does needs it.

Measured in a Windows 11 24H2 Arm virtual machine (UTM on an M4 Pro, 6
cores, the Basic Render Driver; Helio Host, an x64 program, under Windows'
emulation): a still desktop at 1024x768 and 30 fps took 8.8 ms from capture
to encoded at the median (15.5 ms p95) and 30-35 ms from the host's capture
to the Mac's screen, with no loss; `grab_encode` (in
`helio-encode/examples`) converted and encoded the same desktop in 12 ms
at the median, 69 ms p95. Motion at a full 30 fps was not measured.

### Limits and known issues

- **Network priority (QoS).** Sunshine marks its traffic for Wi-Fi's video
  queue with qWAVE; qWAVE will not take Helio Host's dual-stack socket. It would
  take an IPv4 socket of its own, or a system QoS policy on the host.
- **Keeping the host's monitors on, with its monitor asleep or switched
  off.** With **Keep this PC's monitors on while streaming** (or
  `helio stream --keep-host-displays`) the host's own monitor stays part of
  the desktop, so Windows brings it up before the session starts: 12 to 38 s
  measured with the monitor switched off at its button, against about a
  second with it awake. The client waits up to 45 s for a session to start.
  The default mode does not pay this: it takes the host's monitors out of the
  desktop.
- **Ending with the monitor asleep.** Bringing a DisplayPort monitor back
  from deep sleep blocks the display change for as long as it takes to wake
  (up to ~26 s measured); a client connecting meanwhile waits.
- **AV1** needs an NVIDIA GPU that encodes it (RTX 40 series and later).
- **Switching between HDR and SDR.** Windows keeps a virtual display's
  HDR state for the next one, so a session that wants the other state than
  the last switches it. The first switch on a new virtual display takes
  Windows 3.8 s, later ones 25 ms (Windows 11 25H2, RTX 3070 Ti): a client
  that changes between HDR and SDR sessions starts 3.8 s later unless the
  previous session's display is still kept (for a minute after it ends).
- **SmartScreen and a new certificate.** The installer and every program
  and DLL in it are signed (Authenticode, timestamped), so Smart App
  Control lets them run, but SmartScreen judges a publisher by its
  reputation, which a code-signing certificate gathers only as people
  download and run what it signed: until then it may ask before the
  installer runs ([../install.md](../install.md#on-windows)).
- **The client's firewall prompt.** Helio installs per user, without an
  administrator, so it cannot add a firewall rule for itself: Windows
  asks once, the first time Helio listens. The host's rules are added by
  `helio-host enable`, which runs as an administrator.

## Helio, the client

As Moonlight does it on Windows:

- **Decode.** FFmpeg's decoders with the D3D11VA hwaccel, on the same
  Direct3D 11 device that presents, on a thread of their own. Each picture
  is copied out of the decoder's texture array into one of five textures of
  Helio's at once, so the decoder's pool is never held by the screen.
- **Present.** A flip-model swap chain with frame latency one: the newest
  frame, never a queue. Full screen is a borderless window over the monitor,
  which Windows flips straight to the display. Frame pacing draws one frame
  per refresh on DXGI's vblank.
- **Keyboard and mouse.** While captured, a low-level keyboard hook takes
  every key — the Windows key and Alt+Tab included, full screen — as SDL's
  keyboard grab does for Moonlight, except the screenshot shortcuts (Print
  Screen, Win+Shift+S, Win+Shift+R): those are handed back to Windows
  whole, sent with `SendInput` and marked so the hook lets them through.
  The pointer is confined to the window
  and hidden (the host draws its own into the picture); raw input's
  relative motion goes to the host, or the pointer's position after
  Ctrl+Alt+Shift+M.
- **Controllers.** XInput, slot for slot; the Guide button through
  `XInputGetStateEx`; rumble through `XInputSetState`.
- **Sound.** WASAPI shared mode on the default device, following it when it
  changes.
- **Timers.** 1 ms timer resolution while streaming.

The installer (`tools/windows/helio.iss`, Inno Setup, built by
`tools/package-windows.ps1`) installs Helio for the user who runs it, with
no administrator (`PrivilegesRequired=lowest`): Helio and the host's engine
in `%LOCALAPPDATA%\Programs\Helio`, `helio` (the command line:
`bin\helio.exe`, a console program that runs `Helio.exe cli` and waits for
it, since Helio.exe is a window program a terminal neither waits for nor
shares its console with), in the Start menu, and in Settings > Apps (the
key
`HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\dev.mihaicristian.helio_is1`),
whose **Uninstall** runs `unins000.exe`. A Helio that runs is closed
first (Restart Manager). Installed over a Helio whose hosting is on, it
runs `helio-host enable` again from the new files (Windows asks), so the
service runs the new engine. Uninstalling turns hosting off first, while
it is on (`helio-host disable`, as an administrator: Windows asks, and a
refusal leaves Helio installed), then takes `bin` off the user's `PATH`
and removes Helio's start at login (the `Helio` value under
`HKCU\...\Run`). Helio's **Settings →
General → Command line** puts `bin` on the `PATH` (`HKCU\Environment`,
written as the `REG_EXPAND_SZ` Windows keeps it as, then
`WM_SETTINGCHANGE` so that what Explorer starts has it); no installer
does.

Helio's own update (**Install and Restart**,
`helio-update/src/install/windows.rs`) is that installer, run again:

1. The release's `Helio-VERSION-windows-x86_64-setup.exe` is downloaded,
   checked against the SHA-256 GitHub records for it, and its
   Authenticode signature checked (`WinVerifyTrust`): a release build
   given the certificate's subject (`HELIO_HOST_WINDOWS_PUBLISHER`, as the
   engine is) refuses an installer that is unsigned, changed after
   signing, signed by anyone else or by a certificate the PC does not
   trust. Only a Helio the installer put in place updates itself (its
   folder is the entry's `InstallLocation`).
2. While hosting is on, Windows asks for an administrator then, while
   Helio's window is in front: asked later, by a program with no window
   in front, the prompt would only flash in the taskbar. Allowed, it
   starts `helio-host update-engine` from the service's engine in
   Program Files; declined, the update stops and nothing has changed. A
   PC that does not host is never asked.
3. Helio quits, and a Windows PowerShell with no window runs the
   installer for the user (`/VERYSILENT /SUPPRESSMSGBOXES /NORESTART
   /NOCANCEL /HELIOUPDATE`, its log in `update-setup.log` in Helio's data
   folder), then opens Helio again: the new one, or, if the installer
   failed (it then puts back what it changed), the one there was, which
   says why.
4. `update-engine` waits for that run, and once it has ended well, runs
   the new engine's `enable`, which checks and copies it into Program
   Files and restarts HelioHost on it (its log:
   `C:\ProgramData\Helio Host\logs\update-engine.log`). `/HELIOUPDATE`
   tells the installer to leave the host to this, rather than ask
   Windows itself.

Building it needs an FFmpeg 8 shared build and LLVM (for the FFmpeg
bindings): see [../install.md](../install.md#helio-on-windows).

Not verified yet: keyboard, mouse and controllers streaming another machine
(tests so far streamed the same PC, which feeds input back into itself),
on-glass timing on a real monitor, HelioHost starting the host again at
once after a change that restarts it, and the host's API answering on
localhost only with `helio host pair` in an administrator terminal
(type-checked; the restart, the API and `helio host pair` ran on a Mac).
