# Development

How to build, test and check Helio across its three platforms, and the
tools and hooks that make that possible without three desks. The rules for
changes themselves — code style, comments, docs, commits — are in
[AGENTS.md](../AGENTS.md).

## The repository

```
Cargo.toml            the workspace (every crate below but spikes/ and tools/rawinput-probe)
helio-app/            the app's window (GPUI), This computer   ─┐
helio-client/         the client: session, stream, platforms   │ the app
helio-cli/            `helio`, the command line                │
helio-agent/          AI agents: MCP server, runners           ─┘
helio-host/           the host: sessions, pipelines, web UI    ─┐ the host
helio-host-data/      its data folder, identity, clients       ─┘
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
tools/linux-dev                                  # cargo build -p helio-app -p helio-cli
tools/linux-dev cargo test -p helio-host
tools/linux-dev cargo test --workspace --exclude helio-host -j 1
tools/linux-dev tools/linux/loopback-test        # a Linux host streaming to a Linux client
tools/linux-dev tools/linux/virtual-display-test # GNOME's virtual display, headless
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
- `tools/linux/virtual-display-test` runs Helio Host in a headless mutter
  (its own session bus and PipeWire) and Helio on an X screen: the host
  makes a monitor of the client's size for the session, arranges it as the
  whole desktop and leaves mutter to put the desktop back; the logs, a
  picture and mutter's monitors after the session land in
  `target/linux-out/virtual-display`. KDE's virtual display needs KWin with
  OpenGL and a seat: a Plasma desktop, by hand, with `helio-host` installed
  by `tools/linux/install-apps` (KWin gives its virtual output only to a
  program its desktop entry names).
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
- `target\release\helio.exe host enable`, on the host, installs the built
  engine beside it as `HelioHost` (Windows asks for an administrator; the
  engine copies itself to `C:\Program Files\Helio Host` and runs the service
  from there); run it again after a build, and `helio host disable` removes
  it. A built Helio's switch does the same with the `helio-host.exe` beside
  it (`tools/build-helio-win.ps1` puts it in `target\Helio`).
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
| `HELIO_TEST_INPUT="wait 3000; key 1e; click; text Hé!; quit"` | Scripted input once a stream is up (the steps are listed in `helio-cli/src/script.rs`) |
| `HELIO_TEST_SNAPSHOT=PATH[@FRAMES]` | The Windows and Linux client save what is on screen as a PNG after that many frames |
| `HELIO_TEST_LOSS=N[:B]` | The client drops N% of incoming media datagrams, in bursts of B (after decryption) |
| `HELIO_TEST_WIRE_LOSS=N[:B]` | Any endpoint drops N% of all tunnel datagrams, in bursts of B (before decryption; handshakes included) |
| `HELIO_TEST_HANDSHAKE_LOSS=N` | Any endpoint drops N% of handshake datagrams (rekeys under loss) |
| `HELIO_TEST_BLACKOUT=S:D` | Any endpoint sends and receives nothing from S seconds after it opened, for D seconds |
| `HELIO_UI_DEMO=…` | Drive the window and save screenshots ([ui.md](ui.md#checking-it-without-clicking)) |
| `HELIO_HOST_PROGRAM=PATH` | The `helio-host` the hosting switch and `helio host` run (a test host), instead of the one beside the app |
| `HELIO_UI_DEMO_SCREEN=PATH` | The picture the sample agent sessions show |
| `HELIO_HOST_APP_URL`, `HELIO_HOST_APP_CERT`, `HELIO_HOST_APP_TOKEN` | Point This computer at another host's API |
| `HELIO_UPDATE_API=URL` | The update check, and **Install and Restart**, ask this instead of `https://api.github.com`, and take a release's files only from it (the paths are GitHub's: `/repos/OWNER/NAME/releases/latest`, `/OWNER/NAME/releases/download/vVERSION/FILE`); see [Checking an update](#checking-an-update) |
| `HELIO_COMMIT=SHA`, `HELIO_RELEASE=1` | At build time: the commit the update check compares with `main` (else the checkout's), and a release build (follows releases only, and installs them). `cargo run -p helio-update --example check [releases\|main]` asks GitHub as the apps would; `--example install -- FOLDER` installs the latest release into FOLDER as an update would (on a Mac, over an older release's app there; on Windows, the installer checked and copied there, not run), leaving the installed app and its host alone |
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
one archive for each system and architecture, and Windows' installer,
published as the website's downloads. Pushes that change only `docs/`, `Casks/` or
Markdown files build nothing. The website,
[helio.mihaicristian.dev](https://helio.mihaicristian.dev), is not on
`main`: it lives on the `landing-page` branch, which is rebased on `main`
when it needs to be and never merged into it.

| Script | Makes |
|---|---|
| `tools/package-macos` | `Helio-VERSION-macos-ARCH.zip`: Helio.app, with the host inside it as a helper app, `Contents/Helpers/Helio Host.app` (`tools/build-helio-app`, which signs the helper, then the app), and `helio`, the command line: `Contents/Resources/bin/helio`, a link to the app's program, the folder Settings > General names in `/etc/paths.d/helio` |
| `tools/package-windows.ps1` | `Helio-VERSION-windows-x86_64-setup.exe`, the installer (`tools/windows/helio.iss`, Inno Setup 6.6 or newer): per user, no administrator, Helio in the Start menu and Settings > Apps, whose **Uninstall** turns hosting off first and takes `bin` off the user's `PATH` if Settings > General put it there; Helio's own update runs it again, silently. And `Helio-VERSION-windows-x86_64.zip`, the same files, unpacked: Helio.exe beside FFmpeg's DLLs and license and the host's engine, `helio-host.exe`; `bin\helio.exe`, the `helio` command (helio-app's `helio-console`, which runs `Helio.exe cli`). `-Sign` signs them all (below) |
| `tools/windows/test-install.ps1 -Setup FILE` | Not a package: installs the installer silently, turns hosting on, installs again (as a person does, then as Helio's update does, `/HELIOUPDATE`, which leaves the host's engine alone), uninstalls, and checks each step and that nothing is left; `-Signed` also verifies every signature, and `-UnsignedEngine` that the engine refuses an unsigned and a changed copy. As an administrator, for a user without Helio. CI runs it on each pull request, the release workflow on each package |
| `tools/linux/package` | `Helio-VERSION-linux-ARCH.tar.gz`: a folder with `helio-app`, `helio-host`, the host's unit `helio-host.service`, the launcher entry, its icon and `install.sh` (`tools/linux/install-release`; its `--uninstall` also removes the `~/.local/bin/helio` link Settings > General makes) |
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
A build with `--features require-signature` (`tools/package-windows.ps1
-RequireSignature`, the release workflow's whenever it signs) and
`HELIO_HOST_WINDOWS_PUBLISHER` set to the signing certificate's whole subject,
as `(Get-AuthenticodeSignature helio-host.exe).SignerCertificate.Subject` prints
it (`CN=…, O=…, L=…, S=…, C=…`), refuses an unsigned, changed or
otherwise-signed engine, including one whose certificate shares only the
common name; any other build only prints a warning. The feature does not
build without the publisher; a debug build never requires the
signature.

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

`main` takes changes only through pull requests: work on a branch, push it,
and open one (`gh pr create`).

### Windows signing

The Windows programs are signed with a Certum open-source code-signing
certificate, whose key stays in Certum's cloud (SimplySign; every new
certificate's key must stay with its issuer, in a hardware module or the
issuer's cloud, and Microsoft's Artifact Signing issues to individuals only
in the US and Canada). [ssign](https://docs.rs/ssign), an unofficial client
for SimplySign's API pinned at 0.1.8, signs them on the runner:
`tools/package-windows.ps1 -Sign` signs every `.exe` and `.dll` it ships,
FFmpeg's included (Smart App Control may block an unsigned DLL), then
builds the zip and the installer from the signed files and has Inno
Setup sign the installer, its uninstaller and the uninstaller's copies of
itself (helio.iss's `SignTool=ssign`). Every signature is timestamped
(Certum's timestamp authority, ssign's default).

The release workflow builds on one runner and signs and packages on
another, where no build script of any dependency has run: the job
`windows-package`, in the environment `release`, on `main` only. It needs,
once:

1. **Settings → Environments → `release`**: deployment branches `main`
   only, and required reviewers if every signing run should wait for one.
2. Its secrets `CERTUM_EMAIL` (the SimplySign account) and `CERTUM_OTP`
   (the TOTP seed behind its authenticator: base32, or the whole
   `otpauth://` URI). The seed signs as you for as long as it is valid:
   keep it nowhere else, and issue a new one in SimplySign if it leaks.
3. The repository variable `HELIO_HOST_WINDOWS_PUBLISHER`: the
   certificate's subject, exactly. A signing run without it fails and
   prints the subject to copy; a run with it but without the secrets fails
   too, since its engine would refuse itself.

With neither the secrets nor the variable, the Windows release is unsigned
and its run says so. Signed, the run checks with `signtool verify /pa /v`
every signature and its timestamp, and that the engine refuses an
unsigned copy of itself and a signed copy changed after signing
(`tools/windows/test-install.ps1`). By hand, with a code from the
authenticator rather than the seed:

```powershell
$env:CERTUM_EMAIL = 'you@example.com'; $env:CERTUM_TOKEN = '123456'
powershell -ExecutionPolicy Bypass -File tools\package-windows.ps1 -Sign
```

To try the signature check without Certum, in a VM: a self-signed
certificate (`New-SelfSignedCertificate -Type CodeSigningCert -Subject 'CN=Helio
Test Signing' -CertStoreLocation Cert:\LocalMachine\My`), trusted in the
VM's `Root` and `TrustedPublisher` stores, a build with
`HELIO_HOST_WINDOWS_PUBLISHER='CN=Helio Test Signing'` and
`-RequireSignature`, and `SSIGN` naming a script that takes ssign's
command line (`-n NAME -u URL FILES`) and signs with `signtool sign /sm
/sha1 THUMBPRINT /fd sha256 /tr TIMESTAMP_URL /td sha256`. Then
`test-install.ps1 -Signed -UnsignedEngine` with the engine as built.

The same variable makes Helio's own update require it
(`helio-update/src/install/authenticode.rs`): a build given
`HELIO_HOST_WINDOWS_PUBLISHER` runs a newer release's installer only if it
carries a valid signature by that subject, so the first signed release
is the one from which every Windows update is checked for its publisher;
the releases before it check the SHA-256 alone.

### Checking an update

`cargo test -p helio-update --test fake_release` serves a fake latest
release from this computer through `HELIO_UPDATE_API`, with this system's
package, and has a 1.0.0 build download, check and put it in place in a
folder of the test's (`install::rehearse`); a changed package, one served
from another server and the release the build already is, are refused.
On a Mac, with a Developer ID identity in the keychain,
`HELIO_TEST_DEVELOPER_ID="Developer ID Application: …" cargo test -p
helio-update --test fake_release -- --ignored` also installs one signed by
the same team and refuses one signed ad hoc.

By hand, on each system, an installed release updates to a fake newer
one:

1. Build twice with `HELIO_RELEASE=1`, the workspace's version set to,
   say, 1.0.0 and then 1.0.1 (put it back after): `tools/build-helio-app`
   with `HELIO_SIGN_IDENTITY` a Developer ID (macOS; zip the second with
   `ditto -c -k --keepParent`), `tools/package-windows.ps1`,
   `tools/linux/package` (`HELIO_PROFILE=dev` is quicker).
2. Install the first as the downloads install it, turn hosting on, pair
   something.
3. Lay out the second as a release, and serve it:
   `tools/fake-release http://ADDRESS:8080 DIR 1.0.1 PACKAGE`, then
   `python3 -m http.server 8080 --bind ADDRESS --directory DIR`.
4. Start the installed Helio with `HELIO_UPDATE_API=http://ADDRESS:8080`
   and `HELIO_UI_DEMO=install` (as **Install and Restart**). It opens
   again as 1.0.1; the host, while hosting is on, restarts on the new
   engine; pairings and settings stay. To see a refusal, change the
   package in DIR after `fake-release` listed it, or serve one signed by
   someone else.

On Windows, the installer's signature is checked without Certum the way
the engine's is above: build with `HELIO_HOST_WINDOWS_PUBLISHER='CN=Helio
Test Signing'` and `-RequireSignature` (`-StageOnly`), sign every program
in the stage folder and, after Inno Setup, the installer with the test
certificate (`Set-AuthenticodeSignature -TimestampServer
http://timestamp.digicert.com`), in a VM that trusts it. A second
self-signed certificate, trusted as well, signs the installer that must be
refused for its publisher; an unsigned one and a signed one changed after
signing (a byte of its DOS stub) are refused too. With hosting on, Windows
asks when the update starts; answering in a VM from a Mac:
`osascript -e 'tell application "UTM" to input keystroke (virtual machine
"NAME") text "y" with modifiers {option}'` (Alt+Y; Alt+N for No).

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
