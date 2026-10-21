# Plugin SDK Schemas

Canonical JSON Schemas for **Runory Plugin API 1.0**:

```text
schemas/v1/manifest.schema.json
schemas/v1/workflow.schema.json
schemas/v1/input.schema.json
```

Developer CLI (from `src-tauri`):

```bash
cargo run -p runory --bin runory-plugin -- init my-plugin --yes
cargo run -p runory --bin runory-plugin -- validate
cargo run -p runory --bin runory-plugin -- pack
cargo run -p runory --bin runory-plugin -- inspect dist/*.runory-plugin
```

- Getting started: [`docs/getting-started.md`](./docs/getting-started.md)
- API reference: [`docs/plugin-api/v1/`](../docs/plugin-api/v1/overview.md)
- Hello template: [`templates/hello/`](./templates/hello/)

These schemas match shipped Core validators. Do not fork copies; extend only via documented additive revisions.
