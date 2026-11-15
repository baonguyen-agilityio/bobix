# Bobix relay

The relay forwards end-to-end encrypted packets between a desktop running Bobix and the phones paired with it. A phone
and a desktop never need to reach each other directly; both connect to the relay with a WebSocket.

## What it can and cannot see

It can see: which device ids are connected and when, which device sends a packet to which (by id), the size and time of
a packet, the push tokens a phone registers, and a client's network address while the connection is open.

It cannot read anything inside a packet. A packet is sealed on the sending device for the receiving device; the relay
holds no key that opens one, and has no code or dependency that could (`relay/tests/security.rs` checks this). It
never parses a packet body. A packet it alters, replays or re-routes is refused by the device that receives it.

It keeps nothing about packets after delivery. An undelivered packet waits in memory for at most 10 minutes (64
packets and 2 MiB per device) and is lost when the relay restarts. On disk it keeps only the revoked list and the
pairing record (a small file per desktop in `REVOKED_DIR`, 90 days) and, optionally, a daily log without packet bodies
or tokens (`LOG_DIR`, 14 days).

## Self-hosting

Build the image from the repository root (the relay depends on `crates/bobix-link` by path):

```bash
docker build -f relay/Dockerfile -t bobix-relay .
```

Run it behind a TLS terminator (devices connect with `wss://`). The relay itself speaks plain HTTP and WebSocket on
port 8787.

```bash
docker run --rm -p 8787:8787 -v bobix-relay-data:/data \
  -e RELAY_PUBLIC_URL=wss://relay.example.com \
  -e RELAY_ID=relay.example.com \
  -e INVITE_FILE=/data/invites.txt \
  bobix-relay
```

