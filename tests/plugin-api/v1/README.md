# Plugin API 1.0 Compatibility Fixtures

See `fixtures.json` for the canonical list of plugins and schema paths.

Rust tests live in `src-tauri/src/plugins/api_v1_compat_tests.rs` and validate:

1. Schema files exist and are valid JSON
2. Fixture manifests parse + validate via Core
3. Fixture workflows parse + validate via Core
4. Referenced input schemas are objects with `type: object`
5. Hello + Django (+ Docker when present) remain installable contracts for API 1
