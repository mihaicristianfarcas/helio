# Command line

`helio` is Helio's command line: streaming and pairing from a terminal,
and this computer as a host with `helio host`. It is the one command you
type. `helio --help` lists every command, and `helio host --help` the
host's. AI agents have a command of their own, [`helio-agent`](#helio-agent--ai-agents).

`helio` comes with Helio, inside the app: the app's own program is the
command when it is started as `helio`. Installing Helio does not put it on
your `PATH`. To type it in a terminal, open Helio's **Settings → General**
and, under **Command line**, choose **Install**; **Remove** takes it off
again. Terminals opened from then on have it.

| System | Install does | Uninstalling Helio |
|---|---|---|
| macOS | Writes `/etc/paths.d/helio`, naming the app's own folder for the command (`Helio.app/Contents/Resources/bin`), which every login shell adds to its `PATH`; macOS asks for an administrator | `helio` goes with the app, however you remove it. The one-line `/etc/paths.d/helio` stays, naming a folder that is gone, until you install it again or remove it (`sudo rm /etc/paths.d/helio`, or `brew uninstall --zap --cask helio`) |
| Windows | Adds Helio's `bin` folder (`%LOCALAPPDATA%\Programs\Helio\bin`, with `helio.exe` in it) to your user `PATH`; no administrator | Settings > Apps > Uninstall takes it off your `PATH`, and the folder goes with Helio |
| Linux | Links `~/.local/bin/helio` to `helio-app`; most distributions have `~/.local/bin` on the `PATH` | `install.sh --uninstall` removes the link |

Install is there only for a copy of Helio that stays where it is: on a Mac,
one in Applications (not on the disk image, nor run from Downloads), and a
packaged release (a build of your own from `cargo build` has no command
beside it; `tools/build-helio-app` and `tools/build-helio-win.ps1` make
one that does). A `~/.local/bin/helio` that is not Helio's is left as it is.

The app's program also takes the command after `cli`, wherever it is:
`/Applications/Helio.app/Contents/MacOS/Helio cli hosts` is `helio hosts`
(`helio-app cli` on Linux, `Helio.exe cli` on Windows, though there only
`bin\helio.exe` shows what it prints in the terminal). From a checkout,
`cargo build --release -p helio-cli` builds `helio` on its own
(`target/release/helio`), and `-p helio-agent` builds `helio-agent`.

## `helio`

`helio stream` is the app's streaming code, in a window of the platform's
own.

In a terminal, `helio` is in colour (headings, what to type, what worked
and what failed) and its lists (`hosts`, `discover`, `host clients`) are
in columns under a header. Piped or redirected, it prints plain text, each
list a line of tab-separated cells per row, for scripts. `NO_COLOR` turns
the colour off, `CLICOLOR_FORCE=1` on (through a pager, say).

### Streaming

| Command | Does |
|---|---|
| `helio stream NAME [FLAGS]` | Stream a paired host |
| `helio watch NAME [FLAGS]` | Watch the AI agent working on a host (no sound; Ctrl+Alt+Shift+T takes over) |
| `helio wake NAME` | Send Wake-on-LAN packets to a paired host |

Stream flags. The defaults come from this display, fitted to the nearest
standard aspect ratio as in the app, at 60 fps and Moonlight's bitrate for
the mode:

