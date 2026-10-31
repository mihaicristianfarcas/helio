# Networking: reaching the host from anywhere

On the local network Helio finds Helio Host by mDNS and connects directly. Away from
home, Helio connects the way Tailscale connects machines — addresses
exchanged through a rendezvous, NATs punched from both sides — but without
servers: the rendezvous is the public BitTorrent Mainline DHT, and nothing
needs to be run, forwarded or paid for.

## Ports

| Port | Protocol | What | Reachable from |
|---|---|---|---|
| 47800 | UDP | The tunnel: every stream, all paired clients | wherever clients are |
| 47801 | TCP | Pairing (PIN) and host information | the local network |
| 47802 | TCP (HTTPS) | Helio Host's API, for Helio and `helio host` on the host | the host itself (localhost) |

All three are settings on the host (**Settings → Hosting**, under Ports, or
`port`, `pairing_port` and `api_port` in its `config.toml`). Pairing is
deliberately local: a device is paired once, at home, and then connects from
anywhere. A VPN that puts both on one network (Tailscale, say) counts as
local: Helio pairs at whichever of the host's addresses last answered on its
pairing port, so away from home that is the VPN's.
The client's own tunnel port is chosen once and kept (`port` in its data
folder), so its NAT keeps giving it the same public port.

## What Moonlight + Sunshine/Apollo do, and why it is not enough

Sunshine asks the router for a port mapping over UPnP (when its `upnp`
setting is on; it is off by default), and Moonlight connects
to the host's public address. That needs a router that speaks UPnP and has a
public address. Double NAT (a router behind the ISP's box) and carrier-grade
NAT are common, so a mapping is an optimisation, not the path. Helio asks
for one too, and also punches.

## How it works

1. **Candidates.** Each side collects the addresses it might be reached at:
   its LAN addresses, the public address its NAT gives the tunnel's socket
   (STUN, on that same socket), and a port mapping if the router grants one.
2. **Rendezvous.** Each device has an Ed25519 key, and the host a 32-byte
   rendezvous secret; both are exchanged during PIN pairing, so only paired
   devices can read each other's records. Records are BEP 44 mutable items
   on the Mainline DHT, signed by the device and sealed with
   ChaCha20-Poly1305 under the host's secret. The DHT sees ciphertext under
   a key it cannot link to anyone.
   - The host publishes its candidates when they change, and every 30
     minutes (items expire after about two hours).
   - A client that wants to connect publishes an *intent*: its candidates,
     a timestamp and a nonce.
3. **Punching.** The client races handshake initiations to every host
   candidate (local ones first, remote ones 300 ms later). The host checks
   its paired clients' records every few seconds; on a fresh intent it sends
   initiations to the client's candidates, which opens its own NAT towards
   the client. Whichever handshake lands first wins, and WireGuard roaming
   keeps the path.
4. **Keeping it open.** The tunnel's keepalive holds the NAT mappings during
   a session; the host re-checks its public address by STUN every 25 s.

### The warm path

A cold connect through the DHT takes seconds (finding records costs a few
round trips, and the host notices an intent on its next check). So while
Helio is open it keeps each host's internet address cached, and publishes a
*presence* record per host; the host then sends a small packet every 20 s
towards each present client, keeping its own NAT open towards it. A client's
handshake then walks straight in, and the intent path remains the fallback.

### Is the host up?

Helio's host list asks each paired host every few seconds, as Moonlight polls
its PCs. A host on the local network answers on its pairing port. One that
does not answer there is asked over the tunnel: a WireGuard handshake from
the client's own tunnel port, along every address a stream would try. A
host that completes it is up, wherever it is; a session does not start.

Away from the host's network (the client's public address, by STUN, is not
the one the host publishes), the handshake gets in only once the warm path
is open: within about a minute of Helio starting or the client changing
networks. Until then a silent host keeps the state its card showed
("Checking…" at first), and it is called asleep after 90 s of silence. On
the host's network a silent host is asleep at once.

The probe never runs while a stream as the same identity does, from this
app or another process (`helio stream`): its handshake would replace that
stream's keys on the host. Streams hold `tunnel.lock` in the data folder,
shared; the probe takes it alone or waits for the next poll.

With internet access off (**Allow streaming over the internet**,
`internet_access`), the host is not on the DHT at all, asks no STUN server
and maps no port: it leaves the DHT when the setting goes off, and joins it
again when it comes back on, without a restart.

### Port mapping

With internet access on, Helio Host also asks the router to forward the tunnel's
port: UPnP IGD first (an SSDP search, the WAN IP or PPP connection service,
`AddPortMapping` for UDP with a one-hour lease renewed at half), else NAT-PMP
(RFC 6886) to the default gateway. The mapping's public address joins the
published candidates, and is released when Helio Host stops or the setting is
turned off. A router whose own "public" address is private (behind another
NAT) is left alone: a mapping there would open nothing. The host's Helio
(This computer) shows what happened under **Router**; **Ask the router to
forward the port** (Settings → Hosting, under Network) turns it off.

## Limits

- **Symmetric NAT on both sides** (a new public port for every destination)
  defeats punching. Tailscale falls back to relays; Helio has none.
  Forward UDP 47800 to the host on its router for a path that always works.
- **Cold connects take seconds** (measured 9.6–11.5 s across two NATs); warm
  ones do not. On the local network nothing of this is used.
- A router that does not loop traffic for its own public address back inside
  ("hairpinning") means a client on the same network as the host cannot
  test the internet path; `helio stream NAME --wan-only` needs the client on
  another network.

## Measured

- Joining the DHT: 3–5 s (Helio joins at launch, so a click does not wait).
  Publishing a record: ~4.7 s; resolving one: ~2.5 s.
- A laptop on a phone hotspot to a host at home, `--wan-only` (neither the
  LAN nor a VPN available): the tunnel came up directly between the two
  NATs in 9.6 and 11.5 s; 1080p60 at 10 Mbit/s streamed at 58–60 fps with no
  loss, a 40 ms round trip and 59 ms from the host's capture to the client's
  screen.
- Away from home, with the warm path: the first connect of the day took
  0.35 s (11.8 s before the warm path and handshake racing). The client
  re-races a fresh handshake along every known path each second; WireGuard
  alone retries after 5 s, to one address.

## Where it lives

| Piece | Code |
|---|---|
| STUN codec and NAT probe | `helio-nat/src/stun.rs`, `examples/stun.rs` |
| Rendezvous records (seal, sign, publish, resolve) | `helio-nat/src/rendezvous.rs`, `examples/dht.rs` |
| UPnP / NAT-PMP | `helio-nat/src/portmap.rs` |
| Keys exchanged in pairing | `helio-pairing/src/pair.rs` (`Extras`) |
| Host: publish, watch intents, punch, map the port | `helio-host/src/presence.rs` |
| Client: resolve, publish intent, race, presence | `helio-client/src/wan.rs`, `helio-client/src/stream/net.rs` |
| Client: is the host up (pairing port, then tunnel) | `helio-client/src/pair.rs` (`probe`), `helio-app/src/model.rs` |
