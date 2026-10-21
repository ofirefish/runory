# Risk Model (API 1)

Risk levels: `R0` < `R1` < `R2` < `R3` < `R4`.

| Level | Meaning (product) | Approval |
|-------|-------------------|----------|
| R0 | Read-only / detect | No |
| R1 | Low-impact local mutation (e.g. mkdir) | Yes |
| R2 | Service start / config write / soft mutation | Yes + ChangeSet when planned |
| R3 | Restart / stop / package / pull / unknown CLI | Yes + ChangeSet |
| R4 | Destructive / migration / firewall | Yes; fail-closed for Critical paths |

## Ownership

1. **Core Action `risk()` is authoritative.**
2. Plugin Agent Tool `risk` metadata is advisory for UI/Agent presentation only and **must not lower** Core risk.
3. `ssh.command` uses a Core-owned classifier over `executable` + `args` + `privilege`.

## `ssh.command` classifier highlights

- Destructive (`rm`, `mkfs`, …) → R4
- Package managers install/remove/… → R3
- `systemctl restart|stop|disable` → R3; `reload|start|enable` → R2; status probes → R0
- `docker version|ps|inspect|logs|images|info` → R0
- `docker start` → R2
- `docker stop|restart|pull|kill` → R3
- `docker rm|rmi` → R4
- Unknown executable → **R3** (conservative)
- `privilege: sudo` raises floors (never stays at R0/R1)

## ChangeSet / Policy / Audit

- Plans with non-R0 risk require approval.
- Mutating plugin runs bind to ChangeSet / Policy where the plugin ChangeSet integration is enabled.
- Audit events record `plugin_id`, `plugin_version`, `workflow`, `run_id`, `step_id`, `host_id`, and `action` — never secrets.
