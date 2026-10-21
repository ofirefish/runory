# Plugin publisher keys

- `*.pub.json` — public Ed25519 keys that may be committed and shipped with Runory.
- Private key JSON (without `.pub`) must **never** be committed. Generate locally:

```bash
cargo run -p runory --bin runory-plugin -- keygen \
  --publisher runory \
  --key-id runory-2026-main \
  --out plugin-sdk/keys/runory-2026-main.json \
  --public-out plugin-sdk/keys/runory-2026-main.pub.json
```

Official desktop Core embeds the public key from `runory-2026-main.pub.json` at startup
(`seed_official_publisher_keys`). Marketplace seeds the same key for
`GET /api/v1/publishers/runory/keys`.
