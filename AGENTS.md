# Working on Helio

This file is for everyone who changes this repository: people, and AI coding
agents (Claude Code, Codex and others read it as their instructions). It is
the project's rules for code, comments, documentation and commits. The
how-to — building, testing, the tools — is in
[docs/development.md](docs/development.md); the map of the code is in
[docs/architecture.md](docs/architecture.md). Read both before a larger
change.

## What the project is

Helio streams a computer's desktop to another the way Moonlight and
Sunshine do, inside one post-quantum WireGuard tunnel. **Helio** is
the app (`helio-app`, `helio-client`): the client for everyone, and hosting as a
switch in it, off by default, which runs **Helio Host**, the host (`helio-host`). Both
run on macOS, Windows and Linux, over shared libraries (`helio-*`).
`helio-agent` lets AI agents use hosts as clients of their own.

## Principles

- **Latency first.** This is a real-time system: a frame is due every 8 ms at
  120 fps. On the data path (capture → encode → send; receive → reassemble →
  decode → present) do not allocate per datagram, take locks another thread
  holds for long, block on I/O, or log per packet. Reuse buffers; keep work
  off the network thread.
- **Moonlight and Sunshine/Apollo are the reference.** When in doubt about
  behaviour — loss recovery, encoder settings, pacing, input, shortcuts,
  settings — do what they do, and say so in a comment with the source file
  (`VideoDepacketizer.c`, `stream.cpp`). Diverge only for a measured reason,
  and write the reason down. Apollo is a fork of Sunshine: for what it adds
  (the virtual display and SudoVDA, client permissions, clipboard sync),
  read Apollo; for everything else, Sunshine's current source, which is the
  one that is maintained.
- **Measure, then change.** Claims about performance or platform behaviour
  come with numbers and the conditions they were taken under (in the
  comment, the commit and, for user-visible results,
  [docs/benchmarks.md](docs/benchmarks.md)).
