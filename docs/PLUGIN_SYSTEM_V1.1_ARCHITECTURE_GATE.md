# Plugin System V1.1 — Architecture Gate Review (P5.6)

Review date: 2026-09-16  
Scope: Local `.runory-plugin` install, Core-owned ActionRegistry, Plan≠Execute, ChangeSet + ToolPolicy integration for R2+.

## Gate checklist

| Item | Status | Evidence |
|------|--------|----------|
| Plugin cannot access credential plaintext | PASS | No vault/SSH APIs exposed to plugins; `secretRef` rejected |
| SSH uses Runory Core connection path | PASS | `ssh.command` → `ServerSessionManager::exec` |
| Permission enforcement is Rust-side | PASS | Plan + execute preflight + per-action `PermissionService::require` |
| Plan and execute are separate | PASS | `StoredPlan` + UI approval; execute consumes plan id |
| R2+ operations integrate with ChangeSet | PASS | `map_plan_to_change_steps` → `ChangeSetService::draft/check_policy`; execute `approve_with_preconditions` + `execute_with_policy`; unmapped mutative → `PLUGIN_POLICY_DENIED` |
| ToolPolicy consulted for mapped actions | PASS | `tool_policy_bridge::require_tool_policy_for_change` |
| Audit records plugin identity | PASS | content-free `log_plugin_audit` |
| Workflow outputs are bounded | PASS | sanitize 1 MiB + secret redaction |
| Cancellation works | PASS | CancellationToken + tests |
| Install is atomic | PASS | package tests |
| ZIP traversal is blocked | PASS | ZIP Slip tests |
| No arbitrary plugin executable code | PASS | action allowlist |

## TOCTOU controls

1. Plugin enabled (`require_enabled`)
2. Version match (`IncompatibleVersion`)
3. Permissions reloaded (`preflight_permissions`)
4. Bound ChangeSet version re-checked before approve/execute

## E2E

- Offline: `plugins::security_tests`
- Live SSH (opt-in): `plugins::ssh_e2e_tests` — `cargo test -p runory plugins::ssh_e2e_tests -- --ignored --test-threads=1` after `docker compose up -d --build openssh`

## Known V1 limitations (accepted)

- `secretRef` env still fail-closed (no vault resolution in V1)
- `remote.fs.write` cannot map to `FilePatch` without expected/replacement → PolicyDenied until a typed patch action exists
- Arbitrary `ssh.command` writes are not promoted to Shell ChangeSteps (by design)

## Verdict

**PASS** — P1–P5 foundation complete for local untrusted plugins with Core ChangeSet + ToolPolicy gates.
