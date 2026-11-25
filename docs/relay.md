# Hosting a GTW Multiplayer Relay

> **Multiplayer is disabled in this version.** Current GTW builds do not
> connect to a relay or read the server list, and the menus have no
> multiplayer or server settings. This document and the relay code are kept
> for a future release.

GTW flight multiplayer uses lightweight UDP relay servers, inherited from the upstream FableFlight relay network. You can host one for friends, join the public relay network, or run a separate private network.

## Quick start

You need a server with Docker and a publicly reachable UDP port **7480**.

From the GTW repository:

```sh
cd relay
docker compose up -d
```

Or run the upstream published image directly (the same relay protocol):

```sh
docker run -d --name gtw-relay --restart unless-stopped \
  -p 7480:7480/udp \
  ghcr.io/danux-be/fableflight-relay
```

Allow inbound **UDP 7480** in your server firewall and hosting provider's firewall.

## Configuration

The relay is configured through environment variables. Add them to your Compose service's `environment:` section or pass them to `docker run` with `-e NAME=value`.

| Variable | Purpose |
| --- | --- |
| `FFMP_PORT` | UDP port used by the relay (normally `7480`). |
| `FFMP_HTTP_PORT` | HTTP status port (normally `7481`); set to `0` to disable. |
| `FFMP_NAME` | Human-readable relay name. |
| `FFMP_PUBLIC` | Public `host[:port]` address advertised to other relays and players. |
| `FFMP_MAX_PLAYERS` | Player limit. |
| `FFMP_RANGE_KM` | Visibility range for nearby players. |
| `FFMP_PEER_SECRET` | Shared secret (at least 16 characters) for relay-to-relay traffic. |
| `FFMP_HUB` | Comma-separated main relay addresses; `none` disables the built-in hubs. |
| `FFMP_PEERS` | Additional relay addresses to connect to directly. |

For example, give your relay a recognizable identity:

```sh
docker run -d --name gtw-relay --restart unless-stopped \
  -p 7480:7480/udp \
  -e FFMP_NAME="My Relay" \
  -e FFMP_PUBLIC="relay.example.com" \
  ghcr.io/danux-be/fableflight-relay
```

Replace `relay.example.com` with a hostname that resolves to your server. If you change the listening UDP port, update the Docker port mapping, firewall rule and advertised address accordingly.

## Status page

The relay exposes `/` and `/status.json` on its HTTP status port (`7481` by default). The page reports connected pilots and start-screen watchers, without publishing aircraft positions. To access it outside the server, expose that port only as needed; for a public status page, put it behind an HTTPS reverse proxy.

The directory endpoint `/servers.json` is also used for relay discovery.

## Public and private networks

Relays can share player information, so pilots on different servers still see one another. Relays exchange signed, dated, sequenced messages using a common `FFMP_PEER_SECRET`. **Do not publish that secret in a repository or documentation.**

By default, relays use GTW's built-in main relays for discovery. These are the upstream relay hosts the simulator ships with. To join an existing coordinated network, configure the network's shared secret and public hostname with its operators. `FFMP_PEERS` can add direct peer connections.

For a separate private network, configure the same secret on your participating relays and use `FFMP_HUB=none` to exclude the built-in hubs, or set `FFMP_HUB` to your own comma-separated hub addresses. Configure clients to reach your private servers directly; they will not automatically appear in the public directory.

## Operating a main relay

If you are adding a new built-in main relay to the project:

1. Give it a stable public hostname, UDP 7480 access, `FFMP_PUBLIC`, `FFMP_NAME`, and the network's shared `FFMP_PEER_SECRET`.
2. Serve its status page and `https://<host>/servers.json` through HTTPS. Leave `FFMP_HUB` unset so it can link with the existing main relays.
3. Add its hostname to `MAIN_RELAYS` in `src/mp_proto.rs` and its directory URL to `DIRECTORY_URLS` in `src/mp.rs`. The project's test checks that the lists agree.
4. Deploy the updated game/relay builds. Older clients can still discover the new relay through existing hubs.

## Troubleshooting

- **Cannot connect:** Check that Docker is running, UDP 7480 is published, and both the host and cloud firewalls allow it. UDP is required; opening only TCP will not work.
- **Not shown in server list:** Confirm `FFMP_PUBLIC` is reachable and that the directory/hub configuration is correct. A quiet relay can disappear from discovery after about a minute.
- **Players on different relays cannot see each other:** Verify that they belong to the same network, use the same `FFMP_PEER_SECRET`, and have reachable peer addresses.
- **Status page unavailable:** Check `FFMP_HTTP_PORT`, the reverse proxy, and whether HTTP access was deliberately disabled.

The relay keeps no persistent player data on disk. It sends aircraft state while multiplayer is enabled; on the start screen, clients request network-map updates instead.