The image holds the binary only (no shell), runs as a non-root user (uid 65532) and keeps its state under `/data`.
`GET /healthz` answers `ok` (or `503` and the words `invitation file missing or unreadable` / `gateway keys file missing or unreadable` when `INVITE_FILE` or `GATEWAY_KEYS_FILE` is set and cannot be read, so a monitor catches a lost file too; the repository's `relay-health.yml` workflow polls it from the variable `RELAY_URL`); the image has no `HEALTHCHECK` because it has no shell or curl, so point your platform's
check at that path.

Without Docker: `cd relay && cargo build --release`, then run `target/release/bobix-relay` with the same variables.
`relay --dev` runs a local relay for development (see `docs/development.md`, "The relay").

### The settings you must decide

| Variable | Why it matters |
|---|---|
| `RELAY_PUBLIC_URL` | **Required.** The address devices use, such as `wss://relay.example.com`. A device signs this address when it logs in, so a signature made for another relay is refused. Wrong value: every device is refused with `relayUrl` and the log says "signed for ... but this relay's RELAY_PUBLIC_URL is ...". |
| `RELAY_ID` | **Required.** The id push grants name; use the relay's host name. Changing it invalidates the push grants phones already hold. |
| `REVOKED_DIR` | **Required** outside `--dev`; the image sets `/data/revoked`. Must be on a persistent volume, otherwise a restart forgets who was revoked. |
| `INVITE_FILE` | Path of a file with one device id per line (`#` comments). When set, only the computers listed may open a pairing or revoke; everyone else is a guest. **Keep it set during the beta, and do not unset it** until per-sender shares of the global mailbox budget exist (today 1,000 mailboxes and 256 MiB are shared by everyone, so one open relay could be filled by a single client; see #227). Read again every 5 seconds, so no restart is needed. If unset, anyone can use the relay. Put it on the data volume, such as `/data/invites.txt`. |
| `CLIENT_IP_HEADER` | The header your proxy sets to the client's address, such as `X-Real-IP` or `X-Forwarded-For`; the last value is used. It keys the per-address limits (8 unauthenticated sockets, 32 sockets in all, 120 connections a minute; an IPv6 address counts by its /64), 32 unauthenticated sockets per IPv6 /48, and the totals (256 unauthenticated, 4096 open). Set it only when the proxy overwrites that header; otherwise a client picks its own address. If unset, the proxy's address is used and every client shares one limit. |

### Every setting

| Variable | Default | Meaning |
|---|---|---|
| `RELAY_ADDR` | `0.0.0.0:8787` (image: same) | Address to listen on. |
| `LOG_DIR` | none (image: `/data/logs`) | One log file per day, 14 days kept. Standard output always carries the log too. |
| `GATEWAY_KEYS_FILE` | none | Gateway keys, one per line, optionally followed by the relay id the key belongs to. Lets other relays use this one's push gateway. A key with a relay id is bound to that relay; a key without one is a shared key, and its holder names the relay in each push, so give it only to operators you trust (a grant still binds the token, the relay id and an expiry, and cannot be forged). |
| `APNS_KEY_FILE`, `APNS_KEY_ID`, `APNS_TEAM_ID`, `APNS_TOPIC` | none | Apple push credentials (`.p8` key). All four together. |
| `FCM_SERVICE_ACCOUNT_FILE` | none | Google service-account JSON for Android push. |
| `PUSH_GATEWAY_URL`, `PUSH_GATEWAY_KEY_FILE` | none | Send pushes through another relay's gateway instead of holding credentials. The URL must be `https://` (plain `http://` only with `--dev`): the key, push tokens and grants travel there. |
| `RELAY_DEV_FORCE_BIND` | unset | Only with `--dev`: set to `1` to listen beyond loopback. |

Credentials are read from the files named, never from the image or the repository. `POST /v1/push` (the gateway) exists
only on a relay that has its own APNs or FCM credentials, so a relay without them is never an open proxy. Limits
(queue sizes, frame rate, global caps) are described in `docs/development.md`, "The relay".

## What has and has not been tried

Tried: the relay's own tests (`cd relay && cargo test`), `relay --dev` with the link crate's fake desktop and fake phone, and the
phone app's tests against `relay --dev`. **Not tried:** a relay deployed on a public domain behind TLS, the Railway steps below,
real push through Apple (APNs) or Google (FCM) from this relay, the push gateway between two relays, and `relay-health.yml`
watching a live address. Treat the deployment steps as a checklist to follow and confirm, not as a record of a working setup.

## Railway (for the owner, later)

Nothing here has been done yet; it needs your Railway account.

1. New project, "Deploy from GitHub repo", this repository. Do **not** set the service's root directory to `relay`: the
   image needs `crates/bobix-link` too. In the service settings set the **Dockerfile path** to `relay/Dockerfile`
   (build context stays the repository root). Choose the branch you release from.
2. Settings, Networking: generate a public domain (or attach your own). Railway terminates TLS, so devices use
   `wss://<that domain>`. The relay listens on 8787 and ignores `PORT`, so if Railway asks for the target port of the
   domain, enter `8787`.
3. Settings, Deploy: set the **healthcheck path** to `/healthz`.
4. Variables:
   - `RELAY_PUBLIC_URL` = `wss://<that domain>` (exactly what phones and desktops will be given).
   - `RELAY_ID` = that domain's host name.
   - `CLIENT_IP_HEADER` = `X-Real-IP` (Railway's proxy sets the client address there; check the current Railway docs for
     the header it overwrites, and leave this empty rather than guess, since a header the client can set makes the
     limits per client meaningless).
   - `INVITE_FILE` = `/data/invites.txt`. **Keep it set during the beta, and do not unset it** until per-sender shares of the global mailbox budget exist (today 1,000 mailboxes and 256 MiB are shared by everyone, so one open relay could be filled by a single client; see #227).
5. Add a **volume** mounted at `/data`. Without it a redeploy forgets revocations. Railway mounts volumes as root; if the
   relay cannot write to `/data` (uid 65532 has no access), add the variable `RAILWAY_RUN_UID=0`, which is Railway's way
   to run the container as root for volume access.
6. Put your computer's device id (one per line) into `/data/invites.txt`, through the Railway CLI shell or any file
   upload route Railway offers. The relay picks the change up within 5 seconds.
7. Deploy, then check `https://<that domain>/healthz` answers `ok`. Enter `wss://<that domain>` as the relay in Bobix.

Push (APNs and FCM) is not part of the first deploy; add `APNS_*` and `FCM_SERVICE_ACCOUNT_FILE` (as files on the
volume) when the phone app needs them.
