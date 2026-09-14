# MySQL Fleet qualification fixture

This opt-in fixture defaults to a pinned MySQL 8.4.11 GTID source and one read-only replica. It exists only
to qualify Runory's generic Fleet orchestration and verification contracts; no MySQL-specific
command or unrestricted database IPC is registered in the production application.

Start the topology and run its deterministic qualification test:

```powershell
docker compose --profile mysql-fleet up -d mysql-source mysql-replica
cd src-tauri
cargo test mysql_gtid_fixture_replicates_deterministic_write_and_reports_topology -- --ignored
```

For a local compatibility smoke test against another already-cached MySQL 8 image, set
`RUNORY_MYSQL_FIXTURE_IMAGE` before both the Compose and test commands. CI and release
qualification should leave it unset and use the pinned default.

The test writes one UUID-backed probe row on the source, waits for the same value on the
replica, and verifies GTID mode, unique server identities, replica read-only mode, plus the
replication receiver and applier service states. Credentials are fixed test-only values and
the database ports bind to loopback only.
