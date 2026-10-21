# Plugin Lifecycle (API 1)

State store: **`plugins.db`** (canonical). Do not introduce a second plugin-state DB.

## States

```text
package → inspect → validate → unpack → grant permissions → enabled
                                                      ↓
                                                 disabled
                                                      ↓
                                                 uninstall
```

## Install

1. Core copies the selected package into a bounded temporary snapshot. Marketplace downloads require a SHA-256 hash and are limited to 50 MiB.
2. Validate the root manifest, compatibility, archive paths, publisher identity and signature; check locally known revocations. Marketplace preparation also refreshes the revocation feed.
3. Return an opaque, single-use `reviewId` with the verified manifest, trust status, installed version and new/expanded permission scopes. At most eight reviews may be pending; each expires after 15 minutes and none survives a restart.
4. The user selects grants and explicitly confirms. `plugin_install` accepts only `{ reviewId, grantedPermissions }`; package paths and unsigned-policy overrides are not accepted in this IPC.
5. Core consumes the review, checks the snapshot hash and current installation state, and rechecks signature, revocation and the current Developer Mode setting.
6. Unpack under `plugin.id/version/`. Commit the version, active pointer and grants in one SQLite transaction. A database failure removes the new package directory and preserves the previous registration and grants.
7. Refresh installed plugins, host menus and workspace contributions. Updates preserve the previous enabled/disabled state.

Closing the review calls `plugin_discard_install_review`. A failed confirmation consumes consent and requires a fresh review. Lifecycle mutations invalidate other pending reviews for the same plugin, including disable/re-enable cycles.

`plugin_marketplace_prepare_install` replaces the former direct Marketplace install IPC. It downloads and prepares a review; it does not install or grant permissions. Updates are offered only when Rust's SemVer comparison finds a newer catalog version. No update is silently installed.

`plugins.db` is the authoritative active-version pointer. `current.json` is an atomically replaced legacy hint; Core does not read it.

Same plugin id + version already installed, including an inactive version → `AlreadyInstalled`.

## Enable / Disable

- `set_enabled(plugin_id, bool)` updates `plugins.enabled`
- Disabled plugins cannot start workflows (`require_enabled`)
- Revoked plugins/versions cannot be enabled or execute workflows. Every pending step rechecks the active version and current grants.

## Uninstall

- Removes active registration and package files per registry service rules
- Running plans bound to old versions must fail stale (`PLUGIN_PLAN_STALE` where enforced)

## Runtime binding

Workflow plans capture plugin id + version. If the active version changes after planning, execution must fail closed rather than silently continue on a different package.

When a session is provided, the target is bound to its actual Core-owned profile ID. A caller-supplied mismatched target fails closed. Scoped grants use exact profile IDs and are checked during planning, execution preflight and each action attempt.

## Audit

Lifecycle and execution emit content-free audit fields: plugin id/version, workflow, run/step, host, action name.
