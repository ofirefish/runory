# Plugin Package Signing (PP4)

## Algorithm

1. Collect all ZIP entries except `META-INF/signature.json`
2. Sort paths lexicographically (UTF-8)
3. For each entry hash:
   - `u64le(path_len)`
   - `path` bytes
   - `u64le(file_len)`
   - `file` bytes
4. `packageSha256 = SHA-256(concatenation)`
5. `signature = Ed25519.sign(packageSha256_bytes)` (raw 32-byte digest)
6. Embed `META-INF/signature.json`

## signature.json

```json
{
  "formatVersion": 1,
  "algorithm": "ed25519",
  "publisher": "runory",
  "keyId": "runory-2026-main",
  "packageSha256": "...",
  "signature": "<base64>"
}
```

## CLI

```bash
runory-plugin keygen --publisher runory --key-id runory-2026-main --out key.json
runory-plugin pack
runory-plugin sign --key key.json dist/plugin.runory-plugin
runory-plugin inspect dist/plugin.runory-plugin
```

Private keys may be password-protected (`--password`) using Argon2id + AES-GCM.

## Desktop policy

| Trust | Install |
|-------|---------|
| Official / Verified / Community Signed | Allowed |
| Unsigned | Only with Plugin Developer Mode |
| Invalid / Revoked / Unknown publisher | Rejected |
