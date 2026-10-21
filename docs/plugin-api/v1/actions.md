# Core Actions (API 1)

All public action IDs are allowlisted in Core (`workflow/validator.rs`). Plugins cannot register new Core Actions.

**Risk is Core-owned.** Values below are floors / fixed risks as implemented.

Cancellation: every action checks `ActionContext::check_cancelled` where execution is long-running; cancelled runs return `PluginError::Cancelled`.

Secrets: plaintext must never appear in `ActionResult.outputs`, workflow context, or audit payloads. Prefer opaque `secretRef` where supported.

---

## system.detect

| | |
|--|--|
| **Input** | none |
| **Permission** | `host.read` |
| **Risk** | R0 |
| **Outputs** | `os`, `distribution`, `version`, `arch` |

## system.command_exists

| | |
|--|--|
| **Input** | `command` (safe program name) |
| **Permission** | `host.read` |
| **Risk** | R0 |
| **Outputs** | existence result fields |

## system.disk / system.memory

| | |
|--|--|
| **Input** | none (disk/memory probes) |
| **Permission** | `host.read` |
| **Risk** | R0 |

## system.preflight_report

| | |
|--|--|
| **Input** | `host` (default `127.0.0.1`), `port` (default `8000`), `minPython` (default `3.10`) |
| **Permission** | `host.read` |
| **Risk** | R0 |

## network.tcp_check

| | |
|--|--|
| **Input** | `host`, `port` |
| **Permission** | `host.read` |
| **Risk** | R0 |

## ssh.command

| | |
|--|--|
| **Input** | `executable`, `args[]`, `env{}`, `privilege` (`none`\|`sudo`), `cwd?` |
| **Permission** | `ssh.exec`; + `ssh.exec.sudo` when `privilege: sudo` |
| **Risk** | Core classifier (see [risk-model.md](./risk-model.md)); unknown executables default **R3** |
| **Planning** | Emits `RemoteCommand` planned change |
| **Secrets** | `secretRef` env values are **rejected** in V1 |

Preferred pattern for domain CLIs (Docker, etc.):

```yaml
uses: ssh.command
with:
  executable: docker
  args: ["ps", "--format", "{{json .}}"]
```

Note: `{{json .}}` is a Docker format string literal, not a Runory expression (Runory expressions use `{{ inputs… }}` / `{{ steps… }}` paths).

## remote.fs.read

| | |
|--|--|
| **Input** | `path` |
| **Permission** | `remote.fs.read` |
| **Risk** | R0 |
| **Outputs** | `path`, `content`, `size` (1 MiB cap) |

## remote.fs.write

| | |
|--|--|
| **Input** | `path`, `content`, `mode?` |
| **Permission** | `remote.fs.write` |
| **Risk** | R2 |

## remote.fs.write_env

| | |
|--|--|
| **Input** | `path`, `content?` **xor** `secretRef?` |
| **Permission** | `remote.fs.write`; + `secret.reference` when using `secretRef` |
| **Risk** | R2 |

## remote.fs.mkdir

| | |
|--|--|
| **Input** | `path` |
| **Permission** | `remote.fs.write` |
| **Risk** | R1 |

## remote.fs.symlink

| | |
|--|--|
| **Input** | `target`, `linkPath` (alias `link`) |
| **Permission** | `remote.fs.write` |
| **Risk** | R2 |

## template.render

| | |
|--|--|
| **Input** | `template` (package-relative), `destination?`, `variables` |
| **Permission** | `remote.fs.write` when writing destination; else none |
| **Risk** | R2 with destination; R0 render-only |

## git.clone

| | |
|--|--|
| **Input** | `repository`, `branch`, `destination` |
| **Permission** | `ssh.exec` |
| **Risk** | R2 |

## git.rev_parse

| | |
|--|--|
| **Input** | `path` (alias `cwd`) |
| **Permission** | `ssh.exec` |
| **Risk** | R0 |

## git.fetch / git.checkout

| | |
|--|--|
| **Permission** | `ssh.exec` |
| **Risk** | R2 |
| **checkout input** | path + ref fields (validated) |

## python.detect

| | |
|--|--|
| **Input** | `python` (default `python3`) |
| **Permission** | `ssh.exec` |
| **Risk** | R0 |

## python.venv

| | |
|--|--|
| **Input** | `python`, `path` |
| **Permission** | `ssh.exec` |
| **Risk** | R2 |

## python.pip.install

| | |
|--|--|
| **Input** | `venvPath` (alias `venv`), `requirements` (path string or `{ file }`) |
| **Permission** | `ssh.exec` |
| **Risk** | R3 |

## systemd.install

| | |
|--|--|
| **Input** | `path`, `content` |
| **Permission** | `ssh.exec` + `ssh.exec.sudo` |
| **Risk** | R3 |

## systemd.daemon_reload

| | |
|--|--|
| **Permission** | `ssh.exec` + `ssh.exec.sudo` |
| **Risk** | R2 |

## systemd.enable / systemd.reload

| | |
|--|--|
| **Input** | `service` |
| **Permission** | `ssh.exec` + `ssh.exec.sudo` |
| **Risk** | R2 |

## systemd.restart

| | |
|--|--|
| **Input** | `service` |
| **Permission** | `ssh.exec` + `ssh.exec.sudo` |
| **Risk** | R3 |

## systemd.status

| | |
|--|--|
| **Input** | `service` |
| **Permission** | `ssh.exec` |
| **Risk** | R0 |

## nginx.test

| | |
|--|--|
| **Input** | none |
| **Permission** | `ssh.exec` + `ssh.exec.sudo` |
| **Risk** | R2 |

## nginx.reload

| | |
|--|--|
| **Input** | none |
| **Permission** | `ssh.exec` + `ssh.exec.sudo` |
| **Risk** | R3 |

## http.check

| | |
|--|--|
| **Input** | `url` (validated HTTP/HTTPS) |
| **Permission** | `host.read` |
| **Risk** | R0 |

## database.migration

| | |
|--|--|
| **Input** | `executable`, `args` (default `manage.py migrate`), `cwd?`, `description?` |
| **Permission** | `ssh.exec` |
| **Risk** | **R4** (always) |

## changeset.checkpoint

| | |
|--|--|
| **Input** | `currentLink`, `service?`, `name?` |
| **Permission** | `ssh.exec` + `remote.fs.read` |
| **Risk** | R0 |

## release.activate

| | |
|--|--|
| **Input** | `currentLink`, `releaseDir`, `previousTarget`, `service`, `healthUrl`, `migrationRan?` |
| **Permission** | write/exec capabilities as implemented |
| **Risk** | R3 |

---

## Approval behavior

`requires_approval` is true for any plan risk other than **R0** (`risk_requires_approval`). R2+ plugin workflows integrate with ChangeSet / Policy / Approval before mutating execution proceeds.
