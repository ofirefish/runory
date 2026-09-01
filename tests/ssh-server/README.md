# Runory SSH/SFTP integration server

Start the real OpenSSH fixture with `docker compose up -d --build openssh`. The development defaults are host `127.0.0.1`, port `2222`, user `runory`, and password `runory-spike`.

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
