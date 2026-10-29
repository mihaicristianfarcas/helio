# macOS

Helio and Helio Host both run on macOS 14 or later (Apple silicon and Intel; the
measurements here are from an M4 Pro).

## Helio, the client

- **Window.** A stream opens in a window of its own. Full screen (the
  default) it goes into a Space of its own, as Moonlight's does: Helio's own
  window stays on the desktop a swipe away, and the Dock and menu bar stay
  hidden in the stream's Space. Ctrl+Option+Shift+X switches between full
  screen and a window. If macOS refuses full screen (a stream started just
  as another left its Space), Helio asks again until it gets it.
- **Picture.** VideoToolbox decodes in real time — one frame in, one frame
  out, no buffering — and the pixel buffer is bound to Metal without a copy
  (`CVMetalTextureCache`). Without frame pacing the newest frame is drawn at
  once; with it, Moonlight's pacer draws one frame per display refresh on a
  `CVDisplayLink` tick.
- **Recording the screen, or something over the stream.** Full screen with
  nothing over it, macOS takes the picture straight to the display, and one
  frame at a time waits for it, so the next frame drawn is the next shown.
  A screen recording, a window, menu or notification over the stream, a
  window rather than full screen, or HDR makes macOS composite the picture
  first, two to three refreshes later. Helio tells which from how long its
  frames take to reach the screen, lets three wait while it lasts, and
  goes back to one after; every frame is still shown (the log says "the
  stream is composited"). The composited path costs latency: about 22 ms
  from decode to the screen at 60 fps, against 6 ms (see
  [benchmarks](../benchmarks.md#recording-the-screen-macos-client)).
  macOS also takes a recorded stream straight to the display at times
  and shows some of its frames a refresh late; at the display's full rate
  (120 fps at 120 Hz), Helio then shows 105–117 frames of 120 rather than
  add a refresh to every one.
- **AV1.** On a Mac with an AV1 decoder (M3 and later), VideoToolbox
  decodes it in hardware, 8- and 10-bit: the format description carries an
  `av1C` record built from the stream's sequence header, and samples go in
  without temporal delimiters, as MP4 stores them. Settings > Video offers
  AV1 only there.
- **HDR and 4:4:4.** On Apple silicon, VideoToolbox decodes HEVC Main10
  and HEVC 4:4:4 (8- and 10-bit) in hardware (measured on an M4 Pro); Helio
  asks for 10-bit pictures as `x420`/`x444`, which Metal samples plane by
  plane (VideoToolbox's own are packed). With **HDR** on (Settings >
  Video) and a display with headroom above SDR white (an XDR panel, an
  HDR monitor), the stream's layer is BT.2100 PQ, 10 bits a channel, with
  the host's HDR metadata (`CAEDRMetadata`), as Moonlight's
  `vt_metal.mm` does: the host's SDR white lands on this Mac's, and
  brighter goes into the display's headroom. Text and statistics drawn
  over an HDR picture are converted to it.
- **Keyboard and mouse.** While captured, the pointer is locked and hidden,
  and its relative motion goes to the host (or its position, after
  Ctrl+Option+Shift+M); the pointer you see is the host's, in the picture.
  Pressing ⌘⇧ gives the pointer to the Mac, shown and free, so a
  screenshot shortcut's crosshair can show and move; a key that reaches the
  stream with them was no system shortcut and gives the pointer back. The
  tool usually takes the release of ⌘⇧, so the stream reads the keyboard's
  state on its next event, and takes the pointer back once no other window
  is under it. Command can be forwarded as the Windows key
  (**Settings > Input**); against a Mac host it always is.
- **Controllers.** GameController, with the Xbox button read from the pad's
  HID reports, as Moonlight does; rumble through the pad's haptics. macOS
  26 also opens its Games overlay on that button, as it does with Moonlight.
- **Sound.** CoreAudio, stereo to 7.1.
- **Local network.** macOS asks once whether Helio may use the local network
  (it declares `NSLocalNetworkUsageDescription`); without it, discovery and
  LAN connections fail.
- **Notifications** for agent sessions (when a session needs you and is not
  on screen) are asked for the first time a session opens.
- **The menu bar** is a Mac app's: Helio (About, Check for Updates,
  Settings, Services, Hide, Quit), File (Add Host, Look for Hosts Again,
  Close Window), Edit, View (the pages), Window and Help (the docs, the
  logs in Finder, reporting an issue).
- **One copy.** Opening Helio again brings the running one forward.

## Hosting: Helio Host

Helio Host also runs on a Mac, with the Windows host's session rules and the same
wire protocol. A client cannot tell them apart, except that the session ack
says which system the host runs (so Helio forwards Command as Command).

| | Windows host | macOS host |
|---|---|---|
| Display for the session | SudoVDA virtual display, primary, others off | `CGVirtualDisplay` (private CoreGraphics) in the stream's mode (HiDPI when the client's own screen is, see below), main, the Mac's own displays mirroring it |
| Capture | Desktop Duplication | ScreenCaptureKit, NV12 video range, BT.709; for HDR its HDR10 capture (`x420`, BT.2100 PQ) |
| Colour conversion | D3D11 shaders | none (ScreenCaptureKit delivers NV12) |
| Encoder | NVENC, reference invalidation | VideoToolbox hardware HEVC/H.264 (HEVC Main10 for HDR), low-latency rate control, recovery from long-term references |
| Sound | WASAPI loopback of a virtual sink, up to 7.1 | ScreenCaptureKit's system mix in stereo; up to 7.1 from a Core Audio tap of a surround output |
| Input | `SendInput` | CoreGraphics events at the HID tap |
| Pointer | Desktop Duplication's, drawn into the picture | ScreenCaptureKit draws it into the picture |
| Keeping awake | `SetThreadExecutionState` | IOKit power assertions (an asleep display cannot be captured), and no sleep when the lid closes |

Everything after the encoder is shared with the other hosts: the paced
sender, FEC, adaptive bitrate, the tunnel, pairing, discovery, reconnection
and the web UI. The Mac-specific modules are in `helio-host/src/mac/`.

### Hosting from Helio, and its menu bar icon

Helio is an ordinary app, in the Dock. The host is a helper app inside it,
`Helio.app/Contents/Helpers/Helio Host.app`, with its own bundle identifier
(`dev.mihaicristian.helio.host`) and usage strings, signed before the app around it
(`tools/build-helio-app`). Helio's hosting switch runs `helio-host enable` from the
helper, which copies the helper to the host's data folder
(`~/Library/Application Support/Helio Host/Helio Host.app`), writes the host's
LaunchAgent for that copy and loads it; `helio-host disable` removes both. The
host runs from an app bundle so that macOS's permissions apply to Helio
Host, not to Helio, and from a copy outside Helio.app because macOS files
Screen Recording for code inside an app under that app: run from inside
Helio.app (`Contents/Helpers` or `Contents/Library/LoginItems`), the
request named Helio and the grant was kept under Helio's identifier; from
outside, it is Helio Host's. Accessibility was the helper's either way
(macOS 26.5). `helio-host enable` from a `helio-host` outside an app bundle
refuses (`helio-host/src/hosting/mac.rs`). An update of Helio copies the new
helper and restarts the host while hosting is on.

The copy has to follow the app. So the LaunchAgent names the app the copy
came from (`helio-host host --app PATH`), and the host watches it: kqueue says
when the app is moved, renamed or deleted, or what is in it changes
(Homebrew's upgrade keeps the app folder and replaces its contents), and
15 s later the host looks again (every update puts the new app in place
within a few seconds).

- **Replaced** (Homebrew's upgrade, the app's own update, a new download
  dragged over it): the host checks the new app's helper meets the
  running copy's designated requirement (its code-signing identity, by
  which macOS keeps the permissions it granted) and, when the app is
  still marked as downloaded, that Gatekeeper accepts it (`spctl
  --assess`: its notarization). Then it copies the helper over its own,
  clears that copy's quarantine flag (an app Homebrew upgraded comes
  marked as never opened, and launchd would wait on Gatekeeper's dialog),
  and restarts on it. Signed otherwise (a development build over a
  release) or refused, the host goes on as it was, and its log says why.
  The same check runs at start, for an app replaced while no host ran.
- **Gone** (uninstalled, dragged to the Trash): the host turns hosting off
  itself, its LaunchAgent and copy removed, rather than start at every
  login with no app to turn it off; at start too.

So every update keeps hosting as it was, with nothing asked of the
package manager. Homebrew could not do it: it upgrades a cask by
uninstalling and installing it, tells neither step which it is doing, and
runs a cask's install steps in a sandbox with a home folder of its own
(`tools/update-casks`). `brew uninstall` quits Helio, and the host stops by
itself 15 s later; `brew uninstall --zap` stops it at once.

While hosting is on, Helio shows the host's icon in the menu bar: what the
host does, a device asking to pair (a notification says so too), the
window, an update. Closing the window leaves the icon, and Helio leaves the
Dock until its window opens again (the accessory activation policy);
**Open at login** in General starts Helio at login as the icon only
(`--background`). Quitting Helio leaves the host running: it is the
helper's copy, under a LaunchAgent that keeps it alive.

Opening **Helio Host** itself (from the Finder, inside Helio.app, or with
Spotlight) starts the host and Helio with it, in the background, so the
icon is in the menu bar whenever you started the host. Opened again while it runs, it changes
nothing. The host started at login by its LaunchAgent leaves the icon to
Helio's own login item (Open at login).

### Permissions

Helio Host needs **Screen Recording** (to capture) and **Accessibility** (to
inject the keyboard and mouse). As the helper app it asks for both in its
own name, **Helio Host**, at start, and System Settings lists it, not Helio,
under both. The host says on its API what macOS
allows it, and Helio's Hosting page and setup show it, with a button to
each pane of System Settings; Helio itself needs neither. From a terminal, the terminal needs them.
Without Accessibility, Helio Host tells the client, and Helio shows that the host
is ignoring its keyboard and mouse. Sound in surround needs **System Audio
Recording** besides (below): Helio Host asks for it the first time a client wants
5.1 or 7.1, and that session streams stereo while you answer.

macOS ties these permissions to the app's signature. An ad hoc signature
changes with every build, so each rebuild would lose them;
`tools/dev-signing-identity` creates one self-signed identity that the
build scripts sign with, so permissions survive rebuilds. Releases are
signed with the project's Developer ID, the same from one release to the
next, and notarized.

### Sound in surround

ScreenCaptureKit, which Helio Host captures the screen with, mixes the system's
sound down to stereo whatever it is asked for (2 channels at 48 kHz when
asked for 6 or 8, measured on macOS 26.5). A client that asks for 5.1 or 7.1
gets it from a Core Audio tap instead (macOS 14.2 and later), which hears
what apps play to an output device in that device's own channels. Apps play
surround only to an output that has the speakers for it, so the Mac needs
one:

- **A receiver or TV with surround on HDMI**, set to 5.1 or 7.1 in Audio
  MIDI Setup (**Configure Speakers**). Helio Host taps it while it stays the
  default output, and keeps the sound off it unless the client asks for the
  sound on the host too (`--host-audio`), as the Windows host does.
- **A loopback device**, as Sunshine uses on a Mac: install
  [BlackHole](https://github.com/ExistentialAudio/BlackHole) 16ch
  (`brew install --cask blackhole-16ch`). For each surround session Helio Host
  makes it the default output, sets its speakers to the session's 5.1 or
  7.1, and puts back the previous output and speakers when the session
  ends, as the Windows host does with Steam Streaming Speakers. The Mac
  plays nothing aloud meanwhile; with `--host-audio` Helio Host leaves the output
  alone and the session is stereo.

The channels go into the stream in Moonlight's order whatever order the
device has them in, and a device that does not run at 48 kHz is resampled
(set it to 48 kHz in Audio MIDI Setup for the best sound). Measured with
BlackHole 16ch, streaming the Mac to itself: a test file sounding one
channel at a time came out of the client's decoder in the same channel,
for 5.1 and for 7.1, each at its level (a 5.1 file's back pair 3 dB lower:
macOS plays its rear speakers into the 5.1 device's surround pair). With
neither, or without the permission, sessions are stereo and Helio Host's log
says why.
`cargo run -p helio-audio --example mac-outputs` lists the outputs and
what each can carry; `-- --tap 5` also listens to the default one for five
seconds and prints each channel's level.

### Measured (M4 Pro, client and host on one Mac)

- Session up **238 ms** after the tunnel (a Windows host takes seconds,
  mostly Windows bringing the virtual display up); the virtual display
  plugged in and made main in ~100 ms.
- VideoToolbox HEVC encode costs **~2.7 ms per megapixel**: 6.9 ms at 1080p,
  15–16 ms at 3024x1890 — so that size tops out near 55 fps with one frame
  in the encoder. Two in flight gave 63 fps but 27 ms from capture to
  encoded; latency wins. H.264 cannot do 3024x1890 at 120 at all (past its
  level limits, the encoder drops every frame).
- Loss: 5% in bursts of 3 recovered by long-term-reference refreshes, with
  no keyframes and no decode errors.
- Sound: 200 packets a second, nothing concealed.
- Helio Host killed mid-stream and restarted: Helio renegotiated, and video was
  back 0.4 s after Helio Host was.

### Behaviour worth knowing

- **One virtual panel.** macOS holds a display it has never seen offline
  until someone answers "What do you want to show on Helio Host?". So the virtual
  panel is the same every session (3840x2400) and put in the session's mode
  once online: sizes from 800x600 to 3024x1890 come up in 0.1–0.7 s without
  a prompt. If it does not come up within 2 s, Helio Host streams the main display.
- **Lid closed**: the stream goes on. A MacBook sleeps a few seconds after
  its lid closes, on battery even with a display attached, and no power
  assertion stops that; for the length of a session Helio Host asks macOS not to
  sleep on the lid (`kPMSetClamshellSleepState`, as Amphetamine's
  Closed-Display Mode does), and lets it again when the session ends. The
  session's virtual display is what gets captured. A locked Mac streams its
  lock screen. Amphetamine's Closed-Display Mode, if you use it, is off
  after a session until it turns it on again.
- **The capture starts again on its own** when macOS stops it (the
  displays turning off as the lid closes, sleep, a display rearranged):
  macOS does not always say so, but a capture that runs hands over
  something many times a second even on a still desktop, so 2 s of nothing
  is a stopped capture. The client keeps the last picture meanwhile.
  Measured with `replayd` (which serves every capture) killed mid-capture:
  the picture and the sound ran again 0.2 s after the 2 s.
- **The display's density follows the client's screen.** A client says its
  scale (200% for a Retina Mac, Windows' Scale setting), and the session's
  display is HiDPI (two pixels a point) for a client at 150% or more, at
  2560 px across or wider: a Retina Mac sees the host at its own scale, a
  1440p monitor at 100% gets a 2560x1440 desktop rather than one that
  looks like 1280x720. A client that does not say gets HiDPI from 3840 px
  across. Another resolution picked in System Settings mid-session is
  followed by the keyboard and mouse at once.
- **Loss recovery** uses VideoToolbox's long-term references. The protocol
  has no per-frame acknowledgement, so Helio Host counts a frame as arrived once
  200 ms pass with no loss reported for it, and acknowledges it to the
  encoder; after a loss the next frame refreshes from an acknowledged one. A
  wrong guess costs a keyframe, as before.
- **The pointer** is in the picture: ScreenCaptureKit draws it, in its
  real shape, and hides it when an app does.
- **HDR** (macOS 15 or later, Apple silicon): for a client that asks, the
  virtual display is an HDR one (a `CGVirtualDisplayMode` with a transfer
  function, measured to give the screen 5x SDR white of headroom),
  captured with ScreenCaptureKit's HDR10 preset and encoded as HEVC Main10
  with BT.2020 primaries, the PQ curve and HDR10 metadata. Verified Mac to
  Mac on one M4 Pro: 52-53 fps of desktop at 1920x1200, no loss.

### Limits

- No 4:4:4: VideoToolbox encodes HEVC at 4:2:2 at most (its RExt profile
  is 4:2:2 10-bit).
- Sound is stereo unless the Mac has a surround output and Helio Host may record
  the system's sound (see [Sound in surround](#sound-in-surround)).
- No controllers yet: macOS makes virtual controllers only for a program
  signed with the HID Virtual Device entitlement
  (`com.apple.developer.hid.virtual.device`), which Apple grants on
  request.
- Streaming a Mac to itself (a loopback test) reports no presented frames:
  the client's window sits on the display being streamed, which reports no
  presentation times.
