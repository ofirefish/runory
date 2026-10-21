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

Build release artifacts with the dedicated configuration so ordinary local builds do not enable the updater plugin (an empty `plugins.updater.pubkey` makes packaged apps exit immediately on launch):

```powershell
$env:RUNORY_UPDATER_ENDPOINT = "https://github.com/OWNER/REPOSITORY/releases/latest/download/latest.json"
$env:RUNORY_UPDATER_PUBKEY = Get-Content -Raw .\runory-updater.key.pub
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
| Variable | `RUNORY_UPDATER_PUBKEY` | Full `.pub` file contents (one base64 line). Also injected into `plugins.updater.pubkey` for signing; if unset, CI falls back to `runory-updater.key.pub` in the repo |
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

After all platform jobs finish, the workflow runs `verify-updater-manifest`, which downloads the draft release `latest.json` and asserts Windows / macOS arm64 / macOS x64 / Linux entries exist with signatures and HTTPS URLs.

Then open the draft release, confirm installers / updater archives / `.sig` files / `latest.json`, edit release notes, and **Publish** the release. Until Publish, desktop clients request:

```text
https://github.com/ofirefish/runory/releases/latest/download/latest.json
```

and receive HTTP 404 → UI shows `UPDATE_CHECK_FAILED` (“Runory 无法检查更新。”).

Manual publish after a successful build:

1. GitHub → **Releases** → open the draft for the version tag  
2. Confirm `latest.json` lists all four platforms  
3. Click **Publish release**  
4. Verify the URL above returns HTTP 200  

Or re-run **Actions → publish → Run workflow** with **Publish the GitHub Release after verifying latest.json** enabled.

Local verification (requires a token that can read drafts):

```powershell
$env:GITHUB_TOKEN = "ghp_..."
$env:GITHUB_REPOSITORY = "ofirefish/runory"
node scripts/verify-desktop-updater-manifest.mjs --tag v0.1.1
# After review, publish:
node scripts/verify-desktop-updater-manifest.mjs --tag v0.1.1 --publish
```

## Publishing

Publish the installers, updater archives, and generated `.sig` files together with `latest.json`. `tauri-action` merges platform rows into `latest.json` as each matrix job finishes; do not publish a half-finished draft. A static manifest must contain every platform distributed by that release and the literal contents of each matching `.sig` file.

## Troubleshooting

| Symptom | Likely cause |
| --- | --- |
| “Runory 无法检查更新。” / `UPDATE_CHECK_FAILED` | `latest.json` URL 404: no published release, or draft never published |
| “此构建尚未配置自动更新” / `UPDATE_NOT_CONFIGURED` | Binary built without `RUNORY_UPDATER_ENDPOINT` + `RUNORY_UPDATER_PUBKEY` |
| “更新下载失败，或更新签名无效。” / `UPDATE_DOWNLOAD_FAILED` | Asset URL broken or signature mismatch with packaged pubkey |
| Website release catalog updated, desktop still fails | Catalog is unrelated; desktop updater only reads GitHub `latest.json` |

The application waits ten seconds after startup, checks once, and automatically downloads an available signed update. Installation remains explicit because restarting Runory can interrupt SSH sessions and transfers. If sessions are active, the first install request is rejected; a second clearly labelled action disconnects them through `ServerSessionManager` before installation.

Release notes are untrusted remote data. Runory renders them as text and limits them to 4,000 characters.
