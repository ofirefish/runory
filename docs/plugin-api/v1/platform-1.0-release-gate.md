# Plugin Platform 1.0 — Release Gate (PP10)

## Checklist

### API
- [x] Plugin API 1 documented (`docs/plugin-api/v1/`)
- [x] schemas frozen (`plugin-sdk/schemas/v1/`)
- [x] compatibility fixtures (hello / django / docker)

### Plugins
- [x] hello
- [x] Django
- [x] Docker Manager

### SDK / CLI
- [x] `runory-plugin init|validate|pack|inspect`
- [x] `runory-plugin keygen|sign`

### Security
- [x] Ed25519 package signature
- [x] tamper detection via canonical digest
- [x] publisher keys in `plugins.db`
- [x] revocation feed + local persistence
- [x] unsigned restricted unless Developer Mode

### Marketplace
- [x] `runory-marketplace` backend (SQLite + HTTP)
- [x] desktop Discover client + revocation sync
- [x] update permission-diff helper / activate side-by-side version

### Agent
- [x] `plugin_agent_tools` lists enabled plugin Agent Tool metadata dynamically

## Build

```bash
cargo check --lib --bins
cargo run -p runory --bin runory-plugin -- --help
cargo run --manifest-path marketplace/Cargo.toml -- --bind 127.0.0.1:8787
pnpm typecheck
```
