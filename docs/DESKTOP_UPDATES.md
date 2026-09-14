# Desktop automatic updates

Runory checks for signed updates on Windows, macOS, and Linux. The Rust core owns update discovery, download, signature verification, session shutdown, and installation. React only renders metadata and sends explicit user actions.

## Build configuration

The update endpoint and public key are embedded into release binaries at compile time:

```text
RUNORY_UPDATER_ENDPOINT=https://github.com/OWNER/REPOSITORY/releases/latest/download/latest.json
RUNORY_UPDATER_PUBKEY=<complete public key content>
```

The public key and endpoint are public configuration. The signing private key must only be supplied by the release environment:

```text
TAURI_SIGNING_PRIVATE_KEY=<private key path or content>
TAURI_SIGNING_PRIVATE_KEY_PASSWORD=<private key password>
```

Generate the updater key pair once and keep an encrypted offline backup of the private key:

```powershell
pnpm tauri signer generate -w runory-updater.key
```

Build release artifacts with the dedicated configuration so ordinary local builds do not require signing credentials:

```powershell
pnpm tauri build --config src-tauri/tauri.release.conf.json
```

The updater signing key does not replace Windows Authenticode signing or macOS Developer ID signing and notarization.

## GitHub Actions release

The workflow [`.github/workflows/publish.yml`](../.github/workflows/publish.yml) builds Windows, macOS (arm64 + x64), and Linux installers with updater artifacts, then uploads them to a **draft** GitHub Release.

Configure these repository settings before the first run.

Use **Settings → Secrets and variables → Actions → Repository secrets / variables**.
Do **not** put signing keys in Environment secrets — the publish workflow cannot read them, which surfaces as `Missing comment in secret key`.

| Kind | Name | Purpose |
|------|------|---------|
| Variable | `RUNORY_UPDATER_ENDPOINT` | Compile-time update endpoint URL |
| Variable | `RUNORY_UPDATER_PUBKEY` | Full `.pub` file contents |
| Secret | `TAURI_SIGNING_PRIVATE_KEY` | Exact `.key` file contents from `signer generate` (one base64 line; wrapping is OK, workflow strips whitespace) |
| Secret | `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | Only if the key was generated with a password; otherwise delete this secret |

Recommended first-time generation without a password:

```powershell
pnpm tauri signer generate -w runory-updater.key --ci -p ""
```

Then paste the entire `runory-updater.key` into `TAURI_SIGNING_PRIVATE_KEY`, and the entire `runory-updater.key.pub` into `RUNORY_UPDATER_PUBKEY`. Keep an offline backup of the private key; losing it means existing installs cannot verify future updates.

Bump `version` in `package.json`, `src-tauri/Cargo.toml`, and `src-tauri/tauri.conf.json`, then either:

```powershell
git tag v0.1.0
git push origin v0.1.0
```

or run **Actions → publish → Run workflow**.

After all platform jobs finish, open the draft release, confirm installers / updater archives / `.sig` files (and `latest.json` when present), edit release notes, then publish.

## Publishing

Publish the installers, updater archives, and generated `.sig` files before publishing `latest.json`. A static manifest must contain every platform distributed by that release and the literal contents of each matching `.sig` file.

The application waits ten seconds after startup, checks once, and automatically downloads an available signed update. Installation remains explicit because restarting Runory can interrupt SSH sessions and transfers. If sessions are active, the first install request is rejected; a second clearly labelled action disconnects them through `ServerSessionManager` before installation.

Release notes are untrusted remote data. Runory renders them as text and limits them to 4,000 characters.
