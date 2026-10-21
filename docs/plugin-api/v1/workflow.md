# Workflow DSL `runory/v1`

JSON Schema: [`plugin-sdk/schemas/v1/workflow.schema.json`](../../../plugin-sdk/schemas/v1/workflow.schema.json)

## Document shape

```yaml
apiVersion: runory/v1
kind: Workflow
metadata:
  id: hello.inspect
  name: Inspect Linux Host
  description: optional
inputs: {}          # optional
outputs: {}         # optional
steps:
  - id: detect
    uses: system.detect
```

## Step fields

| Field | Required | Notes |
|-------|----------|--------|
| `id` | yes | Unique within workflow |
| `uses` | yes | Core Action allowlist ID |
| `with` | no | Structured action input object |
| `if` | no | Safe condition subset |
| `timeout` | no | Non-empty string when set |
| `retry.maxAttempts` | no | `1..=5` |
| `retry.delay` | no | Optional delay string |
| `onFailure` | no | `abort` (default) or `continue` |

## Expressions

String interpolation uses `{{ … }}` with a **safe path subset only**:

| Path | Meaning |
|------|---------|
| `inputs.<key>` | Workflow run input |
| `host.id` | Bound host id |
| `host.os` | Bound host OS |
| `steps.<stepId>.outputs.<key>` | Prior step scalar output |

No JavaScript, Python, function calls, or shell substitution.

### Literal `{{` conflict (API gap)

Runory interpolates every `{{ … }}` occurrence in step `with` strings. Docker Go-template formats such as `{{.ID}}` or `{{json .}}` are therefore **not safe** as literal args today.

Workaround for API 1: avoid Docker `--format` Go templates; use default CLI output or parse stdout in Core/UI later. A future additive escape (e.g. raw-string marker) would require a documented schema revision — it is intentionally **not** added in the 1.0 freeze.

## Conditions (`if`)

Supported:

- `true` / `false`
- `path == literal` / `path != literal`
- `path contains literal`

Literals may be quoted strings, booleans, or numbers.

## Explicitly out of API 1

Unless already shipped and tested (they are **not**):

- `parallel`
- `foreach` / `matrix` / `while`
- custom functions
- arbitrary expressions
- dynamic action loading

## Env values

Where actions accept env maps, values may be:

- literal string
- `{ "value": "…" }`
- `{ "secretRef": "secret://…" }`

See [secrets.md](./secrets.md). Note: `ssh.command` V1 rejects `secretRef` in env (use dedicated secret-aware actions such as `remote.fs.write_env`).
