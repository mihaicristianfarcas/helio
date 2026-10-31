# Collecting logs for a bug report

A [bug report](https://github.com/mihaicristianfarcas/helio/issues/new?template=bug.yml)
needs the logs from both sides: Helio on the computer you sit at, and Helio
Host on the computer you stream. This page says where they are, and how to
get more out of them.

Logs never contain a PIN, password, key or token. They do contain host
names and addresses: edit them out if you like.

## Where the logs are

| System | Helio | Helio Host |
|---|---|---|
| macOS | `~/Library/Logs/Helio/helio.log` | `~/Library/Application Support/Helio Host/logs/helio-host.log.DATE` |
| Windows | `%LOCALAPPDATA%\Helio\Logs\helio.log` | `C:\ProgramData\Helio Host\logs\helio-host.log.DATE` |
| Linux | `~/.local/state/helio/helio.log` (`$XDG_STATE_HOME/helio`) | `~/.config/helio-host/logs/helio-host.log.DATE` |

`HELIO_HOST_DATA_DIR` moves Helio Host's folder, and its `logs` with it.

### Helio

Helio keeps this launch's log and the two before it: `helio.log`,
`helio.1.log` and `helio.2.log`. Each start moves them along and drops the
oldest, so copy them before you open Helio a third time.

- **On a Mac**, **Help → Show Logs in Finder** opens the folder.
- **On Linux**, a stream is a process of its own, and each one starts a new
  log: after a stream, `helio.log` is the stream's and `helio.1.log` the
  app's. Send all three.

`helio`, `helio-agent` and `Helio mcp` write to the terminal, on standard
error, and to no file. To keep it:

```sh
helio stream gaming-pc --stats 2> helio-stream.log
```

### Helio Host

The host starts a new file each day (`DATE` is the date in UTC) and deletes
none. The easiest way to get the newest one is to copy it from the host,
while hosting is on:

- In Helio on the host: **This computer → Logs**, then **Copy**. That is the
  last 400 lines of the newest file.

For a whole file, or while the host is not running:

- **On Windows** only administrators can read the folder. In a PowerShell
  run as administrator:

  ```powershell
  Copy-Item 'C:\ProgramData\Helio Host\logs\helio-host.log.*' $HOME\Desktop
  ```

- **On Linux** the host's user service also has its output in the journal:

  ```sh
  journalctl --user -u helio-host
  ```

If the host does not start at all, `helio host status` says `failed`, with
why.

### Other logs on Windows

| File | What it says |
|---|---|
| `C:\ProgramData\Helio Host\logs\helio-host-service.log.DATE` | The service: starting the host, and starting it again when it exits |
| `C:\ProgramData\Helio Host\logs\update-engine.log` | Helio's update replacing the host's engine |
| `%APPDATA%\Helio\update-setup.log` | The installer, when Helio updates itself |

## Turning on debug logging

Both programs read `RUST_LOG`, which replaces their default filter. This
value suits either one: it keeps everything else as it is, and turns on
debug logging for all of Helio's own code, the client's line of statistics
a second among it.

```text
info,mainline=error,zbus=error,helio=debug
```

For the detail of one part, add it at `trace`. For example,
`,helio_client::input=trace` logs each input event sent.

### In Helio

Quit Helio first: a second start only brings the running copy forward. With
hosting on, that is **Quit Helio** in the menu of Helio's icon. Then start
it from a terminal, and reproduce the problem. Debug logging lasts until
you quit this copy.

macOS:

```sh
RUST_LOG='info,mainline=error,zbus=error,helio=debug' /Applications/Helio.app/Contents/MacOS/Helio
```

Windows, in PowerShell:

```powershell
$env:RUST_LOG = 'info,mainline=error,zbus=error,helio=debug'
& "$env:LOCALAPPDATA\Programs\Helio\Helio.exe"
```

Linux:

```sh
RUST_LOG='info,mainline=error,zbus=error,helio=debug' ~/.local/bin/helio-app
```

### In Helio Host

The host runs as a service, so its filter is set where the service starts,
and the host restarts to read it. A stream that runs stops while it does.

macOS:

```sh
launchctl setenv RUST_LOG 'info,mainline=error,zbus=error,helio=debug'
launchctl kickstart -k gui/$(id -u)/dev.mihaicristian.helio.host
```

Windows, in a PowerShell run as administrator:

```powershell
$service = 'HKLM:\SYSTEM\CurrentControlSet\Services\HelioHost'
New-ItemProperty $service -Name Environment -PropertyType MultiString -Force `
    -Value 'RUST_LOG=info,mainline=error,zbus=error,helio=debug'
Restart-Service HelioHost
```

Linux:

```sh
systemctl --user set-environment RUST_LOG='info,mainline=error,zbus=error,helio=debug'
systemctl --user restart helio-host
```

### Turning it off

Debug logs grow quickly, so turn it off once you have what you need.

macOS:

```sh
launchctl unsetenv RUST_LOG
launchctl kickstart -k gui/$(id -u)/dev.mihaicristian.helio.host
```

Windows, in a PowerShell run as administrator:

```powershell
Remove-ItemProperty 'HKLM:\SYSTEM\CurrentControlSet\Services\HelioHost' -Name Environment
Restart-Service HelioHost
```

Linux:

```sh
systemctl --user unset-environment RUST_LOG
systemctl --user restart helio-host
```

## What to send

1. Helio's logs and Helio Host's, from around the time it happened.
2. For anything about smoothness or latency, a screenshot of the statistics
   overlay during the problem (Ctrl+Alt+Shift+S;
   [what it shows](usage.md#the-statistics)).
3. Both in a
   [bug report](https://github.com/mihaicristianfarcas/helio/issues/new?template=bug.yml).
