# Manifest Schema 1

JSON Schema: [`plugin-sdk/schemas/v1/manifest.schema.json`](../../../plugin-sdk/schemas/v1/manifest.schema.json)

## Version identifiers

| Field | Required | Value |
|-------|----------|--------|
| `schemaVersion` | yes | `1` (integer) |
| `apiVersion` | yes | `"1"` (string) |
| `minRunoryVersion` | no | semver string |
| `maxRunoryVersion` | no | semver string |

Example:

```json
{
  "schemaVersion": 1,
  "id": "runory.hello",
  "name": "Runory Hello",
  "version": "1.0.0",
  "publisher": { "id": "runory", "name": "Runory" },
  "apiVersion": "1",
  "minRunoryVersion": "0.1.0"
}
```

## Fields

| Field | Required | Notes |
|-------|----------|--------|
| `schemaVersion` | yes | Must be `1` |
| `id` | yes | `^[a-z0-9][a-z0-9.-]{2,127}$`, length 3–128 |
| `name` | yes | Non-empty |
| `version` | yes | Semver `major.minor.patch` with optional `-prerelease` on patch |
| `description` | no | |
| `publisher.id` | yes | Non-empty |
| `publisher.name` | yes | Non-empty |
| `apiVersion` | yes | Must be `"1"` |
| `minRunoryVersion` | no | Checked at install |
| `maxRunoryVersion` | no | Checked at install |
| `icon` | no | Package-relative path; no `..` / absolute |
| `permissions` | no | Default `[]`; see [permissions.md](./permissions.md) |
| `contributes` | no | Default empty; see [contributions.md](./contributions.md) |

## Not in API 1 (shipped Core)

These guide aspirational fields are **not** part of Manifest Schema 1 today:

- `homepage`
- `license`
- `contributes.templates`

Do not rely on them until a documented additive revision.

## Permission request forms

Simple:

```json
"permissions": ["host.read", "ssh.exec"]
```

Scoped (optional host list):

```json
"permissions": [
  { "permission": "ssh.exec", "hosts": ["prod-1"] }
]
```

Unknown permission IDs fail install validation.

## Path safety

`icon`, `workflows[].file`, and `workflows[].inputSchema` must be package-relative and must not contain `..` or absolute roots.
