# Plugin lifecycle delivery — 2026-10-03

This delivery completes the desktop install/update/review loop on the existing Rust plugin platform. It does not add a new plugin runtime, remote execution API, dependency, or Tauri capability.

## User-visible behavior

- Discover and Updates use real catalog data. Rust compares semantic versions; loading and unavailable states are distinct from an empty result. Refresh is available without reopening the page.
- Local installation and Marketplace installation use the same review dialog. The user sees the verified package identity, signature status, version transition, requested capabilities, host scopes and permission expansion. Capabilities can be deselected; installation requires an explicit confirmation.
- Updates preserve disabled state. Installed descriptions are displayed. Enable/disable failures are localized; controls prevent duplicate mutations. Uninstall asks for confirmation identifying the plugin and explains that remote resources remain.
- Installed lists, host menus and workspace contributions refresh after lifecycle changes. Older asynchronous responses cannot replace newer installed-list or menu state.
- All added product text is available in zh-CN and en-US.

## Core guarantees

- Install IPC accepts a single-use review ID and an explicit grant list. Frontend-supplied paths or Developer Mode overrides cannot authorize installation.
- Reviews bind to a Core-owned package snapshot, hash, identity and installation baseline. They expire after 15 minutes, are limited to eight pending reviews, disappear on restart, and are invalidated by changes to the same plugin. Closing a dialog discards its review; late responses are also discarded.
- Verification happens before review and again at confirmation. The manifest publisher must match the signer. Plugin/version/key revocation is enforced; a later key sync cannot reactivate a revoked key.
- Package, active-version and grant records commit in one transaction. Database failure preserves the previous version and grants and removes the new extraction. `current.json` is an atomic, non-authoritative compatibility hint.
- Downloads require a hash, have a 50 MiB bound and a timeout, and use random staging filenames. Catalog identifiers cannot select a filesystem path. The archive root manifest is authoritative; path aliases, traversal, duplicate case aliases and excessive decompressed content are rejected. Nested files named `META-INF/signature.json` remain covered by the signature digest.
- Workflow targets bind to the actual session profile. Exact host scopes are enforced during planning, execution preflight and each action attempt. Disabled/revoked plugins and stale versions cannot start further actions. Existing ChangeSet and approval boundaries remain in place.

## Verification

- Plugin Rust regression: **153 passed**, with three Docker tests intentionally excluded from the ordinary run. Corrupt persisted host scopes are explicitly tested to fail closed.
- Real Docker OpenSSH plugin integration: **3 passed**, including rejection of a mismatched session target and the existing Nginx ChangeSet binding check.
- Plugin frontend and i18n: **17 passed**.
- Application navigation: the plugin Marketplace switch test passes and preserves terminal mounts; the other ten navigation tests were not included in this targeted run.
- Plugin frontend ESLint, full desktop TypeScript check, and formatting checks for changed Rust files pass.
- `pnpm build` passes; Vite reports the existing large-chunk warning.
- `cargo clippy --lib --bins --offline` passes with existing warnings. Its blocking `never_loop` diagnostic in the Boundary credential parser was fixed by replacing an always-returning loop with `credentials.first()`, preserving its first-item behavior.
- `cargo build --bins --offline` passes for the Windows desktop application and plugin CLI. Both retain the primary Tauri manifest; there are no duplicate manifest resources.
- The two existing Boundary CLI parser tests pass. The built `runory-plugin.exe --help` starts successfully.

The Windows library test executable previously failed to start with `STATUS_ENTRYPOINT_NOT_FOUND` because native dialogs imported Common Controls v6 without an activation manifest. A Windows-only linker manifest now covers library tests; application binaries retain Tauri's primary manifest. The Docker SSH image normalizes shell-script CRLF on build, so Windows checkouts can run the Linux fixture. The local OpenSSH service was stopped after integration validation.

## Repository-wide qualification remains separate

The working tree already contained substantial changes when this task began, including the plugin foundation. This delivery preserves that work. It does not constitute a clean-repository release sign-off.

The initial full ESLint run reported 487 problems, including generated web build files and existing source issues outside the plugin feature. A full frontend test run also reported failures in navigation, Operations, billing, model settings, incident history and other screens, plus timeouts; it was stopped after it ceased making progress. The plugin-specific suites and the targeted navigation test pass. Those wider failures must be resolved before declaring the whole repository release-ready.

No production Marketplace publication, production server operation or external deployment was performed. Validation uses local packages, local HTTP fixtures and the repository's Docker OpenSSH fixture. macOS/Linux/mobile builds and an end-user native-window walkthrough were not exercised in this Windows environment.

## Reproduce

```powershell
pnpm build
node node_modules/eslint/bin/eslint.js src/features/plugins src/lib/tauri/plugins.ts --max-warnings 0
node node_modules/vitest/vitest.mjs run src/features/plugins src/i18n --pool=threads --maxWorkers=2 --minWorkers=1
cargo test --manifest-path src-tauri/Cargo.toml --lib plugins:: --offline -- --test-threads=4
docker compose up -d --build openssh
cargo test --manifest-path src-tauri/Cargo.toml --lib plugins::ssh_e2e_tests --offline -- --ignored --test-threads=1
cargo clippy --manifest-path src-tauri/Cargo.toml --lib --bins --offline
cargo build --manifest-path src-tauri/Cargo.toml --bins --offline
```

See [lifecycle](lifecycle.md) and [permissions](permissions.md) for the installation protocol and host-scope contract.
