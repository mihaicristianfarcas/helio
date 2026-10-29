# Development

How to build, test and check Helio across its three platforms, and the
tools and hooks that make that possible without three desks. The rules for
changes themselves — code style, comments, docs, commits — are in
[AGENTS.md](../AGENTS.md).

## The repository

```
Cargo.toml            the workspace (every crate below but spikes/ and tools/rawinput-probe)
helio-app/            the app's window (GPUI), This computer   ─┐
helio-client/         the client: stream, platforms, `helio`   │ the app
helio-agent/          AI agents: MCP server, runners           ─┘
helio-host/           the host: sessions, pipelines, web UI      the host
helio-*/              the libraries both share (see docs/architecture.md)
Casks/                the Homebrew cask, written at each release (see below)
docs/                 documentation; docs/design/ is the design history
tools/                build, deploy and test scripts (below)
spikes/               early experiments, not built with the workspace
vendor/               two no-op crates GPUI names
```

Most crates have an `examples/` directory of probes — small programs that
exercise one platform API and print what it does (a virtual display's mode,
what the NAT does, the pointer the host sees). Each says at its top what it
shows and how to run it.

## Building

Rust 1.96 or newer, and the platform packages in
[install.md](install.md#building-from-source).

```sh
cargo build                         # everything for this platform (debug)
cargo build --release -p helio-app  # one crate
```

- **Linux: build `helio-host` on its own**, not in one `cargo` invocation with the
  GPUI apps (a dependency is asked for two async runtimes; one build cannot
  have both). `cargo build --workspace --exclude helio-host` and
  `cargo build -p helio-host` are fine.
- **GPUI's Metal shaders** (macOS) compile at startup by default (the
  `runtime-shaders` feature), so no Xcode is needed; `--no-default-features`
  compiles them ahead of time with Xcode's Metal toolchain, as
  `tools/build-helio-app` does when Xcode is there.
