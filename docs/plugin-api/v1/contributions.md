# Contribution Points (API 1)

Frozen contribution types in Manifest Schema 1:

| Key | Purpose |
|-----|---------|
| `commands` | Command palette / UI command IDs |
| `hostMenus` | Host context menu entries referencing commands |
| `workflows` | Workflow YAML contributions |
| `agentTools` | Agent Tool metadata binding to workflows |

## commands

```json
{ "id": "docker.list", "title": "List Containers", "category": "Docker", "icon": null }
```

- `id` unique within the plugin
- `title` required

## hostMenus

```json
{ "command": "docker.list", "group": "docker", "when": "host.os == 'linux'" }
```

- `command` must reference a declared command id
- `when` must be one of the allowlisted expressions (see manifest schema)

## workflows

```json
{
  "id": "docker.list",
  "file": "workflows/list.yaml",
  "inputSchema": "ui/container.schema.json"
}
```

- `file` / `inputSchema` are package-relative
- Workflow `metadata.id` should match contribution `id` in practice

## agentTools

```json
{
  "name": "docker.restart",
  "description": "Restart a Docker container.",
  "workflow": "docker.restart",
  "risk": "R3",
  "requiresApproval": true
}
```

See [agent-tools.md](./agent-tools.md).

## Not frozen in API 1

- `templates` contribution type (templates are referenced from actions via package paths, not a contribution registry entry)
