# Agent Tools (API 1)

Plugins may declare Agent Tools under `contributes.agentTools`.

## Manifest shape

```json
{
  "name": "docker.list",
  "description": "List Docker containers on the host.",
  "workflow": "docker.list",
  "risk": "R0",
  "requiresApproval": false
}
```

| Field | Required | Notes |
|-------|----------|--------|
| `name` | yes | Tool name exposed to Agent |
| `description` | yes | Human-readable |
| `workflow` | yes | Must refer to a workflow contribution id |
| `risk` | yes | `R0`–`R4` metadata |
| `requiresApproval` | no | Default `false` (UI hint) |

## Rules

1. **Core risk remains authoritative** for the underlying workflow steps / `ssh.command` classifier.
2. Plugin metadata must not be treated as a permission grant.
3. Tools from disabled / revoked / permission-denied plugins must not be exposed (PP9 dynamic registry completes full Agent wiring; metadata contract is frozen here).
4. Execution still goes through plan → permission → approval → ChangeSet → audit.

## Django / Docker examples

- `django.inspect` / `django.deploy` / `django.rollback`
- `docker.list` / `docker.inspect` / `docker.logs` / `docker.restart`
