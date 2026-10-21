# Runory Plugin API 1.0 — Overview

> **Plugin API:** `1`  
> **Manifest Schema:** `1`  
> **Workflow DSL:** `runory/v1`  
> **Status:** Frozen to match shipped Core (Plugin System P1–P5)

This directory is the **canonical public contract** for third-party and official plugins.

Implementation source of truth:

| Area | Core location |
|------|----------------|
| Manifest model / validation | `src-tauri/src/plugins/manifest/` |
| Permissions | `src-tauri/src/plugins/permissions/` |
| Workflow AST / planner / executor | `src-tauri/src/plugins/workflow/` |
| Core Actions | `src-tauri/src/plugins/workflow/actions/` |
| Secrets | `src-tauri/src/plugins/secrets.rs` |
| Registry lifecycle | `src-tauri/src/plugins/registry/` |
| JSON Schemas | `plugin-sdk/schemas/v1/` |

## Documents

| File | Topic |
|------|--------|
| [manifest.md](./manifest.md) | Manifest fields and validation |
| [permissions.md](./permissions.md) | Capability catalog |
| [workflow.md](./workflow.md) | Workflow DSL `runory/v1` |
| [actions.md](./actions.md) | Core Action contracts |
| [risk-model.md](./risk-model.md) | Risk / approval / ChangeSet |
| [secrets.md](./secrets.md) | Secret references |
| [contributions.md](./contributions.md) | Contribution points |
| [lifecycle.md](./lifecycle.md) | Install / enable / disable / uninstall |
| [agent-tools.md](./agent-tools.md) | Agent Tool metadata |
| [signing.md](./signing.md) | Package digest + Ed25519 |
| [platform-1.0-release-gate.md](./platform-1.0-release-gate.md) | PP10 checklist |

## Design invariants

1. **React is UI only.** Plugins never receive SSH handles, vault plaintext, or unrestricted FS.
2. **Rust Core owns risk.** Plugin-declared risk and Agent Tool metadata cannot downgrade Core classification.
3. **No raw shell interpolation.** `ssh.command` uses structured `executable` + `args` (POSIX-quoted only when cwd/env require a shell wrapper).
4. **Unknown permissions fail closed.**
5. **Unknown Core Actions fail closed.**
6. **Additive evolution only** within API `1`. Breaking changes require API `2`.

## Compatibility fixtures

Official API 1 fixtures:

- `tests/fixtures/plugins/runory.hello`
- `tests/fixtures/plugins/runory.django-deployer`
- `tests/fixtures/plugins/runory.docker-manager` (PP2)

Rust compatibility suite: `src-tauri/src/plugins/api_v1_compat_tests.rs`