| Flag | |
|---|---|
| `--size WxH` | Stream resolution, exactly as given |
| `--fps N` | Frame rate |
| `--mbps N` | Bitrate (otherwise automatic) |
| `--codec h264\|hevc\|av1` | Only this codec |
| `--hdr` | Ask for HDR, when this display shows it and the host can |
| `--yuv444` | Ask for 4:4:4 colour, when this computer decodes it and the host can |
| `--windowed` | A window, not full screen |
| `--no-vsync` | Present frames at once, tearing allowed |
| `--frame-pacing` | One frame per display refresh |
| `--stats` | Show the statistics from the start |
| `--cmd-is-win` | Send Command as the Windows key (macOS) |
| `--mute-in-background` | Silence the stream while its window is not in front |
| `--audio-channels 2\|6\|8` | Stereo, 5.1 or 7.1 |
| `--no-audio` | No sound |
| `--host-audio` | Play the sound on the host too |
| `--steam` | Open Steam Big Picture on the host for the session |
| `--keep-host-displays` | Leave the host's own monitors on |
| `--no-clipboard` | Do not share the clipboard |
| `--wan-only` | Only the internet path (no LAN, no known addresses): to test it |
| `--via ADDR` | Only this address (repeatable): to diagnose a path |

A flag `helio` does not know, or one without its value, stops the command
before it connects, and says which.

Ctrl-C ends the stream properly, as Ctrl+Alt+Shift+Q does, so the host puts
its displays back at once. The exit status is 1 when the stream ended for a
reason other than you quitting.

### Hosts

| Command | Does |
|---|---|
| `helio discover` | List hosts on the local network, paired or not |
| `helio pair HOST[:PORT]` | Pair with a host: shows a PIN to type on the host |
| `helio pair-agent HOST[:PORT]` | Pair this device's AI agent (a key of its own) |
| `helio hosts` | List paired hosts |
| `helio add-host NAME ADDR X25519 MLKEM` | Pair by hand, with the host's keys from `helio host identity` on it |
| `helio remove-host NAME` | Forget a host |
| `helio identity` | Print this device's public keys (X25519 and ML-KEM-768) |

### `helio host` — this computer as a host

