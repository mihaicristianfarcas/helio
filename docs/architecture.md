# Architecture

Helio streams a computer's desktop — picture, sound, keyboard, mouse,
controllers and clipboard — to another computer, the way Moonlight and
Sunshine do, with every byte inside one post-quantum WireGuard tunnel.
**Helio** is the app: the client for everyone, and hosting as a switch in
it, off by default, which runs **Helio Host**, the host. One install has
both: the app carries the host's engine, and the switch decides whether it
runs. The two are drawn apart below because they run apart: the host is a
program of its own, which keeps running when you quit the app.

This page is the map: what runs where, how a frame gets from the host's
screen to the client's, and which crate does what. Why each piece is the way
it is, and what was measured on the way, is in the
[design history](design/README.md) and in the code's comments.

## The pieces

```
 Helio (client)                                  Helio Host (host)
 ──────────────                                  ─────────────────
 helio-app  (GPUI window: hosts,                 helio-host  (service / app / user unit)
            pairing, settings, agents;             ├─ web UI + API (HTTPS, 47802)
            This computer: the host's      ──API──▶├─ pairing (TCP, 47801) + mDNS
            pages, the hosting switch,             ├─ presence (DHT, STUN, UPnP)
            the host's icon)                       │
 helio      (CLI; helio host: the switch,          │
            the host's identity and clients)       │
   └─ helio-client ───── one UDP socket ───────────┤
        stream: net thread, input thread           └─ session: display → capture →
        platform: window, decoder, renderer           encode → FEC → pace → tunnel
```

- One **tunnel** per paired client, pq-boringtun (WireGuard with static
  ML-KEM authentication) carried in-process on one UDP socket per side — no
  TUN device, no admin rights for it (`helio-transport`).
- Inside the tunnel, Helio's own **wire format**: a 20-byte header that
  *is* the inner IPv4 header, and four kinds of payload — video, audio,
  input, control (`helio-proto`).
