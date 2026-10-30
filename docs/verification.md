# How it's built and verified

What stands behind Helio, and what does not, so you can decide how far to
trust it. The figures here were taken at commit `c2e75ab` on 2026-10-30.

## Built with AI coding agents

Helio is written with AI coding agents, under the rules in
[AGENTS.md](../AGENTS.md): how code, comments, documentation and commits are
written, what has to be measured, what has to stay private. The history
shows it: all 162 commits on `main` that no bot made carry a
`Co-Authored-By` line naming Claude.

## How changes are reviewed

Every change reaches `main` through a pull request. A review agent checks
it, then the maintainer reads it, and it merges once CI passes.

GitHub enforces the pull request and the four CI checks below, for
administrators too. A pull request merges by rebase only, with its
conversations resolved, so `main`'s history is linear, and `main` takes no
force push or deletion. GitHub does not enforce the review: branch
protection asks for no approvals, and the reviews happen outside GitHub, so
none of the 51 pull requests shows one.

The rest of what protects a release:

| What | Rule |
|---|---|
| Actions | Only GitHub's own and `Swatinem/rust-cache` may run, each pinned to a commit |
| Version tags | Once made, a `v*` tag cannot be moved or deleted |
| The `release` environment, with the Windows signing secrets | Deploys from `main` only |
| Secrets in a push | Secret scanning and push protection are on |
| Vulnerable dependencies | Dependabot alerts are on |

## How changes are tested

### CI

CI runs on every pull request and on `main` after each merge
([ci.yml](../.github/workflows/ci.yml)):

| Check | Runner | What it does |
|---|---|---|
| Formatting | `ubuntu-latest` | `cargo fmt --all -- --check` |
| macOS | `macos-15`, Apple silicon | clippy with warnings as errors, the tests, and the app packaged with the host and `helio` inside it |
| Windows | `windows-latest`, x64 | The same, and the installer run for real: install, turn hosting on, install again, uninstall, each step checked. The drivers must be as they were afterwards; a driver the runner refuses is reported in the log, not a failure |
| Linux | `ubuntu-24.04`, x86_64 | The same as macOS, the host's crate on its own, and the fuzz targets built |

The release workflow ([release.yml](../.github/workflows/release.yml)) also
builds for Intel Macs. It signs and notarizes the macOS apps, signs the
Windows programs, and runs the same installer test on each Windows package.

### The tests

The workspace has 615 test functions, and each system runs the ones that
apply to it:

| System | Passed | Ignored | Failed |
|---|---|---|---|
| macOS | 519, 4 of them doc tests | 2 | 0 |
| Windows | 556 | 6 | 0 |
| Linux | 503 | 3 | 0 |

To count them:

```sh
git grep -E '^[[:space:]]*#\[(tokio::)?test\]' -- 'helio-*/*.rs' ':!helio-proto/fuzz' | wc -l
```

The 9 ignored tests need something a CI runner lacks: SudoVDA (5), a KDE
Plasma session (2), a Mac with an AV1 decoder (1), a Developer ID
identity (1).

### Beyond the unit tests

| What | Does |
|---|---|
| `tools/linux/loopback-test` | Streams a Linux host to a Linux client in a container, with scripted input and a tone |
| `tools/linux/virtual-display-test` | GNOME's virtual display in a headless mutter |
| [Test hooks](development.md#test-hooks) | Script input and impair the link, on real machines: loss, bursts, blackouts, lost handshakes |
| `HELIO_UI_DEMO` | Drives the window and saves screenshots ([ui.md](ui.md#checking-it-without-clicking)) |
| [Benchmarks](benchmarks.md) | Criterion benchmarks of the data path, and real streams, each with its conditions |

### Not in CI

These run by hand, when a change needs them, so a pull request's green
checks say nothing about them:

- the 9 ignored tests;
- running the fuzz targets: CI only builds them;
- the loopback and virtual display tests;
- the UI screenshots;
- a real stream between real machines;
- anything on an Intel Mac, which the release workflow builds and notarizes
  but runs no test on;
- Helio's minimum Rust version, 1.96: CI builds with the current stable
  Rust.

## Fuzzing

The parsers that read the stream's network input first have
[cargo-fuzz](https://github.com/rust-fuzz/cargo-fuzz) targets, in
`helio-proto/fuzz`. Each ran for two minutes without a crash, on an Apple
silicon Mac with rustc 1.99.0 and no sanitizer (`cargo fuzz run -s none`):

| Target | Parser | Runs |
|---|---|---|
| `depacketize` | Video datagrams: `Header::decode` and reassembly (`Reassembler::push`) | 425,148 |
| `control` | Control messages (`Control::decode`) | 183,799,658 |
| `input` | Input packets (`input::decode`) | 176,793,828 |

Other parsers of network input have no target, and rest on unit tests and
bounds checks: audio datagrams (`helio-proto/src/audio.rs`), clipboard
messages (`helio-proto/src/clip.rs`), pairing messages
(`helio-pairing/src/pair.rs`), STUN replies (`helio-nat/src/stun.rs`) and
rendezvous records (`helio-nat/src/rendezvous.rs`).

## What is and isn't verified

Each platform's page has the detail, with its measurements:
[macOS](platforms/macos.md), [Windows](platforms/windows.md),
[Linux](platforms/linux.md).

| | Verified | Not verified |
|---|---|---|
| macOS client | A 14" MacBook Pro with an M4 Pro: the figures in [benchmarks.md](benchmarks.md), HDR and 4:4:4 from a Windows host | Intel Macs; a 59.94 Hz display |
| macOS host | The same Mac, client and host on one machine: loss recovery, restarts, HDR, 5.1 and 7.1 through BlackHole 16ch | Intel Macs |
| Windows host | Windows 11 25H2 with an RTX 3070 Ti and SudoVDA: the same figures, long sessions under injected loss, controllers, Wake-on-LAN. Without NVENC or SudoVDA: a still desktop at 1024x768 in an Arm virtual machine. Installing and removing the drivers: on GitHub's x64 runner | AV1 encoding (that GPU has none); a real AMD or Intel GPU; motion at a full frame rate in software |
| Windows client | Streaming the same PC | Keyboard, mouse and controllers streaming another machine; on-glass timing on a real monitor |
| Linux client | An Ubuntu 24.04 container: Linux to Linux, and a Windows host over a VPN | Real hardware: VA-API, controllers, a real compositor, keyboard and mouse through the window |
| Linux host | The container, with X11 and the portal in a nested GNOME. GNOME's virtual display on headless mutter 46 and 50. KDE's on Plasma 6.6.4 in a virtual machine, with whole streams to a Linux client and a Mac | Real hardware: VA-API, NVENC. GNOME with real monitors; the wheel under Wayland; wlroots desktops; sharing the clipboard from a KDE host |

Feature by feature, against Moonlight + Sunshine/Apollo:
[parity.md](parity.md). What Helio does not do:
[known limitations](../README.md#known-limitations).

## Not audited

Helio has not been externally audited: not its code, not its pairing, not
its tunnel. [SECURITY.md](../SECURITY.md) sets out its security model, and
what is out of scope.

## Reporting

Report a security problem privately, as [SECURITY.md](../SECURITY.md) says.
Anything else goes in an
[issue](https://github.com/mihaicristianfarcas/helio/issues/new/choose).
