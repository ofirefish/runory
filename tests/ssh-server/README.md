# Runory SSH/SFTP integration server

Local TCP tunnel tests add a separate private-network HTTP destination (no published
port), an SSH server with forwarding denied on loopback port 2225, and a
forwarding-only server on port 2226 with `MaxSessions=0`. The latter prohibits
session channels (Shell/PTY/SFTP) but permits direct-tcpip:

```powershell
docker compose --profile tunnels up -d --build openssh tunnel-target openssh-no-forward openssh-forward-only
cd src-tauri
cargo test --lib tunnels:: -- --include-ignored --test-threads=1
```

The tests verify DNS resolution from the SSH server, three simultaneous HTTP streams,
half-close behavior, real TCP checks, port conflict, unreachable/denied distinctions,
stop and immediate port reuse, session/profile binding, disconnect/reconnect, and
continued terminal/SFTP use after stopping a tunnel. The destination fixture is only
reachable on the Compose network; no public HTTP forwarding is created.

Background tests verify authentication without a PTY, sharing across rules,
independence from terminal closure, last-rule release, cancelled starts,
profile-change isolation and release after a bind failure. Historical
terminal-bound transport tests remain test-only regression coverage.

Start the real OpenSSH fixture with `docker compose up -d --build openssh`. The development defaults are host `127.0.0.1`, port `2222`, user `runory`, and password `runory-spike`.

For jump-host coverage, start A plus a target B that has no published host port:

```powershell
docker compose --profile jump up -d --build openssh openssh-jump-target
cd src-tauri
cargo test jump_host_scans_authenticates_and_opens_target_pty_over_direct_tcpip -- --ignored
```

The test authenticates A on `127.0.0.1:2222`, opens SSH `direct-tcpip` to the private `openssh-jump-target:22`, verifies B's own host key, authenticates B independently, and opens B's PTY.

The host key is created inside the container. Runory first scans and displays its SHA256 fingerprint, then requires an explicit **Trust once & connect** action before sending credentials.

Run the real-network regression suite serially:

```powershell
cd src-tauri
cargo test --lib ssh::service::integration_tests -- --ignored --test-threads=1
```

The suite covers password and private-key authentication, encrypted keys, wrong credentials, host verification and changed keys, PTY/resize/reconnect, ten concurrent sessions, 2 MiB terminal output, network interruption, and SFTP directory/stat operations over the existing authenticated SSH transport.

Phase 10J adds an opt-in three-node fault lab. It keeps the original node on port 2222, adds a healthy service node on 2223, and a failed service node on 2224:

```powershell
docker compose --profile fault-lab up -d --build
cd src-tauri
cargo test production_fault_lab_correlates_three_real_openssh_targets -- --ignored
```

The lab only varies typed fixture state. It does not expose an unrestricted command interface or change production execution policy.
