# SSH Jump Hosts

## Scope

This increment adds one native SSH jump to an ordinary `ServerProfile`:

```text
Runory ── SSH/auth/key verification ──> A
   A    ── SSH direct-tcpip channel ──> B:22
Runory ── SSH/auth/key verification over that channel ──> B PTY
```

A must be a direct profile. B stores only `connectionRoute = jumpHost(profileId)`;
credentials remain in `CredentialService` / `CredentialVault`. Multi-hop chains,
fallback routes, SOCKS and command-based `ssh B` execution are not supported.

## Connection flow

1. Load and authorize both profile IDs in Rust.
2. Scan/approve A's host key and authenticate A with A's credential.
3. Open `direct-tcpip` from A to B and scan B's host key through that byte stream.
4. Store A's live transport in a bounded one-use preparation ticket while B awaits trust.
5. Consume the exact ticket, approve B's route-scoped fingerprint, authenticate B with
   B's credential, and open B's PTY/Shell as the visible `ServerSession`.
6. Retain A for B's session lifetime; disconnect B before releasing A.

Preparation tickets expire after 120 seconds, are never persisted, bind both endpoint
identities, and are capped in memory. Changed fingerprints block before credentials are
sent. Remembered credentials are saved only after successful authentication of that leg.

## Persistence and migration

Catalog schema v2 adds `connection_route_json` and route-scoped known-host keys. Existing
profiles and fingerprints migrate to `direct`. Cloud profile payload v3 includes route
metadata only; it never includes credentials, connection history or terminal output.

## Verification

Unit tests cover route validation, referenced-host deletion, route-scoped fingerprints,
catalog migration and the two-stage UI flow. The Docker fixture adds private B without a
published port and verifies host-key scan, independent authentication and PTY opening:

```powershell
docker compose --profile jump up -d --build openssh openssh-jump-target
cd src-tauri
cargo test jump_host_scans_authenticates_and_opens_target_pty_over_direct_tcpip -- --ignored
```
