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

## Publishing

Publish the installers, updater archives, and generated `.sig` files before publishing `latest.json`. A static manifest must contain every platform distributed by that release and the literal contents of each matching `.sig` file.

The application waits ten seconds after startup, checks once, and automatically downloads an available signed update. Installation remains explicit because restarting Runory can interrupt SSH sessions and transfers. If sessions are active, the first install request is rejected; a second clearly labelled action disconnects them through `ServerSessionManager` before installation.

Release notes are untrusted remote data. Runory renders them as text and limits them to 4,000 characters.
