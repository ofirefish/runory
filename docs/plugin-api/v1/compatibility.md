# Compatibility Policy (Plugin API 1)

## Identifiers

```text
Plugin API:        1
Manifest Schema:   1
Workflow DSL:      runory/v1
```

## Guarantees

Within API `1`, Runory:

- **May** add optional manifest/workflow fields and new Core Actions
- **Must not** change the meaning of existing required fields
- **Must not** silently change permission semantics
- **Must not** lower existing risk semantics
- **Must not** rename existing contribution types

Breaking changes require:

```json
{ "apiVersion": "2" }
```

and/or Workflow DSL `runory/v2`.

## Schemas

Canonical JSON Schemas:

```text
plugin-sdk/schemas/v1/manifest.schema.json
plugin-sdk/schemas/v1/workflow.schema.json
plugin-sdk/schemas/v1/input.schema.json
```

Use these from VS Code / Cursor / CI / future `runory-plugin` CLI. Prefer a single schema source — do not fork hand-maintained copies.

## Compatibility fixtures

| Plugin | Path |
|--------|------|
| Hello | `tests/fixtures/plugins/runory.hello` |
| Django Deployer | `tests/fixtures/plugins/runory.django-deployer` |
| Docker Manager | `tests/fixtures/plugins/runory.docker-manager` |

Index: `tests/plugin-api/v1/fixtures.json`

Rust suite: `cargo test --lib api_v1_compat`

Every future Core change that touches Plugin API surface should keep these fixtures parsing, validating, and planning without breaking changes.
