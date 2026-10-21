# Permission Capabilities (API 1)

Canonical catalog — plugins cannot invent new permission IDs.

| Capability | Meaning |
|------------|---------|
| `host.read` | Read host OS / system detection context |
| `host.metadata.read` | Read host metadata |
| `ssh.exec` | Non-interactive remote command execution |
| `ssh.exec.sudo` | Privileged (`privilege: sudo`) remote execution |
| `ssh.shell` | Interactive shell channel (reserved; not required by most workflows) |
| `remote.fs.read` | Remote file read via SFTP / Core FS actions |
| `remote.fs.write` | Remote file create/replace |
| `remote.fs.upload` | Remote upload |
| `remote.fs.delete` | Remote delete |
| `secret.reference` | Bind / resolve opaque `secret://…` refs in Core |
| `network.https` | Outbound HTTPS (reserved for future Core network actions) |

## Enforcement

1. Manifest lists **requested** permissions.
2. Install records grants in `plugins.db`.
3. Each Core Action declares required capabilities for its inputs.
4. Planner and Executor call `PermissionService::require_for_host` and fail closed on deny, including before each action attempt. Action-specific capability checks remain in place.

Scoped requests use `{ "permission": "ssh.exec", "hosts": ["<profile-id>"] }`.
`hosts` contains exact Core profile IDs; an empty list is unrestricted. Wildcards do not grant access to other IDs. A scoped grant requires a matching target; an absent target fails closed. A supplied SSH session must belong to that target.

The installation review shows new capabilities, previously denied capabilities and widened host scopes. The user can deselect a requested capability. Omitted grants stay denied, and dependent operations fail closed. A package cannot claim the identity of a different signer, even when both publishers have trusted keys.

## Not in API 1

These IDs appear in some platform guides but are **not** implemented:

- `remote.fs.chmod`
- `remote.fs.chown`

## Semantics stability

Within Plugin API `1`:

- Existing capability meanings must not silently change.
- New capabilities may be added in a later additive revision with documentation.
- Unknown capabilities always fail validation.