| Command | Does |
|---|---|
| `helio host enable` | Turn hosting on: the host starts now and at every start after (see [usage.md](usage.md#hosting-on-and-off)). Windows asks for an administrator |
| `helio host disable` | Turn hosting off: the host stops and no longer starts. Windows asks for an administrator |
| `helio host status [--json]` | `off`, `starting`, `on` or `failed` (with why); for `on`, whether the host's web UI answers on this computer |
| `helio host identity` | Print the host's public keys |
| `helio host clients` | List paired devices and agents: name, X25519 key, `person` or `agent`, and what each may do |
| `helio host permissions X25519 [LIST]` | Show what a paired device or agent may do, or set it. `LIST`: `all`, `none`, `see-only`, or names joined by commas (`view,keyboard,mouse`; the names are in [usage.md](usage.md#what-each-device-may-do)). What a client of its kind cannot have is left out |
| `helio host add-client NAME X25519 MLKEM [--agent] [--permissions LIST]` | Pair a device by hand (headless setups), with the keys from `helio identity` on it, or an agent's from `helio-agent identity` with `--agent`. Without `--permissions` it gets what pairing gives: everything for the first person's device, see only after it; an agent sees and acts |
| `helio host remove-client X25519` | Unpair a device or an agent |

`helio host enable` and `disable` run the host's own program, as the
hosting switch in Helio does, and the program comes with Helio (where, on
each system: [usage.md](usage.md#hosting-on-and-off)). On Windows, turning
hosting on or off needs an administrator: from a terminal that is not
elevated, `helio` asks Windows, which shows its prompt, as the switch does.
Declined, nothing changes, and `helio` exits with 1.

`helio host status --json` prints one line for programs:

```json
{"state":"off"}
{"state":"starting"}
{"state":"on","api":true}
{"state":"failed","reason":"the host exited with code 1; systemd starts it again"}
```

`api` says whether the host answered `GET /api/state` on
`https://localhost:WEB_PORT` with the certificate it made for itself. A host
that has just started takes a moment to answer.

`identity`, `clients`, `permissions`, `add-client` and `remove-client` read
and change the host's data folder. On Windows only administrators may read
it (`C:\ProgramData\Helio Host`, [platforms/windows.md](platforms/windows.md)),
so run them in an administrator terminal there: from another, they change
nothing and exit with 740 (Windows's `ERROR_ELEVATION_REQUIRED`).

What `add-client`, `remove-client` and `permissions` change reaches a running
host when it next starts (`helio host disable`, then `helio host enable`);
Helio's This computer pages and the web UI change it at once.

### Exit status

| Status | Means |
|---|---|
| 0 | Done |
| 1 | It failed, or the command line was not one `helio` takes; why is on stderr |
| 740 | Windows: the host's data needs an administrator's terminal (above) |

## `helio-agent` — AI agents

| Command | Does |
|---|---|
| `helio-agent providers` | What can run here, and which provider is chosen |
| `helio-agent set-key PROVIDER` | Save an API key (`anthropic`, `openai`, `openrouter`, `custom`, or `cloudflare`: the token clef's [screen checks](ai-agents.md#screen-checks-cloudflare-clef) use), read from stdin; an empty line forgets it |
| `helio-agent check SCREEN.png --click X,Y \| --enter \| --typing N \| --personal` | What clef makes of a screenshot, as a screen check would: its answers, and whether the step would wait |
| `helio-agent identity` | The agent's public keys (for `helio host add-client --agent`) |
| `helio-agent hosts` | Hosts the agent is paired with |
| `helio-agent add-host NAME ADDR X25519 MLKEM` | Pair the agent by hand |
| `helio-agent run --host NAME [FLAGS] TASK` | Run one task, printing each action |
| `helio-agent converse --host NAME [FLAGS] MESSAGE [--then MESSAGE ...]` | A conversation: each message a turn, on one connection |
| `helio-agent mcp [FLAGS]` | The MCP server on stdin/stdout (what `Helio mcp` runs) |
| `helio-agent mcp status` | Each agent this computer has (Claude Code, Codex, OpenCode, ...), and whether the server is in its settings |
| `helio-agent mcp install APP ... \| --all` | Add the server to those agents' settings (`claude-code`, `codex`, `opencode`, `gemini`, `cursor`, `copilot`, `droid`, `amp`, `kiro`, `claude-desktop`, `diri`); `--all`: every one found here |
| `helio-agent mcp uninstall APP ... \| --all` | Take it out again |

`run` and `converse` flags: `--host NAME` (the default: the only host the
agent may use), `--provider codex|claude-code|anthropic|openai|openrouter|custom`,
`--model M`, `--effort E`, `--max-actions N`, `--max-minutes N` (run),
`--base-url URL` (run), `--size WxH` (run), `--approvals off|risky|every`
(which steps wait for your yes on the terminal; `--confirm` is `every`),
`--yes` (run: answer yes to the API's safety checks), `--answer yes|no`
(converse: what every question is answered).

`mcp` flags: `--host NAME` (the default host), `--size WxH`,
`--max-actions N`, `--image-dir DIR` (screenshots also saved as files),
`--events FILE` (a JSON line per action), `--no-auto-screenshot`,
`--approvals off|risky|every` with `--control-dir DIR` (where questions and
answers are exchanged; see [ai-agents.md](ai-agents.md#design-decisions-and-why)),
`--hold-wait SECS` (how long an action waits while a person has the
keyboard and mouse), `--data-dir DIR`. `--until` and `--owner-pid` bound a
run started by Helio.

## The apps

- Started as `helio`, or as `Helio cli ARGS` (`helio-app cli ARGS`), the
  app's program is the command line (above), and none of the app.
- `Helio mcp` (or `helio-app mcp`) runs the MCP server, as `helio-agent mcp`;
  `Helio mcp install`, `uninstall` and `status` add it to other agents'
  settings, as `helio-agent mcp install` does (see
  [ai-agents.md](ai-agents.md#3-or-use-your-hosts-from-another-agent-mcp)).
- On Linux, `helio-app --helio-stream` is the stream's own process, started by
  the app.
- `Helio --background` (`helio-app --background`) starts the app as its icon
  only, without the window, when hosting is on, and quits when it is off:
  what Open at login does.
- Starting the app while it runs shows the copy that runs, and adds none
  (one copy per data folder: a `HELIO_DATA_DIR` elsewhere is another app).

## `helio-host`, the host's program

`helio-host` is Helio Host's program, which comes with Helio: what the
system and Helio run, not a command for you. Its hosting commands are
`helio host`'s.

| Command | Run by |
|---|---|
| `helio-host host` (or no command) | Hosting: the LaunchAgent on a Mac (with `--app PATH`, the app the host came with: the host follows it when it is replaced and turns hosting off when it is gone, [platforms/macos.md](platforms/macos.md#hosting-from-helio-and-its-menu-bar-icon)), the systemd user unit on Linux, the service on Windows |
| `helio-host enable`, `disable` | Helio's hosting switch and `helio host enable`, `disable`, which on Windows run them again through UAC when they exit with 740 (`ERROR_ELEVATION_REQUIRED`): nothing was changed |
| `helio-host status [--json]` | The switch (`--json`) and `helio host status` |
| `helio-host service` | Windows: the service itself (`HelioHost`, started by Windows) |
| `helio-host clipboard-agent` | Windows: clipboard sharing as the signed-in user, started by the host for a session |
| `helio-host update-engine PID ENGINE` | Windows: Helio's update while hosting is on, as an administrator, from the engine in `C:\Program Files\Helio Host`: once the update's installer (process PID) has ended well, ENGINE's `enable` ([platforms/windows.md](platforms/windows.md)) |

## Environment variables

For everyone:

| Variable | Used by | Does |
|---|---|---|
| `RUST_LOG` | all | Log filter, e.g. `info,helio_client::stats=debug` |
| `HELIO_DATA_DIR` | Helio, `helio`, `helio-agent` | Helio's data folder |
| `HELIO_HOST_DATA_DIR` | Helio Host, Helio's This computer, `helio host` | Helio Host's data folder. On a Mac, `helio host enable` hands it on to the host it starts |
| `ANTHROPIC_API_KEY`, `OPENAI_API_KEY`, `OPENROUTER_API_KEY`, `HELIO_AGENT_API_KEY` (a custom endpoint) | agents | API keys; win over saved keys |
| `ANTHROPIC_BASE_URL`, `OPENAI_BASE_URL` | agents | Point the API loops at a gateway or proxy |
| `CLOUDFLARE_AUTH_TOKEN`, `CLOUDFLARE_ACCOUNT_ID` | agents | The Workers AI token and account clef's screen checks use; win over Agent setup's |
| `HELIO_APPEARANCE` | the apps | `light` or `dark`, whatever the system says |
| `NO_COLOR`, `CLICOLOR_FORCE` | `helio` | No colour in a terminal; colour even where the output is not one ([no-color.org](https://no-color.org), [bixense.com/clicolors](https://bixense.com/clicolors/)) |
| `HELIO_UPDATE_API` | the apps | Where the update check asks instead of `https://api.github.com`, and where an update's files are downloaded from (a mirror, or a test) |
| `HELIO_SOFTWARE_DECODE` | Helio on Linux | Decode in software even where VA-API is available |
| `HELIO_HOST_CAPTURE` | Helio Host on Linux | `x11` or `portal`, where the session type is not clear |

For diagnosis (see [benchmarks.md](benchmarks.md)):

| Variable | Does |
|---|---|
| `HELIO_SEND_BATCH=N` | Datagrams per send call (default 16; 0: one at a time) |
| `HELIO_RECV_BATCH=N` | macOS client: datagrams per receive call (default 32; 0: one at a time) |
| `HELIO_LAN_SHARDS=0` | The client asks for path-sized datagrams on the LAN too |
| `HELIO_SERVICE_CLASS=0` | Leave the client's traffic unmarked (not Wi-Fi's voice queue) |

For tests, and the ones only tests need, see
[development.md](development.md#test-hooks).