- **Helio on Windows** needs FFmpeg and LLVM: use `tools/build-helio-win.ps1`
  (see [install.md](install.md#helio-on-windows)).

## Checking a change

Before sending a change, on your platform:

```sh
cargo fmt --all
cargo clippy --workspace --all-targets     # no warnings
cargo test --workspace
```

and on the platforms you cannot run, at least type-check:

```sh
tools/xcheck-windows                              # cargo check for Windows, from a Mac
tools/xcheck-windows clippy --workspace --all-targets
tools/linux-dev cargo clippy --workspace --exclude helio-host --all-targets
tools/linux-dev cargo clippy -p helio-host --all-targets
```

`tools/xcheck-windows` uses a private rustup under
`~/.local/share/helio-xcheck` (it never touches your own toolchain) and
MinGW; the script's header has the one-time setup. The Windows client's
FFmpeg bindings need an FFmpeg 8 build's headers copied to
`~/.local/share/helio-xcheck/ffmpeg`. `tools/xcheck-linux` does the same
for crates that need no system libraries; everything else is checked in the
Linux container.

Tests that need hardware are `#[ignore]`d and say how to run them (for
example `helio-display/tests/windows_display.rs`, which must run in the
host's interactive desktop session).

### Linux

`tools/linux-dev CMD` runs a command in an Ubuntu 24.04 container with the
build dependencies (`tools/linux/Dockerfile`; on a Mac, Docker through
colima). The tree is mounted at `/src`; the build, the registry and git
checkouts live in named volumes, so rebuilds are incremental.

```sh
tools/linux-dev                                  # cargo build -p helio-app -p helio-client
tools/linux-dev cargo test -p helio-host
tools/linux-dev cargo test --workspace --exclude helio-host -j 1
tools/linux-dev tools/linux/loopback-test        # a Linux host streaming to a Linux client
```

- Link the GPUI apps one at a time (`-j 1` for tests): two at once can run
  the container out of memory.
- `tools/linux/loopback-test` runs Helio Host on one virtual X screen (a scene of
  moving boxes that logs the input it receives) and Helio on another, pairs
  them by hand, streams with scripted input and a tone, and leaves the logs,
  a picture and the input log in `target/linux-out/loopback`. With
  `HELIO_DOCKER_ARGS="-e LOOPBACK_PERMISSIONS=view,mouse"` the client is
  paired with those permissions, and the input log shows what the host
  let through (here the click and the wheel, no keys).
- `tools/linux/gnome-desktop` runs a GNOME desktop (mutter nested in an X
  screen, the portal, PipeWire) to test the Wayland host by hand;
  `helio-capture`'s `x11-poke` example answers its dialogs.
- `tools/linux/agent-desktop` is a small desktop (openbox, a terminal, an
  editor, a calculator, a file manager) with a Linux Helio Host, for AI agents to
  work on; `tools/linux/agent-tests/` has scripted clients for it.

### A Windows host from a Mac

- `tools/dev-sync [build ARGS]` mirrors the working tree to a Windows machine
  over SSH (tracked and untracked files, not ignored ones), and optionally
  runs `cargo build --release ARGS` there. Set `HELIO_SSH=user@host` and
  `HELIO_HOST_DIR` in the environment or in `tools/dev.env` (not
  committed).
- `target\release\helio-host.exe enable`, on the host in an administrator
  terminal, installs the built engine as `HelioHost` (it copies itself to
  `C:\Program Files\Helio Host` and runs the service from there); run it again
  after a build, and `helio-host disable` removes it. A built Helio's switch does
  the same with the `helio-host.exe` beside it (`tools/build-helio-win.ps1` puts
  it in `target\Helio`).
- `tools/host-run.ps1 -Command '…'` runs a command in the interactive
  console session and prints its output: SSH lands in session 0, where there
  is no desktop to capture and no display to configure.
  `spikes/run-interactive.ps1` does the same for longer runs.
- `tools/set-resolution.ps1` sets the primary display's mode (for
  measurements on a host without a monitor); `tools/rawinput-probe` reports
  what raw input — the API games read — actually received.

### macOS signing

macOS ties the permissions it grants (Screen Recording, Accessibility, local
network) to an app's signature, and an ad hoc signature changes with every
build. `tools/dev-signing-identity` creates one self-signed code-signing
identity ("helio dev") in your login keychain; `tools/build-helio-app`
signs the app and the host's helper inside it with it when it exists, so
the permissions survive rebuilds. The first signing asks to use the key: **Always Allow**.
`HELIO_SIGN_IDENTITY` names another identity to sign with, as a release
is ([Releases](#releases)).

## Test hooks

Environment variables that let a stream, or a window, be driven and impaired
without a person or a network tool. They are for tests; none is needed in
normal use.

| Variable | Does |
|---|---|
| `HELIO_TEST_INPUT="wait 3000; key 1e; click; text Hé!; quit"` | Scripted input once a stream is up (the steps are listed in `helio-client/src/bin/helio/script.rs`) |
| `HELIO_TEST_SNAPSHOT=PATH[@FRAMES]` | The Windows and Linux client save what is on screen as a PNG after that many frames |
| `HELIO_TEST_LOSS=N[:B]` | The client drops N% of incoming media datagrams, in bursts of B (after decryption) |
| `HELIO_TEST_WIRE_LOSS=N[:B]` | Any endpoint drops N% of all tunnel datagrams, in bursts of B (before decryption; handshakes included) |
| `HELIO_TEST_HANDSHAKE_LOSS=N` | Any endpoint drops N% of handshake datagrams (rekeys under loss) |
| `HELIO_TEST_BLACKOUT=S:D` | Any endpoint sends and receives nothing from S seconds after it opened, for D seconds |
| `HELIO_UI_DEMO=…` | Drive the window and save screenshots ([ui.md](ui.md#checking-it-without-clicking)) |
| `HELIO_HOST_PROGRAM=PATH` | The `helio-host` the hosting switch runs (a test host), instead of the one beside the app |
| `HELIO_UI_DEMO_SCREEN=PATH` | The picture the sample agent sessions show |
| `HELIO_HOST_APP_URL`, `HELIO_HOST_APP_CERT`, `HELIO_HOST_APP_TOKEN` | Point This computer at another host's API |
| `HELIO_UPDATE_API=URL` | The update check asks this instead of `https://api.github.com` (the paths are GitHub's) |
| `HELIO_COMMIT=SHA`, `HELIO_RELEASE=1` | At build time: the commit the update check compares with `main` (else the checkout's), and a release build (follows releases only). `cargo run -p helio-update --example check [releases\|main]` asks GitHub as the apps would; `--example install -- FOLDER` installs the latest release into FOLDER as an update would (on a Mac, over an older release's app there), leaving the installed app and its host alone |
| `HELIO_HOST_NO_PROMPTS=1` | A bundled Mac host does not ask for permissions at start |
| `HELIO_HOST_TEST_TAP_ONLY=PID[,PID]` | A Mac host's surround capture hears only these processes (a test signal's player): a client streaming the Mac to itself would otherwise send its own playback round again |
| `HELIO_AGENT_MCP="CMD … mcp"` | Agent runs start this MCP server instead of their own (e.g. one inside the Linux container) |
| `HELIO_AGENT_PATH_MAP=LOCAL=REMOTE` | Rewrites paths passed to such a server |
| `HELIO_AGENT_FREE_ONLY=1` | OpenRouter runs use free models only |
| `HELIO_PORTMAP_TEST=1` | Run the port-mapping test against this network's real router |

Per-second statistics: `RUST_LOG=info,helio_client::stats=debug` on the client
(`lost_frames` and `recoveries` in that line are cumulative), and the host's
`helio-host.log`.

## Releases

Every push to `main` that changes the programs is built and published:
`.github/workflows/release.yml` builds the app, with the host's engine
inside it, on an Apple silicon and an Intel Mac, on Windows and on Linux:
one archive for each system and architecture, published as the website's
downloads. Pushes that change only `docs/`, `Casks/` or
Markdown files build nothing. The website,
[helio.mihaicristian.dev](https://helio.mihaicristian.dev), is not on
`main`: it lives on the `landing-page` branch, which is rebased on `main`
when it needs to be and never merged into it.

| Script | Makes |
|---|---|
| `tools/package-macos` | `Helio-VERSION-macos-ARCH.zip`: Helio.app, with the host inside it as a helper app, `Contents/Helpers/Helio Host.app` (`tools/build-helio-app`, which signs the helper, then the app) |
| `tools/package-windows.ps1` | `Helio-VERSION-windows-x86_64.zip`: Helio.exe beside FFmpeg's DLLs and the host's engine, `helio-host.exe`; `install.ps1`, which is `tools/install-helio.ps1`; and `Install Helio.cmd`. `install.ps1` copies itself beside the programs and lists Helio in Settings > Apps, whose **Uninstall** runs it with `-Uninstall` (hosting off first) |
| `tools/linux/package` | `Helio-VERSION-linux-ARCH.tar.gz`: a folder with `helio-app`, `helio-host`, the host's unit `helio-host.service`, the launcher entry, its icon and `install.sh` (`tools/linux/install-release`) |
| `tools/publish-downloads DIST` | The downloads: each archive at `https://download.helio.mihaicristian.dev/latest/Helio-OS-ARCH.EXT`, then `latest/SHA256SUMS` and `latest.json` (version, commit, sizes, checksums), in the Cloudflare R2 bucket `helio-downloads`, with wrangler; last, the archives the previous `latest.json` listed and this one does not are deleted |

All of them build with `HELIO_RELEASE=1`, so the programs follow
releases, not `main`. With `HELIO_PROFILE=dev` the three packaging
scripts build the debug profile instead, which is how CI checks on every
pull request that the app still packages, the host inside it. The downloads keep their names from one build to the
next, so the website links them as they are and reads `latest.json` for
what they are. The workflow signs in to Cloudflare with the repository's
secret `CLOUDFLARE_API_TOKEN` (an API token with **Workers R2 Storage:
Edit**) and variable `CLOUDFLARE_ACCOUNT_ID`; by hand, `wrangler login` is
enough.

On Windows, `helio-host enable` checks the Authenticode signature of the engine
it copies to `C:\Program Files\Helio Host` before HelioHost runs it as SYSTEM.
Windows releases are not signed yet, so a failed check only prints a
warning. Once they are, the release build turns the check on:
`cargo build --release -p helio-host --features require-signature` with
`HELIO_HOST_WINDOWS_PUBLISHER` set to the signing certificate's whole subject,
as `(Get-AuthenticodeSignature helio-host.exe).SignerCertificate.Subject` prints
it (`CN=…, O=…, L=…, S=…, C=…`), and `helio-host enable` then refuses an unsigned,
changed or otherwise-signed engine, including one whose certificate shares
only the common name. The feature does not build without the publisher; a
debug build never requires the signature.

An MSVC build of `helio-host.exe` is linked with `/DEPENDENTLOADFLAG:0x800`
(`helio-host/build.rs`): the DLLs it imports load from System32 only, never from
beside it, which matters when `helio-host enable` runs elevated from a folder its
user can write to. A MinGW cross-build (`tools/xcheck-windows`) cannot set
this; use one for checks, not for an install.

A **version** is released by merging the change that sets it: the
workspace's version in `Cargo.toml` (`[workspace.package]`), which every
program has. When a push to `main` has a version with no `vVERSION` tag
yet, the same archives become the GitHub release `vVERSION`, and the
workflow opens a pull request that brings the Homebrew cask,
`Casks/helio.rb`, to it (`tools/update-casks`); `brew upgrade` sees the release once that is
merged. GitHub holds CI on a pull request a workflow opened until it is
approved: the workflow approves it, or says on the pull request that it
needs approving by hand.

To build the archives of a branch without publishing them, run the
workflow on it (`gh workflow run release.yml --ref BRANCH`): they are the
run's artifacts. To try the cask before a release, write it against the
local archives
(`CASKS_DIR=DIR tools/update-casks VERSION target/dist file://$PWD/target/dist`),
put it in a local tap (`brew tap-new`), and `brew install --cask
--appdir=DIR` from it.

On the runners the macOS app is signed with a Developer ID, the helper
inside it first, with the hardened runtime and a secure timestamp,
notarized by Apple in one submission and stapled, so macOS opens it from a
download. The workflow takes the certificate
and an App Store Connect API key from five secrets
(`MACOS_CERTIFICATE_P12`, the certificate and its key in base64;
`MACOS_CERTIFICATE_PASSWORD`; `APPLE_API_KEY_P8`, `APPLE_API_KEY_ID`,
`APPLE_API_ISSUER`), and a release on `main` fails without them. By hand,
with the Developer ID in your keychain:

```sh
HELIO_SIGN_IDENTITY="Developer ID Application: NAME (TEAMID)" \
APPLE_API_KEY_PATH=path/to/AuthKey_KEYID.p8 APPLE_API_KEY_ID=KEYID APPLE_API_ISSUER=ISSUER \
tools/package-macos
```

Without the API key the app is signed but not notarized; without
`HELIO_SIGN_IDENTITY` it is signed as a development build is (below).

The Windows programs are not signed. Signing them takes a code-signing
certificate whose key stays with its issuer (every new one's must, in a
hardware module or the issuer's cloud): for a person rather than a
company, Certum's open-source certificate in its cloud is one, while
Microsoft's Artifact Signing issues to individuals only in the US and
Canada. The signing goes in `tools/package-windows.ps1`, on Helio.exe,
and helio-host.exe before they are zipped, with the issuer's
credentials as repository secrets, as the Developer ID's are.

`main` takes changes only through pull requests: work on a branch, push it,
and open one (`gh pr create`).

## Benchmarks and fuzzing

`cargo bench -p helio-proto`, `-p helio-transport` and
`-p helio-decode` run the Criterion benchmarks; the results and what the
end-to-end numbers are live in [benchmarks.md](benchmarks.md).

The protocol parsers that read network input are fuzzed (cargo-fuzz,
nightly Rust):

```sh
cd helio-proto/fuzz
cargo +nightly fuzz run depacketize   # also: control, input
```

## Assets

- The app's icon has one source, `helio-app/assets/icon.svg`: a lowercase h
  on Apple's icon grid. `cargo run -p helio-app --example make-icon --
  OUTDIR` draws every other icon from it: the macOS iconset (`AppIcon`, for
  the app and Helio Host's helper alike), the Windows `.ico`, the Linux
  icon theme's sizes (`hicolor/`), the host's menu bar mark (`tray.png`,
  `tray@2x.png`), the h alone for the welcome page (`mark.svg`, tinted with
  the text's colour) and the site's `helio.svg` (favicon and logo, for the
  `landing-page` branch). Everything but the iconset and the site's file is
  committed in `helio-app/assets/`; after changing the source, refresh it
  with `cp -R OUTDIR/assets/. helio-app/assets/`. `tools/build-helio-app`
  draws the iconset again when the source or the example changes.
- The UI's icons are SVG files in `helio-ui/assets/icons/`, 24×24 with
  1.75 pt round strokes in `currentColor`, embedded in the binary.
- `helio-decode/tests/fixtures/testsrc.h264` is the known stream the
  decoder tests use (regenerate it with the FFmpeg command in
  `spikes/vt-latency/README.md`).