- **One session at a time** per host, on a display made for its client's
  mode. A new client takes over or is refused (per the host's settings); AI
  agents follow stricter rules ([ai-agents.md](ai-agents.md)).

### Crates

| Crate | What it is |
|---|---|
| `helio-proto` | The wire protocol, with no I/O: headers, packetizing and Reed-Solomon FEC, reassembly, the frame gate (loss recovery), control, input, audio and clipboard messages |
| `helio-transport` | The tunnel endpoint: pq-boringtun per peer on one UDP socket, roaming, segmented handshakes, batched sends and receives |
| `helio-pairing` | PIN pairing (SPAKE2 + ML-KEM-768), mDNS discovery, Wake-on-LAN |
| `helio-nat` | Reaching a host from anywhere: STUN, sealed rendezvous records on the Mainline DHT, hole punching, UPnP / NAT-PMP port mapping |
| `helio-display` | Virtual displays: SudoVDA on Windows, `CGVirtualDisplay` on macOS (Linux's are the compositor's own, in `helio-host/src/linux/virtual_display.rs`) |
| `helio-capture` | Screen capture: Desktop Duplication (Windows), ScreenCaptureKit (macOS), MIT-SHM and PipeWire (Linux) |
| `helio-encode` | Video encoders: NVENC with a D3D11 colour converter, or Media Foundation's H.264 encoder without NVENC (Windows), VideoToolbox (macOS), FFmpeg (Linux) |
| `helio-decode` | Video decoders: VideoToolbox (macOS), FFmpeg with D3D11VA (Windows) or VA-API (Linux) |
| `helio-audio` | Opus, the client's jitter buffer and playback, WASAPI capture on Windows |
| `helio-input` | Input injection on the host: `SendInput`, CoreGraphics events, XTest |
| `helio-clipboard` | Sharing the clipboard both ways during a session |
| `helio-ui` | The app's design system (GPUI), asides' look: theme, glass, motion, keyboard focus, icons, controls, Markdown; and what it needs of the desktop: menus, the tray icon, one running copy, starting at login |
| `helio-update` | Whether a newer release, or newer commits on `main`, exist (GitHub's public API, once a day), and installing a release from the app: the package checked against GitHub's SHA-256 and for who signed it, then each system's way of putting it in place (on Windows, the installer), the host's engine with it while hosting is on |
| `helio-client` | The client: session, stream, input, statistics, the host store; per-platform window, decoder and renderer |
| `helio-cli` | `helio`, the command line: streaming and pairing from a terminal, with the client's own code, and this computer as a host (`helio host`: the host's switch run as the app runs it, `engine`; its identity and clients changed in helio-host-data) |
| `helio-app` | The app's window (GPUI): hosts, agents, settings, setup, and This computer (the host's pages, a client of its web API on localhost; the hosting switch, which runs `helio-host enable`, `disable` and `status` through `helio_cli::engine`; the host's tray icon); `Helio mcp`; and `helio`: started under that name, or as `Helio cli`, the app's program runs helio-cli's command line, so the command comes with Helio (on Windows through `bin\helio.exe`, the `helio-console` bin), and Settings > General puts it on the PATH (`command`) |
| `helio-agent` | Computer use: a headless session, the actions, the MCP server, the agent runners; the `helio-agent` CLI |
| `helio-host` | The host: sessions, the video and audio pipelines, pairing, presence, the web UI, the Windows service |
| `helio-host-data` | What the host keeps on disk, for it and for whatever manages it: its data folder (private to the host on Windows), its identity, its paired clients and what each may do |

`vendor/` holds two tiny no-op crates GPUI names but does not use (see the
workspace `Cargo.toml`); `spikes/` holds the early experiments
([spikes/README.md](../spikes/README.md)).

## One client, one host, on every desktop

Almost all of a streaming client is the same everywhere — the tunnel,
negotiation, loss recovery, input batching and redundancy, the audio jitter
buffer, statistics, pairing, discovery, Wake-on-LAN, the internet path, the
host list and the settings. What differs is a thin layer that has to be
native to be good: how a window takes the keyboard and pointer, how a decoded
frame reaches the screen without a copy or a queue, how a controller is read.
So there is one client and one host, with a platform layer each:

| | macOS | Windows | Linux |
|---|---|---|---|
| Client window and input | AppKit (`helio_client::mac`) | Win32 (`helio_client::win`) | winit, X11 or Wayland (`helio_client::linux`) |
| Client decode | VideoToolbox | FFmpeg + D3D11VA | FFmpeg + VA-API, or software |
| Client present | Metal | Direct3D 11 | wgpu (Vulkan or GL) |
| Client controllers | GameController + IOHID | XInput | gilrs (evdev) |
| Client audio out | CoreAudio | WASAPI | cpal |
| Host display | `CGVirtualDisplay` at the client's mode | SudoVDA at the client's mode, installed with hosting (without it, the main display, scaled) | a virtual display at the client's mode, mutter's (GNOME) or KWin's (KDE); elsewhere the screen as it is, scaled |
| Host capture | ScreenCaptureKit | Desktop Duplication | MIT-SHM (X11), PipeWire through the desktop portal (Wayland) |
| Host encode | VideoToolbox | NVENC, else Media Foundation (H.264, software) | FFmpeg: VA-API, NVENC, x264 or OpenH264 |
| Host input | CoreGraphics events | `SendInput`, ViGEm pads | XTest (X11); under Wayland mutter's remote desktop (GNOME), KWin's fake input (KDE), else the RemoteDesktop portal |
| Host sound | ScreenCaptureKit | WASAPI loopback of a virtual sink | PulseAudio / PipeWire monitor |

The host's platform layer is `helio-host/src/platform.rs` (Windows),
`helio-host/src/mac/` and `helio-host/src/linux/`; they are compiled under one module
name, so the session code above them is the same on every host. Details and
limits per system are in [platforms/](platforms/).

## Processes and threads

**Helio Host.** On Windows a service (`HelioHost`, LocalSystem) launches
`helio-host host` into the console session with a SYSTEM token: only SYSTEM can
open the secure desktop, so only it can capture and control a UAC prompt or
the lock screen (the model Sunshine uses); it runs only from the host's own
copy in `C:\Program Files\Helio Host`, which turning hosting on makes from the
`helio-host.exe` beside Helio. On macOS it is `Helio Host.app`, the helper app Helio
carries, which macOS grants the host's permissions to, run from its copy in
the host's data folder (outside Helio.app, or Screen Recording would be
Helio's) and started at login by a LaunchAgent; on Linux a systemd user unit
in the desktop session.
The host's program sets each of them up or takes it down
(`helio-host enable`, `disable`; Helio's switch and `helio host` run them): hosting
is off until then ([usage.md](usage.md#hosting-on-and-off)). The host
process runs:

| Thread | Does |
|---|---|
| main (`serve`) | Receives every datagram: control and input dispatched at once, session requests handed to the session thread |
| `session` | The session lifecycle (`helio-host/src/session.rs`): negotiate, set up the display, start the pipelines, ack, watch the client, tear down |
| `video-encode` | Capture → colour conversion → encode, at the negotiated rate (`helio-host/src/video.rs`, `helio-host/src/unix/video.rs`) |
| `video-send` | Packetize with FEC and pace onto the tunnel (`helio-host/src/sender.rs`) |
| `audio` | Capture → Opus → send, with parity |
| `pairing`, `web`, `presence` | PIN pairing, the web UI and API, the internet path |

**Helio.** The app and the CLI start the same stream (`helio_client::session`):

| Thread | Does |
|---|---|
| `helio-net` | Receive → reassemble → frame gate → decode; control; timers (`helio_client::stream::net`) |
| `helio-input` | Batch and send input, with redundancy (`helio_client::input`) |
| `helio-render` / `helio-decode` / window | The platform's presentation |

## A frame, from the host's screen to the client's

1. **Display.** The session gets a display at exactly the client's mode —
   a virtual one on Windows, macOS, GNOME and KDE, made primary, the host's
   own monitors off (unless asked otherwise; KDE keeps them on) so windows
   open where the client can see them.
2. **Capture**, paced to the negotiated frame rate — the client display's
   own, to the millihertz (59.94 Hz is not 60), as Sunshine takes
   Moonlight's: never faster, and a still desktop re-encoded at max(fps/5,
   10) frames a second so it keeps sharpening. This is Sunshine's capture
   loop, re-encoding less often than its half rate (`helio-host/src/pipeline.rs`
   says why).
3. **Colour conversion** on Windows: BGRA → NV12 in D3D11 shaders,
   BT.709 limited range, chroma sited left, the colour description written
   into the stream (`helio-encode/src/convert.rs`). For HDR, the FP16
   desktop → BT.2020 PQ in P010; for 4:4:4, packed AYUV.
4. **Encode**, configured as Sunshine configures NVENC: CBR with a
   single-frame VBV, no B-frames, an endless GOP, a few reference frames so
   a loss can be repaired by reference invalidation instead of a keyframe.
5. **Packetize and FEC.** The frame (behind a 4-byte prefix carrying the
   host's processing time) is split into shards — 1180 bytes on a 1280-byte
   path, 1400 on the local network — and Reed-Solomon parity is added per
   block, per frame: recovery never waits for a later frame.
6. **Pace.** Datagrams leave in 1 ms groups under the pacing rate, as
   Sunshine sends, not as one burst that overflows a Wi-Fi queue; batched into
   a few system calls (USO, GSO, `sendmsg_x`).
7. **Tunnel.** Each datagram is encrypted and sent to the client, then to
   anyone watching an agent's session (the same encode, a second recipient).
8. **Reassemble.** The client collects shards per frame and recovers lost
   ones from parity (`helio-proto/src/reassemble.rs`).
9. **Frame gate.** Moonlight's rule: after a lost frame, nothing is decoded
   until a frame that does not depend on it arrives. The client asks the
   host to invalidate the lost frames as references (the next frame is a
   *recovery* frame) or, failing that, for an IDR
   (`helio-proto/src/video.rs`).
10. **Decode and present.** Hardware decode, then the newest frame to the
    screen at once — or, with frame pacing on, Moonlight's pacer: one frame
    per display refresh. An HDR stream is drawn PQ-encoded into an HDR
    layer with the host's metadata (on a Mac with an HDR display).

The client reports loss and round trip once a second; the host adapts the
bitrate to congestion and the FEC to the link's own loss
(`helio-host/src/bitrate.rs`).

### Why the video path looks like Sunshine's

Each of these was a visible artefact until it was done the Sunshine and
Moonlight way, first learnt from Apollo, Sunshine's fork (the full table,
with the source it was checked against, is in
[design/v3-design.md](design/v3-design.md)):

| Choice | Without it |
|---|---|
| Decode nothing after a loss until a recovery frame or IDR | smeared blocks, regions that never heal |
| Own colour conversion, colour description in the stream | soft text with coloured fringes |
| Single-frame VBV, reference frames kept for invalidation | quality swings, keyframes for every loss |
| Desktop Duplication as SYSTEM, following the input desktop | UAC prompts and the lock screen invisible, input dead |
| Re-encode a still desktop | a still image stuck at the quality motion left it |
| Paced, batched sends | burst loss on Wi-Fi |

## The wire

Every Helio packet is one tunnel datagram whose first 20 bytes are the
inner IPv4 header boringtun expects; boringtun checks only three of them, so
the rest carry Helio's own fields (`helio-proto/src/header.rs`, the
only file that knows byte offsets):

```
byte  0     0x45            version = 4 (required by the tunnel)
byte  1     flags           keyframe · frame_end · kind (2 bits) · recovery · lan_shards
bytes 2-3   total_len       big-endian, exact (required by the tunnel)
bytes 4-5   fragment_idx
byte  6     data_shards
byte  7     parity_shards
byte  8     fec_block_idx
bytes 9-11  frame_len
bytes 12-15 capture_ts_us
bytes 16-19 frame_id
```

| Kind | Carries | Reliability |
|---|---|---|
| Video | Frame shards and parity | Reed-Solomon per frame; lost frames repaired by invalidation or IDR |
| Audio | One 5 ms Opus packet per datagram; two parity datagrams per four | Reed-Solomon 4+2, as Sunshine; concealment beyond that |
| Input | The newest events, and the ones before them (up to 8) | Unreliable but redundant: every packet repeats recent events, the host drops what it has seen |
| Control | Session setup, loss reports, recovery requests, cursor state, pings, controllers, clipboard | Retransmitted by the sender where it matters (`SessionStart`), otherwise naturally repeated |

Input is sent unreliably on purpose: a retransmission would hold every later
event back by a round trip. Keys travel as physical scancodes (the host's
layout applies, as games expect); the mouse as relative motion, or
absolute stream pixels when the user switches to positions. The host draws
its pointer into the picture, as Sunshine does, so the pointer the client
sees is the host's own.

## Pairing and trust

No keys are copied by hand:

1. Helio finds Helio Host by mDNS (`_helio._udp`), or is given an address.
2. Helio shows a PIN; you type it into the host's Helio (This computer →
   Devices) or its web UI.
3. SPAKE2 over the PIN, with an ML-KEM-768 exchange beside it, gives both
   sides a key that only they have — the pairing is hybrid, like the tunnel,
   so a recording of it stays closed to a future quantum computer. Each side
   then sends its tunnel keys (X25519 and ML-KEM-768) under it, with the
   keys for finding each other across the internet.

A client is then known by its tunnel key; there is nothing else to steal
from the network. See [SECURITY.md](../SECURITY.md) for the model, and
[design/v3-design.md](design/v3-design.md) §4 for why the pairing is hybrid.

What a client may do once paired is the host's to say, per client, as
Apollo does: its permissions (`helio_proto::permission`, kept in
`clients.toml`) say whether it may see the screen, which input it may send,
which way the clipboard goes, and whether it may start apps, take over or
watch agents. The host checks them where each thing arrives (a session
request, an input packet, a clipboard chunk) and tells the client its set
in the session's ack and when it changes ([usage.md](usage.md#what-each-device-may-do)).

## Away from home

A host with internet access on publishes a sealed record of its public
addresses on the BitTorrent Mainline DHT, readable only by devices paired
with it; a client that wants to connect publishes an intent, and both punch
through their NATs. The router is asked to forward the port too (UPnP,
NAT-PMP), where it can. No servers are involved. See
[networking.md](networking.md).

## Sessions

```
SessionStart ─▶ display at the client's mode ─▶ video pipeline ready
            ─▶ input installed ─▶ SessionAck ─▶ frames flow
SessionEnd / client silent / pipeline died ─▶ release keys ─▶ stop video
            ─▶ the platform puts the host's displays back
```

A session is acked only once its pipeline runs, so the client never waits
on a session that cannot start. A client that goes quiet keeps its session
(and its display) for 20 seconds: coming back within that resumes the
stream. After a session, the host keeps the virtual display for a minute for
a client coming back.

## Where to read next

- [platforms/](platforms/) — what each system does, and its limits.
- [networking.md](networking.md) — the internet path.
- [ai-agents.md](ai-agents.md) — AI agents as clients.
- [benchmarks.md](benchmarks.md) — what it all costs, measured.
- [design/](design/README.md) — the designs and measurements behind this.
