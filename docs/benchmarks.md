# Benchmarks

What Helio costs, measured. First Helio 0.10.0 against Moonlight streaming
from Sunshine and from Apollo, on the same two computers and the same way
for each; then [Helio's own parts](#helios-own-parts), measured by
themselves. Every figure says what it was measured on.

## Test setup

| | |
|---|---|
| Client | 14" MacBook Pro (Mac16,8), Apple M4 Pro, 24 GB, macOS 27.2.1 (26B131); its built-in display, 120 Hz, in a scaled mode drawn at 3600x2338; 1 Gbit/s Ethernet through a USB adapter, Wi-Fi off, on AC power |
| Host | Windows 11 Pro (build 26200.9974), AMD Ryzen 7 5700X, 16 GB, NVIDIA GeForce RTX 3070 Ti (driver 617.14), High performance power plan; one 2560x1440 144 Hz monitor, switched off; 1 Gbit/s Ethernet |
| Network | both machines on one home router |
| Stacks | Helio 0.10.0 as client and host (the same build on both); Moonlight 6.2.0 as the client of Sunshine 2026.914.233613 and of Apollo 0.4.6 |
| When | Moonlight: 2026-11-20 to 2026-11-22. Helio: 2026-11-23, except where a section says otherwise |

One host stack runs at a time: the other two's services are stopped.
Every stack streams HEVC, 4:2:0, SDR, stereo sound not played on the host,
full screen, relative mouse, V-Sync on and frame pacing off (both
clients' defaults), unless a table says otherwise.

- **Sunshine** streams the physical monitor, switched to the client's
  mode for the session (`dd_configuration_option = ensure_active`). The
  monitor goes to 2560x1440 at most, so Sunshine has no 3840x2160 runs.
- **Apollo** streams its **Virtual Display** app: a display at the
  client's mode, as Helio Host's. The monitor stays in the desktop beside
  it, at 144 Hz.
- **Helio Host** adds a virtual display at the client's resolution,
  refreshing at 120 Hz for a 60 fps stream, and switches the monitor off
  for the session.
- **Helio** connects to the address Moonlight uses, so both use the same
  link.

**Equal wire.** At the same bitrate setting Helio puts 27–36% more on the
wire than Moonlight: Helio Host adds forward error correction on top of
the encoder's bitrate. So Helio runs at the setting that puts Moonlight's
rate on the wire, measured on the noise scene (series A):

| Mode | Moonlight's setting | On the wire: Moonlight + Apollo / + Sunshine | Helio's setting at equal wire | On the wire: Helio |
|---|---|---|---|---|
| 1920x1080, 60 fps | 20 Mbit/s | 20.1 / 20.2 Mbit/s | 14 Mbit/s | 19.2 |
| 1920x1080, 120 fps | 28 | 30.0 / 30.1 | 20 | 29.1 |
| 2560x1440, 60 fps | 40 | 38.6 / 38.7 | 30 | 38.4 |
| 2560x1440, 120 fps | 57 | 55.1 / 55.1 | 45 | 55.7 |
| 3840x2160, 60 fps | 80 | 81.0 / – | 61 | 80.8 |
| 3840x2160, 120 fps | 113 | 116.0 / – | 88 | 116.0 |

## How it is measured

**The probe** (`tools/bench/probe-mac`), on the client, clicks at the HID
level, where a real mouse's clicks enter, and reads every frame the Mac
composites with ScreenCaptureKit: the time from a click to the frame that
shows it, and which of the host's frames reached the screen. The host
shows `tools/bench/scene`, which draws a frame counter and lights the
screen while a button is held.

Recording the screen makes macOS composite a full-screen window instead
of handing it straight to the display, so every figure here is **while
the client's screen is recorded**, the same way for every stack. Helio's
full-screen window still goes straight to the display for stretches while
recorded, and the frames it shows then do not reach the recording: at 60
fps the probe undercounts the frames Helio shows (see [frames
shown](#frames-shown)).

**What the probe takes by itself**: with no stream, the scene drawn on the
Mac and clicked through the same probe, a click took 16.2 ms (median of
10 runs, 15.5–16.6) on 2026-11-20 before the series, and 11.5 ms (6 runs,
10.2–13.1) on 2026-11-22 after them (`runs/known-answers`,
`runs/1.0/floor-end`). The streams did not move with it: Moonlight's
clicks, measured again on 2026-11-22 in the clean runs of the bad-link
series, read within 2.0 ms of its runs of 2026-11-20 and 21, in both
directions, at 1920x1080 at 60 fps and 2560x1440 at 120 fps
(`runs/1.0-fix/cross-day.txt`).

Each table gives the median, and Helio's difference from each other stack
with its 95% interval (a two-level bootstrap over runs and clicks); where
the interval holds zero, the table says the two match.

A series is named by the folder its runs were written to, as `runs/1.0/A`:
under `target/bench/` on the Mac that ran it, and not in the repository.
The plans that made them are in `tools/bench/plans/`, and the tools in
[development.md](development.md#benchmarks).

**The two 60 fps fixes.** Helio 0.10.0 differs from its release candidate
in two ways, both for 60 fps streams: the client's full screen goes
straight to the display, and Helio Host's virtual display refreshes at 120
Hz for a 60 fps stream. A section measured on the release candidate says
so, and why its figures still hold.

## Click to frame, recorded

Series A: six modes, five runs of 38 clicks per stack (Helio:
`runs/1.0-fix/A`; Moonlight: `runs/1.0/A`). The flash drawn once per
click, as a click on a still desktop. Median (95th percentile), ms:

| Mode | Helio at equal wire | Moonlight + Apollo | Moonlight + Sunshine | Helio against Apollo | Helio against Sunshine |
|---|---|---|---|---|---|
| 1920x1080, 60 fps | 23.6 (38.4) | 28.0 (42.0) | 25.7 (54.5) | **3.0 faster** (1.3 to 6.0) | matches (−0.5, −3.6 to +0.9) |
| 1920x1080, 120 fps | 25.0 (33.4) | 28.2 (42.1) | 25.6 (46.2) | **4.2 faster** (2.0 to 5.9) | matches (−0.6, −3.5 to +0.7) |
| 2560x1440, 60 fps | 25.8 (39.0) | 29.6 (40.6) | 26.0 (62.8) | **3.5 faster** (0.6 to 5.1) | matches (−0.7, −3.9 to +3.0) |
| 2560x1440, 120 fps | 25.4 (33.3) | 28.7 (39.6) | 26.8 (51.9) | **3.7 faster** (2.0 to 6.0) | matches (−1.9, −5.1 to +0.2) |
| 3840x2160, 60 fps | 30.5 (40.9) | 33.7 (45.8) | – | **3.3 faster** (1.1 to 4.9) | – |
| 3840x2160, 120 fps | 30.4 (39.2) | 33.7 (46.2) | – | **3.3 faster** (0.7 to 4.5) | – |

Series F: the flash drawn every refresh, as in a game, five runs of 58
clicks (Helio: `runs/1.0-fix/F`; Moonlight: `runs/1.0/F-all`):

| Mode | Helio at equal wire | Moonlight + Apollo | Moonlight + Sunshine | Helio against Apollo | Helio against Sunshine |
|---|---|---|---|---|---|
| 1920x1080, 60 fps | 54.1 (67.0) | 55.0 (70.9) | 59.0 (75.8) | matches (−0.6, −2.9 to +0.8) | **4.5 faster** (3.4 to 7.0) |
| 1920x1080, 120 fps | 52.2 (62.4) | 52.6 (66.3) | 53.7 (63.7) | matches (−0.8, −3.5 to +1.5) | matches (−1.6, −4.1 to +0.9) |
| 2560x1440, 60 fps | 57.0 (74.5) | 57.8 (73.4) | 60.1 (78.4) | matches (−1.5, −4.8 to +1.7) | **3.6 faster** (0.8 to 7.5) |
| 2560x1440, 120 fps | 51.5 (64.9) | 55.8 (66.5) | 55.0 (64.8) | **3.8 faster** (1.2 to 7.0) | **2.0 faster** (0.2 to 5.1) |
| 3840x2160, 60 fps | 60.7 (75.5) | 60.4 (75.7) | – | matches (+0.5, −2.6 to +3.1) | – |
| 3840x2160, 120 fps | 58.1 (69.4) | 58.6 (71.2) | – | matches (−0.9, −3.9 to +1.2) | – |

## Frames shown

Of the frames the host drew, the share that reached the client's screen,
on the counter scene and on noise, while recorded (series A):

| Mode | Helio at equal wire | Moonlight + Apollo | Moonlight + Sunshine |
|---|---|---|---|
| 1920x1080, 120 fps | 97.7% / 97.5% | 84.4% / 87.1% | 84.0% / 85.9% |
| 2560x1440, 120 fps | 97.3% / 96.7% | 86.4% / 86.7% | 83.4% / 83.1% |
| 3840x2160, 120 fps | 97.7% / 97.6% | 85.6% / 83.8% | – |

At 60 fps the recording misses frames Helio shows: Helio's full-screen
window goes straight to the display for stretches even while recorded,
and those frames skip what the recording reads, so the table has no
60 fps rows. What the two ends count instead: Helio Host encodes 60 new
pictures a second, and the client presents 60 a second, each one
reported on the screen by macOS (the host's log over six 1920x1080
sessions, the client's statistics over two); the recording read 62–99%
of them, by run, in series A's 60 fps runs.

## Load

Series A, on noise, median of five runs:

| Mode | Client CPU, % of a core: Helio / Apollo's Moonlight / Sunshine's | Host: the stack's CPU, % of a core | Host GPU / encoder | Host board power |
|---|---|---|---|---|
| 1920x1080, 60 fps | 14.5 / 26.1 / 25.9 | 6.4 / 4.9 / 3.7 | 10% / 10%, 11% / 10%, 8% / 10% | 99.6 / 99.9 / 98.9 W |
| 2560x1440, 120 fps | 26.7 / 43.5 / 43.8 | 10.4 / 10.4 / 9.5 | 15% / 33%, 17% / 33%, 15% / 33% | 105.5 / 105.8 / 104.9 W |
| 3840x2160, 120 fps | 30.5 / 41.2 / – | 8.0 / 15.7 / – | 20% / 65%, 21% / 65% | 142.2 / 140.9 W |

## Under other conditions

At 1920x1080 at 60 fps and 2560x1440 at 120 fps, five runs each, recorded
(Helio: `runs/1.0-fix`; Moonlight: `runs/1.0`):

| Condition | 1920x1080, 60 fps: Helio against Apollo / Sunshine | 2560x1440, 120 fps |
|---|---|---|
| V-Sync off on both clients (`B`) | matches / matches | **3.4 faster** / matches |
| Encryption on for Sunshine and Apollo (`E`; Helio always encrypts) | **2.6 faster** / matches | **3.6 faster** / matches |
| Frame pacing on in both clients (`P`) | matches / 3.4 ms slower | matches / 2.6 ms slower |

**Under a demanding game** (`G`): the scene draws as fast as the host's
GPU allows, with heavy work for every pixel (90 fps at 2560x1440 with no
stream, the GPU 88–99% busy), streamed at 2560x1440 at 120 fps; no clicks.
Of the frames the scene drew, the share shown on counter / noise: Helio
95.7% / 94.0%, Apollo 93.5% / 90.2%, Sunshine 94.1% / 93.2%. Freezes a
minute on noise: Helio 246.6, Apollo 410.2, Sunshine 281.3.

## Starting a stream

Measured on 2026-11-21, on the release candidate before the two 60 fps
fixes. At this mode Helio Host is the same in both builds: a
120 fps stream gets a 120 Hz display either way. The client's full-screen
window changed between them. From starting the client to its first video
in the client's log (Helio's decoder built from the first keyframe,
Moonlight's first video packet), 2560x1440 at 120 fps, the host's monitor
on (`runs/1.0/S-monitor-on`).

When a session ends, Helio Host keeps its virtual display, with the
host's monitor off, for 60 s, for a client that comes back
(`LINGER` in `helio-host/src/platform.rs`). A start in that minute skips
making the display and taking the monitor out of the desktop. Each
condition's first start came after the host's service had started, with
nothing kept; every later start came 3 to 25 s after the session before
it. So Helio's two kinds of start are apart:

| Before the start | Helio, first start: nothing kept | Helio, the later starts: median (slowest) | Moonlight + Apollo: median (slowest) | Moonlight + Sunshine: median (slowest) |
|---|---|---|---|---|
| The monitor awake, 10 s after the last session (20 starts) | 1.14 s | 0.42 s (0.44) | 2.82 s (2.89) | 4.26 s (16.06) |
| The monitor asleep (10) | 1.02 s | 1.52 s (1.74)* | 2.98 s (3.04) | 4.53 s (4.96) |
| Right after the last session, 3 s (10) | 1.23 s | 0.42 s (0.43) | 2.84 s (2.88) | 4.24 s (16.60) |

Apollo's and Sunshine's first start of a condition took as long as the
rest (Apollo 2.84, 2.99 and 2.88 s), except Sunshine's first start after
its service started (16.06 and 16.60 s, the slowest figures above), so
one column each covers both. All 120 starts streamed.

\* With the monitor asleep, the second start took 0.41 s and the other
eight 1.51–1.74 s; the bench's logs do not say whether Helio Host had
kept the display for them.

## Long sessions

Measured on 2026-11-22, on the release candidate before the two 60 fps
fixes, where Helio Host is the same at this
mode, as above: one stream of an hour per stack, 2560x1440 at 120 fps on
the counter scene, read for 20 s every minute (`runs/1.0/K`). Every stack
streamed the whole hour, and neither side's memory grew:

| | Helio at equal wire | Moonlight + Apollo | Moonlight + Sunshine |
|---|---|---|---|
| Minutes streamed | 60 of 60 | 60 of 60 | 60 of 60 |
| Frames shown, mean of the minutes | 96.5% | 85.2% | 85.0% |
| Client memory, first minute → last | 103 → 103 MB | 226 → 225 MB | 225 → 224 MB |
| Host: the stack's memory, first → last | 124 → 123 MB | 140 → 132 MB | 157 → 130 MB |

## Bad links

The link impaired on the Mac (macOS's dummynet, both directions), the
same for every stack, checked against known answers before each stack's
runs (`runs/1.0/L-check`; `runs/1.0-fix/L-check` before Helio's). Three
runs per setting, recorded (Helio: `runs/1.0-fix/L1`, `L3`; Moonlight:
`runs/1.0/L1`, `L3`).

**Loss** (`L1`): random loss in both directions, the counter scene.
Frames shown (hitches in brackets):

| Mode | Stack | Clean | 0.5% | 1% | 2% | 5% |
|---|---|---|---|---|---|---|
| 1920x1080, 60 fps | Helio at equal wire* | 93.3% (7.8%) | 96.2% (4.1%) | 100.0% (2.8%) | 93.1% (7.6%) | 99.9% (0.9%) |
| 1920x1080, 60 fps | Moonlight + Apollo | 65.2% (51.3%) | 85.3% (17.8%) | 88.6% (13.4%) | 87.9% (14.6%) | 86.7% (15.9%) |
| 1920x1080, 60 fps | Moonlight + Sunshine | 86.4% (16.3%) | 89.3% (12.3%) | 89.8% (11.9%) | 89.0% (13.0%) | 88.5% (13.6%) |
| 2560x1440, 120 fps | Helio at equal wire | 97.6% (4.6%) | 97.4% (2.5%) | 96.8% (3.3%) | 97.6% (2.4%) | 96.1% (4.2%) |
| 2560x1440, 120 fps | Moonlight + Apollo | 84.0% (18.7%) | 83.2% (19.7%) | 85.8% (16.1%) | 88.4% (12.9%) | 84.3% (18.0%) |
| 2560x1440, 120 fps | Moonlight + Sunshine | 86.0% (16.2%) | 86.4% (15.5%) | 84.7% (17.9%) | 86.1% (15.7%) | 85.2% (16.9%) |

At 2560x1440 at 120 fps Helio shows 96–98% of the frames from the clean
link to 5% loss. Moonlight's frames shown stay where they are on the
clean link, where recording costs it frames, so the loss it recovers
from does not show in this figure. \* At 1920x1080 at 60 fps Helio's
share moves from run to run with the recording, not with the loss (93.3%
clean, 99.9% at 5%): the recording misses frames Helio shows (see
[frames shown](#frames-shown)), so the row is a floor.

**Delay with loss** (`L3`): 1% loss with 10, 20 and 40 ms each way.
Frames shown: Helio 96.6–98.8% at 2560x1440 at 120 fps at every delay,
and 95.5–97.5%\* at 1920x1080 at 60 fps; Moonlight 84–88% and 70–91%,
as on the clean link.

**The link narrowing mid-stream** (`L4n`): 60 Mbit/s, then 20 for 20
seconds, then 60, at 2560x1440 at 60 fps on noise (about 38.5 Mbit/s on
the clean link, every stack). Over the 40 s read:

| | Helio at equal wire | Moonlight + Apollo | Moonlight + Sunshine |
|---|---|---|---|
| Frames shown, clean | 80.7%* | 87.3% | 89.1% |
| Frames shown, narrowing | 84.8%* | 40.5% | 20.7% |
| Freezes a minute, narrowing | 17.0 | 55.3 | 32.5 |
| Arriving at the Mac, narrowing | 24.2 Mbit/s | 42.0 | 44.1 |

\* At 60 fps, while recorded: the recording misses frames Helio shows
(see [frames shown](#frames-shown)), so Helio's share is a floor.

Helio Host sent less when the link narrowed; Sunshine and Apollo sent
more than it carried (the Mac's interface counts what arrives before the
link drops it).

## The host at rest

Each stack's host running with no session, ten minutes each
(`runs/1.0/idle`, on the release candidate before the two fixes; they
change only sessions): Helio Host
0.41% of a core and 60 MB; Sunshine 0.00% and 63 MB; Apollo 0.01% and
59 MB.

## Not measured

- Picture quality (PSNR, SSIM, VMAF at equal wire): the tools are not
  built.
- The direct path, with nothing recording the screen. The method is
  built: a camera films the Mac's screen and keyboard at 240 frames a
  second while the probe clicks without recording, lighting the Caps Lock
  key's LED with each click (`tools/bench/film-clicks`). Helio 0.10.0 is
  not filmed.
- Other clients and hosts: Windows and Linux clients, a Linux host.

## Helio's own parts

Helio measured by itself, part by part: micro-benchmarks of its code
paths, the client's presentation, and the stream's own figures. Unless a
section says otherwise:

| | |
|---|---|
| Client | 14" MacBook Pro, Apple M4 Pro, macOS; its built-in display (120 Hz ProMotion), on Wi-Fi |
| Host | Windows 11 25H2 PC, NVIDIA RTX 3070 Ti, SudoVDA; on the same LAN (Wi-Fi at first, then 1 Gbit/s Ethernet) |
| Stream | 3024x1890 (the MacBook's display below the notch) at 120 fps, 100 Mbit/s, HEVC, 7.1 audio: a demanding Moonlight setting |
| Content | a full-screen scene of random noise at 60 fps, which fills the bitrate; or moving boxes, for decode damage |

- **Micro-benchmarks** (Criterion, release profile):

  ```sh
  cargo bench -p helio-transport      # pq-boringtun handshake and data plane
  cargo bench -p helio-proto          # packetize, FEC, reassembly, audio
  cargo bench -p helio-decode         # the Annex-B split before VideoToolbox
  ```

- **The stream**: the statistics overlay (Ctrl+Alt+Shift+S, or
  `helio stream NAME --stats`) shows the same figures as Moonlight's, plus the
  host's capture to the client's screen. For a log of them, one line a
  second: `RUST_LOG=info,helio_client::stats=debug helio stream NAME ...`. The
  host logs a line a second too (`helio-host.log`: frames, capture to encoded,
  Mbit/s, recoveries).
- **The client's presentation alone** (macOS), with no host: synthetic
  frames through the stream window and renderer, a line a second of frames
  shown and decode to glass (the options are at the top of the file):

  ```sh
  cargo run --release -p helio-client --example mac-present -- --fps 120 --secs 10
  ```

- **Host capture to the client's screen** is measured directly: the two
  machines' clocks are related through the ping exchange the stream
  already runs (every 500 ms; as in NTP, the sample with the shortest round
  trip of the last 16 sets the offset).
- **Impairing the link** without network tools: see
  [development.md](development.md#test-hooks) (`HELIO_TEST_LOSS`,
  `HELIO_TEST_WIRE_LOSS`, ...).

Micro-benchmark numbers below: Apple M4 Pro, rustc 1.98.1, 2026-09-27.

## pq-boringtun (static ML-KEM authentication, 1280-byte path)

In memory, no sockets: exactly the per-peer state an `Endpoint` keeps.

| What | Time | Notes |
|---|---|---|
| Complete handshake, both sides | 364 µs | initiation is 3 datagrams, 2,600 bytes (segmented) |
| Encrypt a 1200-byte packet | 0.63 µs | 1.76 GiB/s |
| Encrypt + decrypt, 1200 bytes | 1.38 µs | 827 MiB/s |
| Encrypt + decrypt, 100 bytes (input, control) | 0.36 µs | |

At an 80 Mbit/s stream (~8,300 datagrams/s) encryption is ~5 ms of CPU per
second, about 0.5% of one core, per side. A rekey every two minutes costs
0.36 ms of CPU and, being off the data path, no stall.

## Data path

Since 2026-09-30 (the optimization pass below; the earlier figures in
brackets):

| What | 2 KB P-frame | 42 KB P-frame | 600 KB IDR |
|---|---|---|---|
| Packetize (split + RS parity) | 0.20 µs (0.44) | 5.1 µs (7.3) | 126 µs (156) |
| Reassemble, no loss | 0.08 µs (0.48) | 1.2 µs (5.6) | 25 µs (88) |
| Reassemble, one datagram lost per FEC block | 101 µs (102) | 119 µs (126) | 675 µs (763) |

`cargo bench -p helio-decode` measures the Annex-B split every frame
goes through before VideoToolbox: 2.5 µs for a 120 KB frame (a SIMD
start-code search), where the byte-at-a-time loop took 33 µs.

42 KB is a busy 1080p60 frame at 20 Mbit/s; 600 KB a 4K keyframe.

Audio: packetizing a 5 ms Opus packet costs 95 ns; recovering a block of four
with one lost, 101 µs.

### What the numbers say

- **Recovery has a fixed cost of ~0.1–0.2 ms.** reed-solomon-simd is Leopard
  RS over GF(2^16): locating erasures takes a transform over the whole field,
  whatever the block size. For video that is small beside a frame interval
  (8.3 ms at 120 fps). For 4+2 audio blocks a GF(2^8) Cauchy code, which is what
  Sunshine and Moonlight use (nanors), would take microseconds; worth doing if
  recovery ever shows up in a profile.
- **The first encoder pays ~6 ms of table setup.** Host and client now warm the
  FEC tables at startup (`fec::warm_up`), so a session's first keyframe does not.

## Decode to glass (client presentation)

Measured 2026-09-27 on the MacBook's built-in 60 Hz display, 1080p60 HEVC
over the LAN. "Decode to glass" is from the decoder's output to the time
Metal reports the drawable reached the display (`presentedTime`), so it
includes the compositor.

| Mode | Content | Decode to glass | Frames shown |
|---|---|---|---|
| Full screen, present on decode (default) | 60 fps noise | 7.2 ms | 57 of 61 |
| Full screen, display link (the first frame pacing) | 60 fps noise | 28.8 ms | 60 of 60 |
| Full screen, present on decode | 31 fps noise | 6.3 ms | 31 of 31 |
| Full screen, display link | 31 fps noise | 20.1 ms | 28 of 31 |
| Window, present on decode | 31 fps noise | 21.1 ms | 30 of 31 |

The display link hands out its drawable a refresh or more before the refresh
it targets, so a frame decoded just after the callback waits almost two. In a
window it aims three refreshes ahead (50–60 ms). Presenting on decode, as
Moonlight does by default, loses a frame now and then when two land in one
refresh; frame pacing is an opt-in setting, as in Moonlight.

The rest of the pipeline in the same runs, from the overlay: host capture to
encoded 2.2–2.6 ms, in flight about half the ~9 ms Wi-Fi round trip, first
packet to complete frame 1–1.6 ms, decode 1.5–2 ms.

### Recording the screen (macOS client)

Measured 2026-10-07 with `helio-client`'s `mac-present` (synthetic frames
through the stream window and renderer, no host or decoder; each frame
handed over up to 3 ms late), on the MacBook's built-in 120 Hz display at
3600x2338, V-Sync on, full screen. "Recorded" is `screencapture -V` running
through the whole run; "a window over it" is a small window above the
stream for 4 s, as a recorder's controls or a dialog would be. Before is a build
that let one frame at a time wait for the glass full screen; after,
three while the trips to the glass say the picture is composited.

| Case | Before: shown | Before: decode to glass | After: shown | After: decode to glass |
|---|---|---|---|---|
| Nothing over it, 60 fps | 60 of 60 | 6.1 ms | 60 of 60 | 6.0 ms |
| Nothing over it, 120 fps | 120 of 120 | 10.1 ms | 120 of 120 | 10.0 ms |
| Recorded, 60 fps | 40 of 60 | 34.0 ms | 60 of 60 | 22.2 ms |
| Recorded, 120 fps | 40 of 120 | 28.6 ms | 119 of 120 | 25.6 ms |
| A window over it, 60 fps | 40 of 60 | 30–34 ms | 57–60 of 60 | 20–23 ms |
| A window over it, 120 fps | 39–48 of 120 | 25–28 ms | 111–119 of 120 | 24–29 ms |

Composited, a drawable reaches the glass 17.8–27.4 ms after it is committed,
against 2.8–11.3 ms straight to the display; with one at a time a frame is
shown every third refresh. After the window over it is gone, the stream is
straight again within 1–2 s. A recording that starts while the stream is
already full screen is composited for 1.5–2 s; macOS then takes the stream
straight to the display again, with some frames a refresh late: 105–117 of
120 shown at 120 fps, before as after (at 60 fps, all of them). Real
streams with a recording running, through CleanShot X: 39 of 60 and 40 of
120 shown before.

### One app, with glass (macOS client)

Measured 2026-10-28 with `mac-present` (frames handed over on time), on the
MacBook's built-in 120 Hz display at 3600x2338, V-Sync on, 10 s runs.
Before is Helio with Helio Host's window still an app of its own; after is the one
app, in asides' glass. The stream's code (`helio-client`, `helio-decode`,
`helio-proto`) and its dependencies' versions are the same in both, so
this checks that nothing new reaches the stream: "glass window beside it"
is Helio's main window open on a settings page next to the stream's.
Figures are a run's steady seconds (seconds composited because something
was over the stream for a moment are left out).

| Case | Before: decode to glass | Before: shown | After: decode to glass | After: shown |
|---|---|---|---|---|
| Full screen, 60 fps | 5.8–6.6 ms | 99.2–99.8% | 5.7–7.8 ms | 100% |
| Full screen, 120 fps | 6.0 or 13.3 ms | 99.7–100% | 7.2 or 13.6 ms | 99.8% |
| Window, 120 fps | 26.2 ms (1 run) | 100% | 23.3–31.4 ms (5 runs) | 100% |
| Window, 120 fps, glass window beside it | | | 25.7–28.6 ms (3 runs) | 99.8–100% |

The synthetic frames come on a clock of their own, not the display's, so
each run lands at a phase of its own against the refresh and keeps it: a
run's figure is steady, and runs differ by up to one refresh (8.3 ms),
before as after (full screen at 120 fps, a run sits at about 6–7 or
13–14 ms). The glass window's blur is the WindowServer's, for that window
alone; the stream's window stays opaque. `mac-present` does not load
`helio-ui`; in the app, its change to GPUI's blending
(`glass::blend_alpha_over`) acts once, as Metal makes a pipeline, and
leaves the stream's renderer's pipelines as they are.

## Capture to glass

The overlay's last line, "Host capture to screen", measures it directly:
from the host picking up a captured frame to Metal reporting it on the Mac's
display. The two machines' clocks are related through the ping exchange
the stream already runs (every 500 ms): a ping reply's header carries the host's
clock when it replied, and as in NTP the sample with the shortest round trip
of the last 16 sets the offset.

Measured 2026-09-27, 1080p60 HEVC at 20 Mbit/s, 60 fps noise, full screen
(present on decode), Mac on Wi-Fi to the host on the LAN: **17–24 ms per
second, typically 18 ms**, which matches the sum of the stages above. Peaks of
80–200 ms are the Mac's Wi-Fi stalling (below).

Not included: the host compositor's time from an application's present to
the capture (DWM), and the Mac panel's scanout and pixel response. Those
need a photodiode or a 240 fps camera on both screens.

## Long sessions and loss

35 minutes, 1080p60 moving scene with audio, every received datagram
(handshakes included) dropped at 2% in bursts of 3 and handshake datagrams
at a further 30% (`HELIO_TEST_WIRE_LOSS=2:3 HELIO_TEST_HANDSHAKE_LOSS=30`
on the client), so each of the ~17 rekeys had to survive lost messages:

- No session interruptions: no IDR requests, no renegotiation, no lost contact.
  (Keys are rejected after 3 minutes, so the session outliving that is itself
  the proof that every rekey landed.)
- 123,599 of 125,460 frames shown (98.5%); 339 frames lost to the injected
  loss, each repaired by a reference invalidation. 85 of 2,091 seconds showed
  fewer than 55 frames.
- Audio: 0.9% of packets concealed.

The test Mac's Wi-Fi on its network stalls for 50–200 ms every 20–30 s, loss or
not: audio packets the host sent 20 ms apart arrive up to 205 ms apart, while
the client's receive loop never spends more than 1–3 ms on a datagram. Each
such stall empties the audio buffer (about 2 underruns a minute in the long
run). AWDL (AirDrop, Continuity) was up on that Mac and is a common cause of
exactly this pattern; not yet confirmed here by turning it off
(`sudo ifconfig awdl0 down`, until the next reboot).

## Optimization pass (2026-09-30)

Helio (Mac, Wi-Fi) against Helio Host (the Windows host, Ethernet): 3024x1890 at
120 fps, 100 Mbit/s, HEVC, full screen, 60 fps noise on the host, about 114
Mbit/s on the wire. Each figure is the
average of a run's middle 24-26 seconds; A and B were run back to back.

| Change | Measured |
|---|---|
| Client: receives batched (`recvmsg_x`, 32 at a time) | first packet to complete frame 2.4-2.6 ms -> 1.1-1.2 ms; the same with batches of 8 or 4 as with none: Wi-Fi hands over bursts |
| Client: SIMD Annex-B split, the AVCC sample written straight into the block buffer | decode 2.36-2.51 -> 2.24-2.36 ms |
| Client: reassembly into reused buffers (no allocation per datagram) | see the table above |
| Host: sends batched, 16 datagrams per call (USO on Windows, GSO on Linux, `sendmsg_x` on macOS); packetizing into one reused buffer | helio-host.exe 24-25% of a core -> 13-18%, frame delivery unchanged (1.6 ms) |
| Host: 64 datagrams per call (a whole pacing group) | the same CPU, but frames 1 ms later: a call that large held the sender until it was all on the wire |
| LAN-sized shards (1400-byte payloads on the local network, as Moonlight's 1392) | the client's receive thread 33% -> 31% of a core at the same rate; latency unchanged |
| Client: user-interactive QoS on the stream's threads | no change measured (the receive thread still runs mostly on efficiency cores); real-time scheduling did no better |
| Client: socket marked for Wi-Fi's voice queue (`SO_NET_SERVICE_TYPE`, as Moonlight) | no change in round trip measured on the test network |

A five-minute run with all of it: 117.9 fps received, 111.7 shown, 2 frames
lost (both recovered), memory flat.

What the client's CPU goes to at this rate (Instruments, 8 s): the receive
thread about 31% of a core, nearly all of it in the kernel (`recvmsg_x` and
what comes with waking); ChaCha20-Poly1305 1%; Metal's command submission
3%; drawing 2%; VideoToolbox's callbacks 2%; audio 1%. Fewer, larger
datagrams saved less than their count suggests: the kernel's cost follows
the Wi-Fi bursts more than the datagrams.

Idle: Helio Host's session thread ticks every 100 ms between sessions (was 8 ms);
Helio polls its host list every 30 s while in the background (every 4 s in
front): Helio.app hidden went from about 160 wakeups a second to about 90.

Switches for diagnosis: `HELIO_SEND_BATCH=N` (0: one datagram per call),
`HELIO_RECV_BATCH=N` (0: one per call), `HELIO_LAN_SHARDS=0` (client:
path-sized datagrams on the LAN too), `HELIO_SERVICE_CLASS=0` (see
[cli.md](cli.md#environment-variables)).

## Trackpad scrolling (2026-11-04)

How far a Mac's trackpad scrolls a Windows host. A gesture recorded on the
MacBook Pro (795 points in 1.09 s, fingers and glide, 108 events) was replayed
with its timing as scripted input (`HELIO_TEST_INPUT`) through a real stream:
3024x1890 at 60 fps, HEVC, over a VPN on Wi-Fi, to the Windows PC with that
display at 225%, onto a page in Microsoft Edge that logged its position. A
point is two stream pixels at this size, so as far as the fingers is 1590 stream
pixels.

| Input | Sent | Page moved, in stream pixels | Against what was sent |
|---|---|---|---|
| The gesture at 3 wheel units a point, as a trackpad went until this was measured | 2385 units | 4472 | 2.81× the fingers |
| The gesture as a distance | 1590 pixels | 1590 | 1.00× |
| Up, one event | 450 pixels | 450 | 1.00× |
| 100 events of one pixel, 16 ms apart | 100 pixels | 99 | 0.99× |
| Right, then left, one event each | 900 pixels each | 900 each | 1.00× |
| Down, one event | 2250 pixels | 1697 | 0.75× |

A wheel notch (120 units) moved the page 100 CSS pixels, Chromium's figure. The
one large event falling short is not explained; the largest event in the
recording is 45 points, 90 stream pixels.

What apps do with parts of a notch, measured with wheel messages sent on the
same PC with its own display at 100%:

| App | Input | Result |
|---|---|---|
| Microsoft Edge | 1200 units in steps of 1 to 120 | 998 to 1002 CSS pixels, whichever the step |
| Windows' Settings | Steps of 5 units, 170 ms apart | Moved at every step |
| File Explorer | Steps of 5 units, 180 ms apart, 300 units in all | Moved at 120 and at 240 units, nowhere else |

Recorded gestures replayed into File Explorer, in the units each path gives:

| Gesture | Path | Units | Notches in them | Explorer moved, in notches |
|---|---|---|---|---|
| 795 points | 3 units a point | 2385 | 19.9 | 18 |
| 795 points | Distance, display at 175% | 1090 | 9.1 | 8 |
| 795 points | Distance, display at 225% | 848 | 7.1 | 6 |
| 37 points, gentle, 2.9 s | 3 units a point | 111 | 0.9 | 1 |
| 37 points, gentle, 2.9 s | Distance, display at 175% | 50 | 0.4 | 1 |
| 37 points, gentle, 2.9 s | Distance, display at 225% | 39 | 0.3 | 0 |

The gentle gestures ran one after another. Explorer appears to carry parts of a
notch from one gesture to the next, which would be how a gesture under a notch
moved it a whole one, and why the same small movement sometimes moves it and
sometimes does not.

Not measured: other apps that use whole notches, Explorer's horizontal
scrolling, and a touchpad on a Windows client.

### On a Linux host

How far the same 795-point gesture moves a browser on a Linux host: 1590 stream
pixels, replayed as scripted input through a real stream at 3024x1890, 30 fps,
H.264 in software, debug build, in a container (Ubuntu 24.04 on Arm, no GPU),
from a Linux client on another X screen. The browsers are Chromium 156 and Firefox 157,
on a page that logged its position.

- **X11:** Xvfb with no window manager, the scale set through `xsettingsd` with
  the values the desktops' daemons write (below).
- **GNOME:** mutter 46.2 headless, browsers native to Wayland, on the session's
  virtual monitor at the scale a client asks for, set for the test (a Linux
  client does not send one). 150% takes mutter's experimental
  `scale-monitor-framebuffer`. A CSS pixel is counted as the monitor's scale in
  stream pixels: the page's own device pixel ratio stayed at 1 in some runs,
  opened as the monitor changed.

| Desktop | Scale | Chromium: 100 pixels a notch, scale ignored | Chromium: 120 pixels a notch at the desktop's scale | Firefox: 100 pixels a notch, scale ignored | Firefox: 120 pixels a notch at the desktop's scale |
|---|---|---|---|---|---|
| X11 | 100% | 1.13× | 0.98× | 1.08× | 0.93× |
| X11 | 150% | 1.70× | 0.91× | 1.64× | 0.88× |
| X11 | 200% | 2.26× | 0.91× | 2.15× | 0.86× |
| GNOME | 100% | 1.13× | 0.98× | 1.08× | 0.93× |
| GNOME | 150% | 1.70× | 0.91× | 1.75× | 0.88× |
| GNOME | 200% | 2.26× | 0.91× | 2.15× to 3.30×, four runs | 0.86×, three runs |

These desktops take whole notches only, so a gesture stops short by up to a
notch and the rest waits for the next scroll: at 200% a notch is 240 stream
pixels, 1590 of them are 6.6 notches, and six moved the page. Over several
scrolls it adds up: at 200% on X11, scripted scrolls of 1000 down, 400 up and
600 down (1200 stream pixels in all) moved Chromium 960, 240 and 480, 1200 in
all. Firefox's run-to-run spread at 200% under GNOME is not explained: at 100
pixels a notch it moved further than the notches sent, which X11 did not show.

How far a notch goes, at the scales above (sideways under GNOME, at 100% and
200%):

| Browser | Desktop | Down, in CSS pixels | Sideways, in CSS pixels |
|---|---|---|---|
| Chromium | X11, GNOME | 120 | 120 |
| Firefox | X11 | 114 to 116 | 20 to 21 |
| Firefox | GNOME | 114 to 116 | 60 |

Firefox's sideways notch is shorter than any figure that suits its vertical one:
at 120 pixels a notch it goes about a sixth as far as the fingers on X11, and half
as far under GNOME.

Where the scale on an X desktop comes from: real settings daemons on Xvfb, wheel
buttons clicked with `xdotool`. The DPIs are as the settings give them over 1024.

| Daemon | Set to | `Xft/DPI` | `Gdk/UnscaledDPI` | `Gdk/WindowScalingFactor` | `Xft.dpi` in the X resources | Both browsers' device pixel ratio |
|---|---|---|---|---|---|---|
| None | `Xft.dpi: 192` with `xrdb` | none | none | none | 192 | 1 |
| `gsd-xsettings` 46.0, under mutter on X11 | Scale 2 | 192 | 96 | 2 | 192 | 2 |
| `gsd-xsettings` 46.0 | Text 1.25 | 120 | 120 | 1 | 120 | 1.25 |
| `xfsettingsd` 4.18.4 | Window scale 2 | 96 | none | 2 | 96 | 2 |
| `xfsettingsd` 4.18.4 | DPI 144 | 144 | none | 1 | 144 | 1.5 |
| `xsettingsd` 1.0.2 | Window scale 2 alone | none | none | 2 | none | 2 |
| `xsettingsd` 1.0.2 | `Xft/DPI` of 192 and window scale 2 | 192 | none | 2 | none | 4 |
| `xsettingsd` 1.0.2 | `Xft/DPI` of -1 | -1, as written | none | none | none | 1 |

So the ratio is the window scale times `Gdk/UnscaledDPI` (else `Xft/DPI`) over
96. A desktop's `Xft.dpi` alone neither sets it nor always follows it.

Not measured: KDE, the desktop portal's path, a real trackpad, and apps other
than browsers.

### On a Mac host

How far the same 795-point gesture moves a Mac host (2026-11-05): replayed
as scripted input from a client on the same Mac, the 14" MacBook Pro above
(M4 Pro, macOS 26.5.2), release build, 3024x1890 at 60 fps, HEVC at 40
Mbit/s. The virtual display ran at 200%, 1512x945 points, so a point is two
stream pixels and the gesture is 1590 of them, and sat beside the built-in
display (`--keep-host-displays`). The targets were a native AppKit scroll view
(`NSScrollView`), and Playwright 1.64's builds of Chromium 156, Firefox 157
and WebKit, on a page that logged its position. A CSS pixel was a point in
each.

| Input | Sent | Should move, in points | AppKit view | Chromium | Firefox | WebKit |
|---|---|---|---|---|---|---|
| A wheel notch | 120 units | 30 | 30 | 30 | 30 | 30 |
| The gesture at 3 wheel units a point, as a trackpad went until the Windows measurement above | 2385 units | 596, 0.75× the fingers | 596 | 596 | 596 | 596 |
| The gesture as a distance | 1590 pixels | 795 | 795 | 795 | 795 | 795 |
| Down, one event | 2250 pixels | 1125 | 1125 | 1125 | 768 | 1125 |
| Up, one event | 450 pixels | 225 | 224 | 224 | 224 | 224 |
| 100 events of one pixel, 16 ms apart | 100 pixels | 50 | 49 | 49 | 49 | 49 |
| Right, then left, one event each | 900 pixels each | 450 each | 450 each | 450 each | 450 each | 450 each |

The point missing from two rows is not lost. The notches before left a
quarter of a point over, and the host keeps what is short of a point for the
next scroll (`helio-input/src/macos.rs`). Firefox moved the one large event
768 points, a little under the height of its view (800 points). A trackpad's
events are far smaller: the largest in the recording is 45 points. At 1920x1080, where the
virtual display is not HiDPI and a stream pixel is a point, the AppKit view
moved 1590 points for the gesture, 1.00×.

The host posts its scrolls without a gesture's phases (began, changed,
ended, momentum): the AppKit view saw none. So apps that act on the phases,
such as the rubber band at the end of a page or swiping between pages,
see a plain scroll. That was not tried.

Not measured: a real trackpad, from a Mac client on another machine;
the default arrangement, where the virtual display mirrors the others;
Safari itself and browsers as installed; and other apps.
