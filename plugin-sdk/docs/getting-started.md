# Getting started with Runory plugins

## Install the CLI

From the Runory repository:

```bash
cargo build -p runory --bin runory-plugin
# binary: src-tauri/target/debug/runory-plugin
```

Or:

```bash
cargo run -p runory --bin runory-plugin -- --help
```

## Create a plugin

```bash
runory-plugin init my-plugin --yes \
  --id example.hello \
  --publisher-id example \
  --name "Example Hello"
```

## Validate / pack / inspect

```bash
cd example-hello   # or your output directory
runory-plugin validate
runory-plugin pack
runory-plugin inspect dist/example.hello-1.0.0.runory-plugin
```

Install the `.runory-plugin` file from Runory Desktop → Plugins.

## Canonical contracts

- Schemas: `plugin-sdk/schemas/v1/`
- API docs: `docs/plugin-api/v1/`
- Official fixtures: `tests/fixtures/plugins/`
