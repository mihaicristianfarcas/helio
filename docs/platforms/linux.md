# Linux

Helio and Helio Host both run on Linux, under X11 and Wayland. Linux support is the
youngest: it is developed and tested in a container (Ubuntu 24.04, Xvfb,
Mesa's software renderer), and has not yet been tried on much real hardware
(see "Not verified yet" below).

## Helio, the client

The same app as on the other systems. Because winit allows one event loop
per process and the app's window has it, a stream runs in a second process
of the app (`helio-app --helio-stream`), which tells the app how it ended on a
line of its standard output. That process is `helio_client::linux`:

- **Window and input.** A winit window (X11 or Wayland). Keys go as
  scancodes through the shared keyboard handling (Moonlight's
  Ctrl+Alt+Shift chords); the pointer is captured by locking or confining
  the cursor.
- **Decode.** FFmpeg, with VA-API where the machine has it, else four slice
  threads in software. `HELIO_SOFTWARE_DECODE=1` skips VA-API. A 4:4:4
  stream (**YUV 4:4:4**, which a Linux client asks for when set) decodes in
  software where VA-API does not take it.
- **Present.** wgpu (Vulkan or GL), drawing the planes (NV12, I420 or I444)
  with the same colour maths as the Windows renderer.
- **Controllers.** gilrs (evdev), rumble included.
- **Sound.** cpal (ALSA; PipeWire and PulseAudio through it).
- **In the launcher.** `tools/linux/install-apps` (from a checkout) and the
  download's `install.sh` install Helio for the user with a desktop entry
  and an icon, named after the window's app id (`dev.mihaicristian.helio`), so
  the dock and the task switcher show it under its own icon, under Wayland
  and X11.
- **The window's glass.** GPUI blurs behind a window only under Wayland,
  where the compositor offers KDE's blur (KWin, its blur effect on and
  compositing with OpenGL); elsewhere, KWin drawing without a GPU
  included, the window is opaque, in the same colours
  ([../ui.md](../ui.md#the-glass-on-each-system)).
- **No tray icon for hosting.** There is no one tray on Linux (the
  StatusNotifier protocol needs a D-Bus service and a menu protocol of its
  own, and GNOME shows it only with an extension), so with hosting on Helio
  is still an ordinary app: closing it quits, and the host runs on as its
  user unit. The hosting switch runs `helio-host enable` (`systemctl --user`),
  as `helio host enable` does, which asks for nothing.
- **`helio` in a terminal.** **Settings → General → Command line** links
  `~/.local/bin/helio` to the app's program, which most distributions put
  on the `PATH` (Fedora's `~/.bashrc`, Ubuntu's `~/.profile`); a `helio`
  there that is not Helio's is left as it is. `install.sh --uninstall`
  removes the link.

## Hosting: Helio Host

The same host — sessions, pairing, the API, the bitrate controller, the
internet path — with `helio-host/src/linux/` where Windows and macOS have theirs.
On GNOME and KDE under Wayland, each session gets a virtual display at the
client's mode, made by the desktop's compositor (below). Elsewhere it
streams the screen as it is, scaled to the client's size (letterboxed when
the shapes differ, the pointer mapped to match).

### A virtual display (GNOME, KDE)

As SudoVDA does on Windows and `CGVirtualDisplay` on macOS, the desktop
makes a monitor of the client's size and rate for the session, and removes
it when the session ends; the client sees that monitor alone, its pointer
drawn in, at its own density where the client says it (a Retina Mac's
200 %). It needs nothing installed beyond the desktop
(`helio-host/src/linux/virtual_display.rs`):

- **GNOME** (tested with mutter 46 and 50): mutter's own screen cast and remote
  desktop (`org.gnome.Mutter.ScreenCast`, `RecordVirtual`, and
  `org.gnome.Mutter.RemoteDesktop`), what gnome-remote-desktop uses, open to
  the session's own programs — so there is no dialog to answer. mutter 50
  takes the mode as asked; earlier ones size the monitor from the stream,
  which asks for exactly the client's size (`helio-host/src/linux/mutter.rs`).
  For the session the virtual monitor is the whole desktop, the host's own
  monitors off, so windows open where the client sees them, as on Windows
  (unless **Keep this PC's monitors on while streaming**: then it extends
  the desktop beside them). The arrangement is mutter's temporary one: when
  the monitor goes, mutter puts back the one it keeps for the host's own
  monitors (`helio-host/src/linux/mutter_layout.rs`).
- **KDE Plasma** (KWin 6): KWin's own screen cast makes a virtual output
  (`zkde_screencast_unstable_v1`), and input goes through KWin's fake input
  (`org_kde_kwin_fake_input`), as krfb's virtual monitor and KDE's portal do
  (`helio-host/src/linux/kwin.rs`). KWin offers both only to a program whose
  desktop entry asks for them (`X-KDE-Wayland-Interfaces`), found by the
  program's path: Helio's `install.sh` installs `helio-host`'s own,
  `dev.mihaicristian.helio.host.desktop`, hidden from the launcher. The
  output extends the desktop; the host's monitors stay on. Text goes by
  the keys that type it on the host's layout, read from KWin's keymap, with
  Shift pressed around a capital, as the X11 host types: KWin's own keysym
  typing sets the modifiers out of step with the key (a capital came out
  small, the next letter capital).

Typed text reaches characters on the host's keyboard layout only (mutter
types a keysym with the layout's keys; one KWin types on a keymap of its
own did not reach Konsole), as through the portal.
Under X11, and on other Wayland desktops (wlroots), there is no virtual
display. The host's log says which it uses (`a Wayland session: sessions
get a virtual display of the desktop's own`, or `... go through the desktop
portal`).

### Under X11

- **Capture.** MIT-SHM into memory shared with the X server, XDamage to
  know when the screen changed, the pointer drawn in from XFixes
  (`helio-capture/src/x11.rs`; x11rb, no C libraries).
- **Encode.** FFmpeg, trying VA-API (Intel, AMD), then NVENC, then x264
  (H.264 only, in software), then OpenH264 (H.264 at 4:2:0, in software:
  Fedora's FFmpeg has it in place of x264, and Fedora's VA-API encodes no
  H.264; `sudo dnf install openh264` installs Cisco's library it loads),
  set up as Sunshine sets them up: no B-frames, an endless GOP, a VBV of
  about a frame, an IDR on a loss (none of these encoders invalidates
  references). swscale converts to BT.709 4:2:0 and
  scales (`helio-encode/src/ffmpeg.rs`); to 4:4:4 for a client that asks
  for it, where NVENC or x264 encodes (VA-API takes 4:2:0 only). No HDR:
  the X server and the portal share SDR pictures. OpenH264 and VA-API keep
  the bitrate a session opened with. OpenH264 also adds IDRs of its own at
  scene changes, and goes over the bitrate on a picture of noise, so use
  it on a wired network or a generous bitrate.
- **Input.** XTest: keys by position, text by the layout's own keys (spare
  key codes remapped only for characters the layout lacks), the wheel as
  buttons 4–7 (`helio-input/src/x11.rs`).
- **Sound.** The default output's monitor through libpulse-simple, which
  PipeWire serves too; stereo.
- The screensaver and DPMS are held off during a session.

### Under Wayland

Where the desktop makes no virtual display for the host, the screen and
the input go through the desktop portal, the way desktop sharing is meant
to work there (`helio-host/src/linux/portal.rs`): a RemoteDesktop
session with a screen cast in it (GNOME, KDE), or a ScreenCast session alone
where the desktop has no RemoteDesktop portal (wlroots: sway, Hyprland — no
input from the client there). Frames come over PipeWire with the pointer
drawn in by the compositor; keys go as Linux key codes, text as keysyms,
the pointer as the portal's motion and buttons.

The first time, the desktop asks whoever is at the host — Helio Host asks as it
starts, not when a client connects. On GNOME, switch on **Allow Remote
Interaction** in that dialog for the keyboard and mouse. The portal then
gives a restore token (kept as `portal-token` in Helio Host's data folder), and
later sessions start without asking. The screen is kept on through the
Inhibit portal. `HELIO_HOST_CAPTURE=x11|portal` overrides the choice where the
environment does not make it clear.

### Running it

Helio Host runs in the desktop session, as the user: `tools/linux/helio-host.service` is
a systemd user unit for that, which Helio's `install.sh` installs beside the
app without turning it on, with the host's desktop entry and the
libraries the programs need that the system lacks
([../install.md](../install.md#on-linux)),
and which turning hosting on and off (the switch, or `helio host enable` and
`disable`) enables and disables (`systemctl --user enable|disable --now
helio-host`; no root).
Data and logs are in `~/.config/helio-host`.

## Measured (in the container)

| What | Result |
|---|---|
| Linux host to Linux client, 1920x1080 screen to a 1280x720@60 stream, x264 in software | 60 fps; 4.2 ms capture to encoded; client decode 0.3 ms; 8.5 ms capture to screen; clicks, keys, typed text ("Hé!") and the wheel arrive; a 440 Hz tone at 200 packets a second |
| The same at 4:4:4 (`--yuv444`: x264 High 4:4:4 Predictive) | 60 fps; 5.7 ms capture to encoded; client decode 0.34 ms in software; 10.4 ms capture to screen; no loss |
| The same through the Wayland portal (GNOME 46, nested, software rendering) | frames at the 42 fps the nested compositor draws; 4.7 ms capture to encoded; 10–11 ms capture to screen; input arrives; later sessions start from the restore token without asking |
| Linux client streaming a Windows host over a VPN, 1280x720@60 | H.264 and HEVC; 60 fps received, decoded and presented; 1 ms decode (software); 14–20 ms capture to screen |
| Offscreen decode and present of the 600-frame test stream (`helio-client/examples/linux-render-check.rs`) | every colour bar matches FFmpeg's own BT.709 conversion exactly |
| GNOME's virtual display: Ubuntu 26.04's mutter 50.1, headless (no GPU), to a client in the same VM, x264 in software, release build | a 1600x900@60 monitor made for the session and removed at its end; the host's monitor off for the session and back after it; 2.9 ms capture to encoded (p50), 4.2 ms capture to screen; a still desktop sends 12 fps; a click, keys and typed text reach a terminal opened on it |
| The same with Ubuntu 24.04's mutter 46.2 (no `modes`), in the container, debug build | a 1440x900 monitor sized from the stream; arranged and restored the same |
| KDE's virtual display: Fedora 44's Plasma 6.6.4 under Wayland (a virtual machine, virtio GPU), `kwin.rs`'s desktop tests | a 1280x800 output at 200 % made, its frames received at 1280x800, the pointer drawn where it was moved, the output gone with its stream; a Konsole opened on it receives `ls from Helio!` typed as keys and text |
| A whole stream from that desktop: Helio Host installed by `install-apps` and running as its user service, OpenH264 in software, release build, a Linux client in a container on another computer, FFmpeg decoding in software | a 1600x900 output made for the session and gone within 2 s of its end; a click and typed text reach a Konsole on it; a terminal of random text scrolling, as fast as the VM draws: 23 fps, 11 Mbps of 15, 5.0 ms capture to encoded (p50; p99 6.8), no frame lost, no IDR after the first |
| The same to a Mac, 1280x720, VideoToolbox decoding | 45 fps, 11 Mbps of 10, 3.0 ms capture to encoded (p50), 1.2 ms decode, 27 ms capture to screen, no frame lost |
| OpenH264 alone: 1920x1080, 600 pictures, 60 fps asked for, six Arm cores of a VM (Fedora's OpenH264 2.6.0) | a smooth picture scrolling: 212 fps at 19.8 Mbps of 20 with the 4 slices it is given (94 fps with 1); 5.0 Mbps of 5; only the IDRs asked for. A fine texture scrolling 12 pixels a frame: an IDR of its own every 17 frames, 20.6 Mbps of 20, 9.9 of 5. Noise over most of the frame: 164 Mbps of 20 |

`tools/linux/virtual-display-test` repeats the row for Ubuntu 24.04's mutter.

`tools/linux/loopback-test` repeats the first row; see
[../development.md](../development.md#linux).

## Not verified yet

- The host starting again in its own process after a change that restarts
  it (`helio-host/src/restart.rs`), its API on localhost only, and `helio
  host pair`: the code a Mac host runs, which was run there.
- Sharing the clipboard from a KDE host: in the virtual machine above, once
  a Konsole was open, each read of the clipboard through XWayland timed out
  (`Time-out hit while reading the clipboard`, about a minute a look). A
  session's end does not wait for it (`helio-clipboard`'s `STOP_WITHIN`);
  whether a copy gets through, on that machine or on real hardware, is
  untried.
- GNOME's on a desktop with real monitors (the tests ran headless mutter,
  whose own monitor is virtual too), and a client's density there (Linux
  clients do not send one; the scale chosen is unit-tested).

- Real hardware: VA-API decode and encode, NVENC, controllers, a real
  window manager, compositor and cursor theme, keyboard and mouse through
  the client's window (the container has none; scripted input goes around
  the window).
- The wheel under Wayland: the nested test desktop gives X clients no wheel
  events at all, so the portal's scroll is sent but unseen.
- wlroots desktops (Ubuntu 24.04's wlroots portal needs a GPU to share the
  screen, and the container has none).

## Not built yet

- Input on wlroots desktops (no RemoteDesktop portal there): uinput, or
  their virtual keyboard and pointer protocols.
- The host at the client's resolution under X11 (RandR modes) rather than
  scaled, and on wlroots desktops (`swaymsg create_output`).
- The host's monitors off for a session on KDE (KWin's output management),
  as on GNOME.
- Controllers on a Linux host (uinput gamepads).
- Packages: there is a systemd unit, a release archive and a per-user
  install script (which installs the libraries it needs with apt), but no
  `.deb` or Flatpak.
