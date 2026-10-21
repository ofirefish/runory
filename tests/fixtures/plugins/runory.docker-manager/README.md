# runory.docker-manager

Official Plugin API 1.0 validation plugin for Docker operational workflows.

## Features

| Workflow | Docker CLI | Risk |
|----------|------------|------|
| `docker.inspect-host` | `docker version` | R0 |
| `docker.list` | `docker ps -a` | R0 |
| `docker.inspect` | `docker inspect` | R0 |
| `docker.logs` | `docker logs --tail` | R0 |
| `docker.start` | `docker start` | R2 |
| `docker.stop` | `docker stop` | R3 |
| `docker.restart` | `docker restart` | R3 |
| `docker.pull` | `docker pull` | R3 |

All remote operations use Core Action `ssh.command` with structured `executable` + `args` (no shell interpolation, no Docker-specific Core Actions).

## Security

- Permissions: `host.read`, `ssh.exec`
- Risk classification is Core-owned (`classify_ssh_command_risk`)
- R2/R3 plans require approval / ChangeSet / Policy
- Audit records plugin id, workflow, host, and action
- `docker rm` is intentionally not exposed

## Deferred

- Interactive `docker exec`
- Compose
- Image build
- Registry login
- Container file browser / stats streaming