- **Every platform stays green.** Shared code must build and pass clippy on
  macOS, Windows and Linux. You can check all three from one machine (see
  [Before you finish](#before-you-finish)).
- **Hostile input never panics.** Anything read from the network is
  untrusted: parse with bounds checks, return `None`/`Err`, and keep the
  fuzz targets (`helio-proto/fuzz`) covering new parsers.
- **Secrets stay secret.** Never log a PIN, password, key or token. Files
  holding secrets are readable only by their owner (0600, or an ACL on
  Windows). Never commit keys, `.env` or `tools/dev.env`.
- **A refactor changes no behaviour**, including user-visible strings, file
  formats and the wire format. Keep refactors in commits of their own.
- **No copied code.** Helio is licensed under the GPL-3.0, as Moonlight,
  Sunshine and Apollo are, and is an implementation of its own: read them
  to learn what they do, then write your own; never paste or translate
  their code. Never take code from a source whose license is not
  compatible with the GPL-3.0. Vendored code keeps its license and notice
  beside it (`helio-encode/src/nvenc_sys/LICENSE`).

## Code

### Style

- Rust 2021, MSRV 1.96 (`rust-version` in the workspace `Cargo.toml`).
- `cargo fmt` with the default configuration. Where rustfmt gives up on a
  long expression (usually a UI builder chain holding a long string), split
  the string with a `\` continuation (the value is unchanged) so it can
  format the rest.
- `cargo clippy --workspace --all-targets` without warnings, on all three
  platforms. Silence a lint only with a targeted `#[allow(...)]` and a
  comment saying why.
- Match the surrounding code: its naming, comment density and idiom. Names
  say what a thing is in the domain (`FrameGate`, `LossCounter`,
  `Recipients`), not how it is implemented.
- Keep files and functions a size a reader can hold: split a file by concern
  when it grows past roughly 800 lines (see `helio-client/src/stream/`,
  `helio-app/src/host/`), and a function into named steps when it passes
  roughly 150 (UI element trees excepted).
- Constants get names and a doc comment on why that value, with the
  measurement where there is one.

### Structure

- **Portable logic is pure.** Protocol and policy code lives where it can be
  tested on any machine: `helio-proto` does no I/O at all; the input and
  display crates keep their bookkeeping (held keys, coordinate transforms)
  free of platform calls.
- **Platform code sits behind a seam of the same shape everywhere**: a trait
  (`InputSink`, `DisplayControl`, `VideoOut`) or modules compiled under one
  name per platform (`helio-host/src/platform.rs`, `helio-host/src/mac/platform.rs`,
  `helio-host/src/linux/platform.rs` are all `platform`; `helio_client::mac`, `win`,
  `linux`). Add to all of them, or say in the code why a platform has no
  equivalent.
- **Threads are named** (`std::thread::Builder::name`); the latency-critical
  ones call `priority::latency_critical()`. Hand data between threads over
  channels (crossbeam); bound them where backpressure matters, and say what
  happens when one is full.
- **Errors**: libraries return typed errors (`CaptureError`,
  `EncodeError`) or `Result<_, String>` with a message a person can act on;
  user-facing text says what happened and what to do. `unwrap`/`expect` only
  on invariants, with the invariant in the message or a comment.
- **`unsafe`**: every new or changed `unsafe` block gets a `// SAFETY:`
  comment saying why it is sound. Many existing FFI calls (Win32,
  CoreGraphics) have none; add one when you touch a block whose soundness is
  not obvious.
- **Dependencies**: prefer pure Rust and crates already in the tree. Git
  dependencies are pinned by `rev`. Keep the `windows` crate on one version
  across the workspace (two versions mean two incompatible `HANDLE` types).
- **Logging**: `tracing`, with structured fields (`tracing::info!(fps, mbps,
  "stream")`). `info` for lifecycle (a session started, a setting changed),
  `debug` for per-second statistics, `trace` for per-event detail.

### The UI

- The window uses GPUI through `helio-ui`, in asides' look: its controls,
  theme tokens (`Theme`, `Ink`, `Type`, `Metrics`, `Radius`) and motion
  (`FADE`, `RESIZE`, `ease`, `Glide`), never raw colours, sizes or timings.
  No GPUI shadows on glass, and nothing of this on the stream's window.
- One module per page or panel; the page's state stays in the app struct.
- A new page or state gets a `HELIO_UI_DEMO` step, so it can be
  screenshotted without clicking ([docs/ui.md](docs/ui.md)).
- Every row in a settings page says in a line what it does.

### Tests

- Unit tests sit beside the code (`#[cfg(test)] mod tests`), named as
  sentences that state the behaviour:
  `fn a_frame_straddling_two_reports_is_not_loss()`.
- Test the logic, not the platform: the pure parts are where bugs hide, and
  they run everywhere.
- Tests that need hardware or a desktop are `#[ignore = "requires ..."]` and
  say at the top of the file how to run them.
- End-to-end: `tools/linux/loopback-test` streams a Linux host to a Linux
  client with scripted input, in a container; the test hooks in
  [docs/development.md](docs/development.md#test-hooks) script input and
  impair the link on real machines.

## Comments

- **Every file starts with a `//!` module doc** (a script: a comment block
  at its top): what the module is, what it is for, and the non-obvious
  decisions (which platform API and why, what Moonlight or Sunshine do). A
  new file has one before it is committed; a changed decision changes it.
- **Say why, not what.** The code says what. A comment earns its place by
  explaining a reason, a constraint, a measurement or a trap ("looks
  removable, is not: …"). Delete comments that restate the code.
- **Measurements are facts with conditions**: the value, its unit, and what
  it was taken on and under. "3.9 s with the host's monitor asleep", not
  "slow".
- **References**: Moonlight and Sunshine (and Apollo, for what is its own)
  by source file; the design documents as `v1 design §5.1` or
  `v2 design §6.4` ([docs/design/](docs/design/README.md)); other docs by
  path.
- **No history in comments**: no dates, people's or machines' names, "we
  tried", "used to", ticket or plan numbers. Git holds the history; a
  comment describes the code as it is. ("An earlier version did X, which
  caused Y" is fine when it stops someone from reintroducing X.)
- **Keep comments true.** Change them with the code they describe: search
  the comments and docs for every name you rename or remove. A comment that
  no longer holds is a bug in the change that made it false.

## Documentation

Documentation is part of the change, not a follow-up: a pull request is not
finished until the pages a reader would open say what the code does. These
rules hold for `README.md`, every page below and the issue forms in
`.github/ISSUE_TEMPLATE/`.

`docs/` is organised by what the reader wants to do:

| File | For |
|---|---|
| `README.md` | What Helio is, what it can do, where to start, its known limitations |
| `docs/install.md`, `docs/usage.md`, `docs/cli.md`, `docs/logs.md` | People installing and using it, and reporting what goes wrong |
| `docs/architecture.md`, `docs/networking.md`, `docs/platforms/*.md`, `docs/ai-agents.md`, `docs/ui.md` | How it works, and each platform's behaviour and limits |
| `docs/benchmarks.md`, `docs/parity.md`, `docs/verification.md` | What it achieves, measured, against Moonlight + Sunshine/Apollo; how it is built and verified |
| `docs/development.md`, `AGENTS.md`, `CONTRIBUTING.md` | People changing it |
| `docs/design/` | The design history. Frozen: fix links and typos only |

A new page gets a row here and a line in [docs/README.md](docs/README.md).

### What goes where

**Documentation changes with the code, in the same commit.** Every fact has
one home, the page below. Other pages link to it and do not repeat it: a
second copy is the one that goes stale.

| When you change | Update |
|---|---|
| A setting, on either side | The tables in [docs/usage.md](docs/usage.md): what it does in a line, its default, its `config.toml` name |
| A flag or command | [docs/cli.md](docs/cli.md) and the command's own usage text |
| An environment variable | `docs/cli.md` (for users) or `docs/development.md` (for tests) |
| Where a program logs, or what a level shows | [docs/logs.md](docs/logs.md) |
| What a platform does or cannot do | Its page in `docs/platforms/`: under its limits, "Not verified yet" (built, not run) or "Not built yet" |
| A limit someone meets on their first try | Also "Known limitations" in [README.md](README.md) |
| A crate, a thread, the data path, the wire format | [docs/architecture.md](docs/architecture.md) |
| Ports, discovery, the internet path | [docs/networking.md](docs/networking.md) |
| Anything Moonlight, Sunshine or Apollo also do | Its row in [docs/parity.md](docs/parity.md), with its status word |
| A user-visible result | [docs/benchmarks.md](docs/benchmarks.md), with its conditions |
| Installing, packaging, downloads, signing | `docs/install.md`, and `docs/development.md` for how it is made |
| What is kept, exposed, installed or run with privileges | [SECURITY.md](SECURITY.md) |
| A CI job, a kind of test, a fuzz target, how changes are reviewed | [docs/verification.md](docs/verification.md) |
| A page or state of the window | [docs/ui.md](docs/ui.md), and a `HELIO_UI_DEMO` step |

### How it is written

- **Write plainly**: present tense, short sentences, the reader's words
  ("the host", "the client", "your computer"). Address users as "you".
  Tables for reference, numbered steps for procedures, code blocks for
  commands. A command block can be pasted as it stands: no prompt signs, and
  what the reader replaces is in capitals (`HOST`) and explained in the text.
- **One name for each thing.** **Helio** is the app, **Helio Host** the
  host, `helio-host` its engine, `helio` the command line, and hosting the
  switch that runs it. Words from the window are spelled as the window
  spells them, in bold, with arrows for a path: **Settings → Hosting**.
- **Be exact and checkable**: real flag names, real paths, real defaults,
  each read from the code rather than remembered. Every claim can be checked
  in this repository: in the code, in a test, or in a measurement. A
  comparison or a superlative ("matches Moonlight", "fast", "best") comes
  with its figures, their conditions and a link to them, or it goes.
- **Numbers come with how and on what they were measured**: the machine,
  the system, the settings, the build, the date. A number that ages (a test
  count) also gives the commit it was taken at and the command that repeats
  it.
- **Say how far it was checked**: by CI, on real hardware, in a virtual
  machine, in a container, or a type-check only. Nothing implies more than
  that. What is built and has not run goes under "Not verified yet".
- **Describe the release `main` is becoming.** A page reads as the reader of
  that release finds it. No "soon", "planned", "for now", "recently" or
  "it used to"; no TODO, TBD, stub, empty table cell or link to something
  that does not exist. What is not built goes under "Not built yet", and git
  and the release notes hold the history. Text written ahead of a release
  (a certificate, a package, a website) merges only with what it describes:
  its pull request lists what has to exist first.
- **Never invent what only a measurement or the maintainer can give**: a
  figure, a result, how the project is run. Measure it, or ask. A section
  with no result yet gives its method, and no figure.
- **No private details**: machine names, addresses, user names or paths
  from someone's setup, in text, in pasted logs or in pictures. Use
  `gaming-pc`, `192.168.1.20`, `203.0.113.7` (documentation addresses) and
  `C:\path\to\...`.
- **Links are relative**, so they work on GitHub and in a checkout. The
  issue forms are the exception: GitHub does not resolve relative links
  there. Every link resolves: the file exists, and an `#anchor` is the one
  GitHub derives from the heading. Renaming a heading means searching for
  its anchor.
- **Pictures** are in `docs/images/`, show sample data, and have alt text
  that says what they show.
- **The issue forms** pass GitHub's schemas
  (`check-jsonschema --builtin-schema vendor.github-issue-forms` for a form,
  `vendor.github-issue-config` for `config.yml`), and ask for what
  [docs/logs.md](docs/logs.md) tells people to send.
- **`docs/design/` is frozen**: the history of how it was designed. Fix links
  and typos only. A design that changed is described in the living pages.

## Commits and pull requests

- `main` takes changes only through pull requests, and a pull request
  merges once CI (`.github/workflows/ci.yml`: formatting, clippy and the
  tests on macOS, Windows and Linux) passes. Work on a branch.
- A review agent checks each change, and the maintainer reads it, before it
  merges ([docs/verification.md](docs/verification.md#how-changes-are-reviewed)).
- One logical change per commit. Keep formatting, refactors and behaviour
  changes in separate commits. Documentation goes in the commit of the code
  it describes; a `docs:` commit changes documentation only.
- The summary line names the area and says what changed, in plain words:
  `Helio Host: input from whoever does not drive is seen and dropped`,
  `helio-agent: ANTHROPIC_BASE_URL and OPENAI_BASE_URL`, `docs: …`. The body
  says why, and how it was verified (which platforms, which tests, what was
  measured).
- A pull request says the same: what, why, how it was checked, and what was
  not (a platform you could not run, hardware you do not have).

## Before you finish

```sh
cargo fmt --all
cargo clippy --workspace --all-targets
cargo test --workspace
tools/xcheck-windows clippy --workspace --all-targets     # from macOS or Linux
tools/linux-dev cargo clippy --workspace --exclude helio-host --all-targets
tools/linux-dev cargo clippy -p helio-host --all-targets
```

Plus, as the change needs: the UI demo screenshots before and after, the
Linux loopback test, a real stream with the statistics on.

For anything that touches documentation, or should have:

```sh
git grep -nE 'TODO|TBD|FIXME|coming soon' -- '*.md' .github   # nothing new
```

- Every line in the diff was checked against the code, a test or a
  measurement, and each page in [What goes where](#what-goes-where) that the
  change touches is updated.
- Every link you added or changed resolves, anchors included.
- A procedure you wrote ran once as written, on that system, or the pull
  request says it did not.
- The names you changed are gone from the comments and the docs.

The hooks in `.githooks/` check the cheap part as you go: nothing private
and the formatting on commit, the summary line's shape, clippy on push.
Enable them once per clone:

```sh
git config core.hooksPath .githooks
```

CI runs the formatting, clippy and the tests on every pull request, natively
on each system, and packages the app; there they cannot be skipped. The
screenshots, the Linux loopback test, the fuzz targets and a real stream
are yours to run ([docs/verification.md](docs/verification.md#not-in-ci)).

## For AI agents

- Read the module docs of the files you change, and follow the code's own
  style over any habit of yours.
- Do not stream to, or deploy on, someone's real host unless asked: a
  session switches off the host's monitors and takes its keyboard and mouse.
  Use the Linux container, a loopback test, and `HELIO_DATA_DIR` /
  `HELIO_HOST_DATA_DIR` pointing at temporary folders so your checks neither read
  nor change the user's own pairings and settings.
- Do not edit `docs/design/` beyond links and typos.
- Do not add private details (machine names, addresses, paths from the
  environment you run in) to code, tests or docs.
- Never write a number, a test result or a statement about how the project
  is run that you did not take from the repository or the maintainer. Count
  it, measure it, or ask.
- Before writing what a release, download, package or website does, look at
  the thing itself (`gh release view`, the URL), not at the workflow meant
  to make it. If it does not exist yet, say so in the pull request.
- Say what you verified and what you could not. "Builds on Windows" from
  `tools/xcheck-windows` is a type-check, not a run.
