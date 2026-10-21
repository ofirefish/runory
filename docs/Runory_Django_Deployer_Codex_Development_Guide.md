# Runory Django Deployer Plugin — Codex Development Guide

> Version: 1.0  
> Target: Runory Plugin System V1  
> Plugin ID: `runory.django-deployer`  
> Audience: Codex / Runory developers  
> Status: Implementation Guide

## 1. Goal

Implement the first production-grade official Runory plugin:

```text
runory.django-deployer
```

The plugin deploys and manages a standard Django application on Linux using:

```text
Git
Python virtualenv
requirements.txt
Django
Gunicorn
systemd
Nginx
```

The plugin is also the reference implementation for validating Runory Plugin System V1. The implementation is complete only when this lifecycle works end-to-end:

```text
Inspect
↓
Preflight
↓
Plan
↓
ChangeSet
↓
Policy
↓
Approval
↓
Deploy
↓
Health Check
↓
Update
↓
Rollback
↓
Audit
```

The plugin must never bypass the existing Plugin System security and execution boundaries.

---

## 2. Existing Runory Foundation

The following functionality already exists and must be reused rather than reimplemented:

```text
src-tauri/src/plugins/

Manifest validation
ZIP secure extraction
plugins.db registry
permission model
Workflow AST
workflow planning
workflow execution
system.detect
ssh.command
remote.fs.*
template.render
```

Runory also already provides structured remote command execution through:

```text
RemoteCommand::dynamic_program
```

Plugins provide `executable`, `args`, `cwd`, `env` and `privilege`; plugins must not control quoting themselves.

Existing IPC:

```text
plugin_*
plugin_workflow_*
```

Existing frontend:

```text
Settings → Plugins
two-stage install flow
Host menu injection
Workflow Plan dialog
Workflow Execute dialog
```

Existing E2E reference fixture:

```text
tests/fixtures/plugins/runory.hello/
tests/fixtures/plugins/runory.hello.runory-plugin
```

`runory.hello` must continue to work after every change made for Django support.

---

## 3. Non-Negotiable Architecture Boundary

The Django plugin is declarative and untrusted.

It must not:

```text
create its own SSH connection
read SSH passwords
read private key bytes
read JumpServer credentials
read Teleport credentials
read Boundary credentials
read secret plaintext
execute arbitrary local code
bypass Permission
bypass ChangeSet
bypass Policy
bypass Audit
```

Every operation must follow:

```text
Django Plugin
   ↓
Workflow Engine
   ↓
Core Action
   ↓
Permission
   ↓
Planner
   ↓
ChangeSet
   ↓
Policy
   ↓
Approval
   ↓
Executor
   ↓
Runory SSH / Remote FS
   ↓
BastionProvider
   ↓
Target Host
```

Do not add a `DjangoSshService`, alternate connection stack, or plugin-specific credential subsystem.

---

## 4. v0.1 Scope

Supported target systems:

```text
Ubuntu 22.04
Ubuntu 24.04
Python 3.10+
```

Supported deployment model:

```text
Git repository
venv
requirements.txt
Gunicorn
systemd
Nginx
```

Required plugin commands:

```text
django.inspect
django.preflight
django.deploy
django.update
django.rollback
django.restart
django.logs
django.health
```

Primary milestone commands:

```text
inspect
preflight
deploy
update
rollback
health
```

Explicitly out of scope for v0.1:

```text
Poetry
PDM
uv
pipenv
Docker
Docker Compose
Kubernetes
uWSGI
Supervisor
Daphne/ASGI deployment
Celery
Redis provisioning
multi-host deployment
load balancers
Let's Encrypt
DNS automation
database provisioning
database rollback
custom React plugin UI
WASM plugin runtime
```

Do not expand scope while implementing the first version.

---

## 5. Plugin Directory

Create:

```text
tests/fixtures/plugins/runory.django-deployer/
```

Structure:

```text
runory.django-deployer/
├── manifest.json
├── README.md
├── workflows/
│   ├── inspect.yaml
│   ├── preflight.yaml
│   ├── deploy.yaml
│   ├── update.yaml
│   ├── rollback.yaml
│   ├── restart.yaml
│   ├── logs.yaml
│   └── health.yaml
├── ui/
│   ├── deploy.schema.json
│   ├── update.schema.json
│   └── rollback.schema.json
├── templates/
│   ├── gunicorn.service.hbs
│   ├── nginx.conf.hbs
│   └── env.hbs
└── assets/
    └── icon.svg
```

Add:

```text
scripts/pack-django-plugin.py
```

or extend the existing fixture packer.

Generated fixture:

```text
tests/fixtures/plugins/runory.django-deployer.runory-plugin
```

---

## 6. Manifest

Baseline `manifest.json`:

```json
{
  "schemaVersion": 1,
  "id": "runory.django-deployer",
  "name": "Django Deployment",
  "version": "0.1.0",
  "description": "Deploy and manage Django applications on Linux servers.",
  "publisher": {
    "id": "runory",
    "name": "Runory"
  },
  "apiVersion": "1",
  "minRunoryVersion": "0.8.0",
  "permissions": [
    "host.read",
    "ssh.exec",
    "ssh.exec.sudo",
    "remote.fs.read",
    "remote.fs.write",
    "remote.fs.delete",
    "secret.reference"
  ],
  "contributes": {
    "commands": [
      {"id":"django.inspect","title":"Inspect Django Environment","category":"Django"},
      {"id":"django.preflight","title":"Django Preflight Check","category":"Django"},
      {"id":"django.deploy","title":"Deploy Django Project","category":"Django"},
      {"id":"django.update","title":"Update Django Project","category":"Django"},
      {"id":"django.rollback","title":"Rollback Django Project","category":"Django"},
      {"id":"django.restart","title":"Restart Django Service","category":"Django"},
      {"id":"django.logs","title":"View Django Logs","category":"Django"}
    ],
    "hostMenus": [
      {"command":"django.inspect","group":"deployment","when":"host.os == 'linux'"},
      {"command":"django.deploy","group":"deployment","when":"host.os == 'linux'"},
      {"command":"django.update","group":"deployment","when":"host.os == 'linux'"},
      {"command":"django.rollback","group":"deployment","when":"host.os == 'linux'"}
    ],
    "workflows": [
      {"id":"django.inspect","file":"workflows/inspect.yaml"},
      {"id":"django.preflight","file":"workflows/preflight.yaml","inputSchema":"ui/deploy.schema.json"},
      {"id":"django.deploy","file":"workflows/deploy.yaml","inputSchema":"ui/deploy.schema.json"},
      {"id":"django.update","file":"workflows/update.yaml","inputSchema":"ui/update.schema.json"},
      {"id":"django.rollback","file":"workflows/rollback.yaml","inputSchema":"ui/rollback.schema.json"},
      {"id":"django.restart","file":"workflows/restart.yaml"},
      {"id":"django.logs","file":"workflows/logs.yaml"}
    ]
  }
}
```

Adapt field names only where required by the already-implemented Manifest schema. Do not fork the schema specifically for this plugin.

---

## 7. Deployment Directory Model

Use immutable releases:

```text
/opt/runory/apps/{projectName}/
├── releases/
│   ├── 20260916143021/
│   ├── 20260916150140/
│   └── 20260916154012/
├── shared/
│   ├── .env
│   ├── media/
│   └── logs/
└── current
    -> releases/20260916154012
```

Runtime values:

```text
appRoot     = /opt/runory/apps/{projectName}
releaseRoot = {appRoot}/releases
sharedRoot  = {appRoot}/shared
releaseDir  = {releaseRoot}/{releaseId}
currentDir  = {appRoot}/current
```

`releaseId` should be generated by Runory Core/workflow runtime, not by embedding shell expressions in YAML.

Recommended format:

```text
YYYYMMDDHHMMSS
```

A deployment must never modify the existing release in place.

---

## 8. Input Schema

Create `ui/deploy.schema.json` using the JSON Schema subset already supported by Runory.

Required fields:

```text
projectName
repository
branch
wsgi
domain
```

Recommended schema:

```json
{
  "type": "object",
  "required": ["projectName", "repository", "branch", "wsgi", "domain"],
  "properties": {
    "projectName": {
      "type": "string",
      "title": "Project Name",
      "pattern": "^[a-zA-Z0-9_-]+$",
      "minLength": 1,
      "maxLength": 64
    },
    "repository": {
      "type": "string",
      "title": "Git Repository"
    },
    "branch": {
      "type": "string",
      "title": "Branch",
      "default": "main"
    },
    "python": {
      "type": "string",
      "title": "Python Executable",
      "default": "python3"
    },
    "wsgi": {
      "type": "string",
      "title": "WSGI Application",
      "description": "Example: config.wsgi:application"
    },
    "domain": {
      "type": "string",
      "title": "Domain"
    },
    "port": {
      "type": "integer",
      "title": "Gunicorn Port",
      "default": 8000,
      "minimum": 1024,
      "maximum": 65535
    },
    "healthPath": {
      "type": "string",
      "title": "Health Check Path",
      "default": "/health/"
    },
    "environmentSecret": {
      "type": "string",
      "format": "secret-ref",
      "title": "Environment Secret"
    }
  }
}
```

If `format: secret-ref` is not already supported by the current form renderer, add it as a generic schema capability, not a Django-specific control.

---

## 9. Secrets

Workflow state may contain:

```text
secret://django/shop-production
```

but never the resolved value.

Never expose these values to plugin-visible state:

```text
SECRET_KEY
DATABASE_PASSWORD
API tokens
Git credentials
```

Core resolves secret references only immediately before the operation requiring them.

Secret values must be redacted from:

```text
stdout
stderr
Tauri events
ChangeSet
Audit
plugins.db
panic/error strings
```

---

## 10. Inspect Workflow

`workflows/inspect.yaml` must be read-only.

Conceptual workflow:

```yaml
apiVersion: runory/v1
kind: Workflow
metadata:
  id: django.inspect
  name: Inspect Django Environment
steps:
  - id: system
    uses: system.detect
  - id: python
    uses: ssh.command
    with:
      executable: python3
      args: ["--version"]
  - id: git
    uses: ssh.command
    with:
      executable: git
      args: ["--version"]
  - id: nginx
    uses: ssh.command
    with:
      executable: nginx
      args: ["-v"]
  - id: systemd
    uses: ssh.command
    with:
      executable: systemctl
      args: ["--version"]
```

Expected risk:

```text
R0
```

This phase validates the real plugin → Workflow → SSH → BastionProvider path before deployment logic is added.

---

## 11. Preflight

`django.preflight` performs no mutation.

Check:

```text
Linux
Ubuntu release
architecture
Python >= 3.10
Git
pip
venv
systemd
Nginx
sudo
disk space
memory
target path
target port
repository access
```

Return structured data, for example:

```json
{
  "os": "ubuntu",
  "osVersion": "24.04",
  "python": "3.12.3",
  "git": "2.43.0",
  "nginx": "1.24.0",
  "systemd": true,
  "sudo": true,
  "diskFreeBytes": 75161927680,
  "memoryTotalBytes": 4294967296,
  "portAvailable": true,
  "ready": true
}
```

Do not automatically install missing OS packages in the first milestone unless a generic package-manager Action already exists.

---

## 12. Generic Core Actions to Add

Only add reusable actions.

Recommended actions:

```text
system.command_exists
system.disk
system.memory
network.tcp_check

git.clone
git.fetch
git.checkout
git.rev_parse

python.detect
python.venv
python.pip.install

remote.fs.mkdir
remote.fs.symlink

systemd.install
systemd.enable
systemd.daemon_reload
systemd.restart
systemd.reload
systemd.status

nginx.test
nginx.reload

http.check

database.migration
changeset.checkpoint
```

Good:

```text
python.venv
systemd.restart
http.check
```

Bad:

```text
django.create_venv
django.restart_gunicorn
django.git_clone
```

Django-specific behavior belongs in Workflow YAML.

---

## 13. Action Requirements

Each new Core Action must define:

```text
input validation
required capabilities
risk calculation
plan output
execution
cancellation
error mapping
output bounds
secret redaction
Audit metadata
```

Each action must continue using existing Runory remote execution services.

---

## 14. Git Actions

Example:

```yaml
- id: clone
  uses: git.clone
  with:
    repository: "{{ inputs.repository }}"
    branch: "{{ inputs.branch }}"
    destination: "{{ workflow.releaseDir }}"
```

Requirements:

```text
no shell concatenation
repository as structured argument
branch as structured argument
destination as structured argument
bounded output
timeout
cancellation
```

`git.rev_parse` should return the deployed commit SHA for result and Audit.

---

## 15. Python Actions

Create venv:

```yaml
- id: venv
  uses: python.venv
  with:
    python: "{{ inputs.python }}"
    path: "{{ workflow.releaseDir }}/.venv"
```

Install dependencies:

```yaml
- id: dependencies
  uses: python.pip.install
  with:
    venv: "{{ workflow.releaseDir }}/.venv"
    requirements:
      file: "{{ workflow.releaseDir }}/requirements.txt"
```

Internally prefer:

```text
{venv}/bin/python -m pip install -r requirements.txt
```

rather than calling `pip` through shell.

`python.pip.install` should be at least R3 because package installation can execute third-party build/install code.

---

## 16. Django Commands

For v0.1 these may remain structured `ssh.command`:

```text
manage.py check
manage.py collectstatic
```

Example:

```yaml
- id: check
  uses: ssh.command
  with:
    executable: "{{ workflow.releaseDir }}/.venv/bin/python"
    args:
      - manage.py
      - check
    cwd: "{{ workflow.releaseDir }}"
```

Do not add `django.manage` yet unless the implementation proves it is needed generically.

---

## 17. Database Migration

Migration is special and must not be treated as an ordinary low-risk SSH command.

Implement:

```text
database.migration
```

or ensure the planner produces an explicit `DatabaseMigration` ChangeSet operation.

Conceptual Workflow:

```yaml
- id: migrate
  uses: database.migration
  with:
    executable: "{{ workflow.releaseDir }}/.venv/bin/python"
    args:
      - manage.py
      - migrate
    cwd: "{{ workflow.releaseDir }}"
```

Core-enforced properties:

```text
Risk = R4
requiresApproval = true
rollback = not guaranteed
```

Plan UI must explicitly display the database risk.

Never automatically execute reverse migrations during application rollback.

---

## 18. Shared Environment

Use:

```text
/opt/runory/apps/{projectName}/shared/.env
```

Systemd references:

```ini
EnvironmentFile=/opt/runory/apps/shop/shared/.env
```

If `.env` contains secrets, Core must render/write it without leaking resolved values into plugin-visible context or logs.

---

## 19. Gunicorn systemd Template

Create `templates/gunicorn.service.hbs`:

```ini
[Unit]
Description=Gunicorn for {{projectName}}
After=network.target

[Service]
Type=simple
User={{runUser}}
Group={{runGroup}}
WorkingDirectory={{currentDir}}
EnvironmentFile={{sharedDir}}/.env
ExecStart={{currentDir}}/.venv/bin/gunicorn --workers {{workers}} --bind 127.0.0.1:{{port}} {{wsgi}}
Restart=always
RestartSec=5

[Install]
WantedBy=multi-user.target
```

Do not default to running Gunicorn as root.

For v0.1, the deployment user may be supplied/configured or pre-created on the test VM.

---

## 20. Nginx Template

Create `templates/nginx.conf.hbs`:

```nginx
server {
    listen 80;
    server_name {{domain}};

    location /static/ {
        alias {{currentDir}}/static/;
    }

    location /media/ {
        alias {{sharedDir}}/media/;
    }

    location / {
        proxy_pass http://127.0.0.1:{{port}};
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
    }
}
```

Write to:

```text
/etc/nginx/sites-available/{projectName}
```

Then create:

```text
/etc/nginx/sites-enabled/{projectName}
```

Before reload, `nginx -t` must succeed.

Never reload Nginx after a failed configuration test.

---

## 21. Health Check

Add generic `http.check`.

Conceptual usage:

```yaml
- id: health
  uses: http.check
  with:
    url: "http://127.0.0.1:{{ inputs.port }}{{ inputs.healthPath }}"
    expectedStatus: [200]
    timeout: 10s
```

The check should run from the remote server context where possible, not from the user's desktop, to avoid local DNS/firewall differences.

Return structured output:

```json
{
  "status": 200,
  "latencyMs": 42,
  "ok": true
}
```

---

## 22. Deploy Workflow

Required order:

```text
Preflight
↓
Create app root
↓
Create shared directories
↓
Create release directory
↓
Clone source
↓
Record Git revision
↓
Create venv
↓
Install requirements
↓
Prepare shared environment
↓
Django check
↓
Database migration
↓
collectstatic
↓
Render systemd unit
↓
Render Nginx config
↓
nginx -t
↓
Checkpoint
↓
Switch current symlink
↓
systemd daemon-reload
↓
enable/restart service
↓
reload Nginx
↓
Health Check
```

Do not switch `current` until the candidate release has completed all checks that can reasonably be performed before activation.

---

## 23. Conceptual deploy.yaml

Adapt this to the actual DSL already implemented; do not change the DSL simply to match the example.

```yaml
apiVersion: runory/v1
kind: Workflow
metadata:
  id: django.deploy
  name: Deploy Django Project
steps:
  - id: detect
    uses: system.detect

  - id: app-root
    uses: remote.fs.mkdir
    with:
      path: "{{ workflow.appRoot }}"

  - id: releases-root
    uses: remote.fs.mkdir
    with:
      path: "{{ workflow.appRoot }}/releases"

  - id: shared-root
    uses: remote.fs.mkdir
    with:
      path: "{{ workflow.appRoot }}/shared"

  - id: release
    uses: remote.fs.mkdir
    with:
      path: "{{ workflow.releaseDir }}"

  - id: clone
    uses: git.clone
    with:
      repository: "{{ inputs.repository }}"
      branch: "{{ inputs.branch }}"
      destination: "{{ workflow.releaseDir }}"

  - id: revision
    uses: git.rev_parse
    with:
      directory: "{{ workflow.releaseDir }}"

  - id: venv
    uses: python.venv
    with:
      python: "{{ inputs.python }}"
      path: "{{ workflow.releaseDir }}/.venv"

  - id: dependencies
    uses: python.pip.install
    with:
      venv: "{{ workflow.releaseDir }}/.venv"
      requirements:
        file: "{{ workflow.releaseDir }}/requirements.txt"

  - id: check
    uses: ssh.command
    with:
      executable: "{{ workflow.releaseDir }}/.venv/bin/python"
      args: ["manage.py", "check"]
      cwd: "{{ workflow.releaseDir }}"

  - id: migrate
    uses: database.migration
    with:
      executable: "{{ workflow.releaseDir }}/.venv/bin/python"
      args: ["manage.py", "migrate"]
      cwd: "{{ workflow.releaseDir }}"

  - id: collectstatic
    uses: ssh.command
    with:
      executable: "{{ workflow.releaseDir }}/.venv/bin/python"
      args: ["manage.py", "collectstatic", "--noinput"]
      cwd: "{{ workflow.releaseDir }}"

  - id: gunicorn-config
    uses: template.render
    with:
      template: templates/gunicorn.service.hbs
      destination: "/etc/systemd/system/{{ inputs.projectName }}.service"

  - id: nginx-config
    uses: template.render
    with:
      template: templates/nginx.conf.hbs
      destination: "/etc/nginx/sites-available/{{ inputs.projectName }}"

  - id: nginx-link
    uses: remote.fs.symlink
    with:
      source: "/etc/nginx/sites-available/{{ inputs.projectName }}"
      destination: "/etc/nginx/sites-enabled/{{ inputs.projectName }}"
      replace: true

  - id: nginx-test
    uses: nginx.test

  - id: checkpoint
    uses: changeset.checkpoint
    with:
      name: before-release-switch

  - id: switch
    uses: remote.fs.symlink
    with:
      source: "{{ workflow.releaseDir }}"
      destination: "{{ workflow.appRoot }}/current"
      replace: true

  - id: daemon-reload
    uses: systemd.daemon_reload

  - id: enable
    uses: systemd.enable
    with:
      service: "{{ inputs.projectName }}"

  - id: restart
    uses: systemd.restart
    with:
      service: "{{ inputs.projectName }}"

  - id: nginx-reload
    uses: nginx.reload

  - id: health
    uses: http.check
    with:
      url: "http://127.0.0.1:{{ inputs.port }}{{ inputs.healthPath }}"
      expectedStatus: [200]
```

---

## 24. Update Workflow

`django.update` must create a new immutable release.

Never update files directly inside `current/`.

Flow:

```text
inspect existing deployment
↓
remember current release
↓
create new release ID
↓
checkout requested revision
↓
create venv
↓
install requirements
↓
check
↓
migrate
↓
collectstatic
↓
checkpoint
↓
switch current
↓
restart
↓
health
```

Keep the prior release available after success.

Suggested future retention:

```text
latest 5 releases
```

Release cleanup can be deferred if it complicates v0.1.

---

## 25. Rollback Workflow

Rollback is code/application rollback only.

Flow:

```text
resolve current target
↓
select previous release
↓
checkpoint
↓
switch current
↓
restart application
↓
health check
```

If the rollback target is unhealthy:

```text
restore original current target
↓
restart
```

Always communicate:

```text
Application files were rolled back.
Database migrations were not automatically reversed.
```

---

## 26. Failure Recovery

### Failure before current switch

Leave production untouched.

```text
candidate release failed
current remains unchanged
stop workflow
```

### Failure after current switch

Automatically restore previous code release:

```text
current -> previous
restart application
health check previous
```

### Failure after migration

Code rollback is allowed.

Database rollback is not automatic.

The execution result must clearly state that migration may already have changed the database.

---

## 27. Checkpoint

Create checkpoint immediately before `current` changes.

Checkpoint should record at minimum:

```text
previous current symlink target
previous release ID
service identity
```

Use the existing interruption/recovery framework where possible.

---

## 28. Interrupted Runs

If Runory exits during a deployment:

```text
Running → Interrupted
```

Do not automatically resume mutating workflows.

At next startup, surface:

```text
project
host
last successful step
release ID
whether current was switched
```

If switch has not occurred, retry can be offered after inspection.

If switch occurred, require explicit recovery/inspection.

---

## 29. Risk Model

Minimum recommended classifications:

| Operation | Risk |
|---|---|
| inspect | R0 |
| logs | R0 |
| remote read | R0 |
| mkdir release | R1/R2 |
| git clone | R1/R2 |
| create venv | R1/R2 |
| pip install | R3 |
| write systemd config | R2/R3 |
| write Nginx config | R2/R3 |
| restart Gunicorn | R3 |
| reload Nginx | R3 |
| database migration | R4 |
| delete release | R3/R4 |

Core owns the final risk classification.

A plugin may never lower Core risk.

---

## 30. ChangeSet UX

Before execution, the user should see something like:

```text
Deploy Django Project

Target
prod-web-01

Project
shop

Source
git@example.com:team/shop.git
main

R1/R2
+ Create release directory
+ Clone source
+ Create virtual environment

R2/R3
+ Write Gunicorn systemd unit
+ Write Nginx configuration
+ Switch current release

R3
! Install Python dependencies
! Restart shop.service
! Reload nginx

R4
!! Run Django database migration

Database migration rollback is not guaranteed.
```

Do not require users to read raw Workflow YAML to understand the deployment.

---

## 31. Policy

Reuse the existing Runory Policy engine.

If needed, minimally extend the existing ChangeSet domain to represent:

```text
DatabaseMigration
```

Do not create `DjangoPolicyEngine`.

Example policy behavior:

```text
host tag = production
operation = DatabaseMigration
risk = R4
→ explicit approval
```

---

## 32. Command Injection Tests

Hostile values must remain literal arguments:

```text
foo; rm -rf /
$(reboot)
`whoami`
" && shutdown -h now
```

Never use:

```rust
format!("git clone {} {}", repo, destination)
```

Use structured arguments all the way into Runory Core command construction.

---

## 33. Remote File Access

The plugin legitimately needs to modify:

```text
/opt/runory/apps
/etc/systemd/system
/etc/nginx/sites-available
/etc/nginx/sites-enabled
```

Do not globally forbid `/etc`.

Control writes through:

```text
Permission
Risk
ChangeSet
Policy
Approval
Audit
```

---

## 34. Audit

Every workflow run should record:

```text
plugin ID
plugin version
workflow ID
host ID
project name
release ID
Git revision
step ID
action ID
risk
approval
start/completion time
result
error/recovery
```

Never record secret plaintext.

---

## 35. Result Model

A successful deployment should return structured output similar to:

```json
{
  "project": "shop",
  "releaseId": "20260916154012",
  "revision": "7cbf28d",
  "current": "/opt/runory/apps/shop/releases/20260916154012",
  "service": "shop.service",
  "health": {
    "status": 200,
    "ok": true
  }
}
```

The existing workflow UI should render these outputs in a reusable way.

---

## 36. Test Django Application

Use a minimal Django test application.

Revision V1:

```text
/        -> Runory Django Demo V1
/health/ -> {"status":"ok","version":"v1"}
```

Revision V2:

```text
/        -> Runory Django Demo V2
/health/ -> {"status":"ok","version":"v2"}
```

Revision V3 should intentionally fail health checking.

This creates a deterministic E2E sequence:

```text
Deploy V1
Update V2
Deploy broken V3
Automatic recovery to V2
Rollback to V1
```

---

## 37. E2E Host

Recommended test VM:

```text
Ubuntu 24.04
2 vCPU
4 GB RAM
20+ GB disk
OpenSSH Server
sudo
```

Prefer a clean VM.

Real-host validation is mandatory because mocks cannot validate:

```text
systemd
Nginx
Unix ownership
symlink behavior
Gunicorn
SSH interruption
```

---

## 38. Required E2E Scenarios

### A. Fresh deploy

```text
install plugin
inspect
preflight
deploy V1
approve R4
health = V1
```

### B. Update

```text
update to V2
new release created
previous release retained
health = V2
```

### C. Rollback

```text
rollback
current -> V1
health = V1
```

### D. Broken release

```text
deploy V3
health fails
current restored to V2
health = V2
Audit records recovery
```

### E. Permission revoked after Plan

```text
Plan
revoke ssh.exec.sudo
Execute
→ denied
```

### F. Plugin disabled after Plan

```text
Plan
disable plugin
Execute
→ PLUGIN_DISABLED
```

### G. Plugin version changes after Plan

```text
Plan under 0.1.0
install/activate 0.1.1
Execute old plan
→ stale plan rejected
```

### H. Migration

```text
Plan shows DatabaseMigration R4
explicit approval required
rollback never reverses migration automatically
```

---

## 39. Frontend

Do not create a large Django-specific page.

Reuse:

```text
Workflow form
Plan dialog
Risk display
Approval
Execution step progress
Logs/output
Result rendering
```

Only add generic UI features if necessary, such as:

```text
R4 database warning
rollback/recovery message
release ID display
structured deployment result
```

---

## 40. Development Phases

### D1 — Fixture and Inspect

Implement:

```text
plugin directory
manifest
inspect workflow
pack script
```

DoD:

```text
installs successfully
Host menu appears
inspect works on Ubuntu
all inspect actions R0
```

### D2 — Preflight

Implement generic actions if missing:

```text
system.command_exists
system.disk
system.memory
network.tcp_check
```

DoD:

```text
structured readiness result
no host mutation
```

### D3 — Git and Python

Implement:

```text
git.clone
git.rev_parse
python.venv
python.pip.install
```

DoD:

```text
release source cloned
Git revision captured
venv created
requirements installed
```

### D4 — Config and Services

Implement:

```text
remote.fs.symlink
systemd.*
nginx.test
nginx.reload
http.check
```

DoD:

```text
Gunicorn runs under systemd
Nginx config validates
Nginx proxies to Gunicorn
health check works
```

### D5 — Database Migration

Implement:

```text
database.migration
```

DoD:

```text
always R4
always explicit approval
never automatically reversed
```

### D6 — Deploy

DoD:

```text
Deploy V1 works end-to-end
health = V1
```

### D7 — Update

DoD:

```text
new immutable release
V1 retained
current -> V2
health = V2
```

### D8 — Rollback

DoD:

```text
current -> V1
service restarted
health = V1
DB untouched
```

### D9 — Failure Recovery

DoD:

```text
broken V3 is activated
health fails
previous release restored
previous health succeeds
Audit records recovery
```

### D10 — API Freeze Review

After all E2E cases pass, review:

```text
Manifest schema
Permission model
Workflow DSL
Action interface
ChangeSet operation model
Plan/Execute contract
```

Only then mark Plugin API / Workflow DSL as 1.0 candidates.

---

## 41. Codex Rules

Codex must:

1. inspect existing code before implementation;
2. reuse current SSH/BastionProvider;
3. reuse Plugin Workflow Engine;
4. reuse Permission;
5. reuse ChangeSet;
6. reuse Policy;
7. reuse Audit;
8. keep storage in the existing independent `plugins.db`;
9. add only generic Core Actions;
10. keep patches small and reviewable;
11. add tests per phase;
12. preserve `runory.hello`.

Codex must not:

```text
create another plugin database
create another SSH client
create Django-specific SSH infrastructure
put credentials into WorkflowContext
use shell concatenation for user-controlled values
allow plugins to lower Core risk
run migrations without approval
automatically reverse migrations
switch current before candidate validation
reload Nginx after nginx -t failure
add Node/Python plugin runtime
add WASM
add Marketplace
refactor unrelated Runory modules
```

---

## 42. Build and Test Requirements

At the end of every phase run the project's standard equivalents of:

```text
cargo check --lib
cargo test
frontend TypeScript check
```

Fix failures in the same phase rather than accumulating them.

Each Codex completion report must include:

```text
files added
files changed
Core Actions added
permissions/risk changes
tests added
commands run
results
known limitations
architecture gaps discovered
```

---

## 43. Final Definition of Done

### Plugin lifecycle

```text
[ ] installs from .runory-plugin
[ ] permissions displayed
[ ] enable/disable/uninstall works
[ ] Host menu works
```

### Inspect / Preflight

```text
[ ] Ubuntu detected
[ ] Python detected
[ ] Git detected
[ ] Nginx detected
[ ] systemd detected
[ ] disk checked
[ ] memory checked
[ ] port checked
[ ] structured readiness produced
```

### Deploy

```text
[ ] app root created
[ ] immutable release created
[ ] repository cloned
[ ] Git revision captured
[ ] venv created
[ ] requirements installed
[ ] Django check runs
[ ] migration is R4
[ ] collectstatic runs
[ ] Gunicorn systemd config generated
[ ] Nginx config generated
[ ] nginx -t required
[ ] current symlink created
[ ] service starts/restarts
[ ] Nginx reloads
[ ] health passes
```

### Update

```text
[ ] new release created
[ ] old release not overwritten
[ ] current switches only after validation
[ ] previous release retained
```

### Rollback

```text
[ ] current switches to previous release
[ ] service restarts
[ ] health validates rollback
[ ] DB migration not reversed automatically
```

### Recovery

```text
[ ] failure before switch leaves current unchanged
[ ] failure after switch restores previous release
[ ] broken health triggers code rollback
[ ] migration warning remains visible
```

### Security

```text
[ ] plugin never sees SSH credentials
[ ] plugin never sees secret plaintext
[ ] structured args prevent shell injection
[ ] execution rechecks permission
[ ] stale Plan rejected after plugin disable
[ ] stale Plan rejected after plugin version change
[ ] R4 cannot bypass approval
```

### Observability

```text
[ ] Audit records plugin/version
[ ] Audit records host/project/release/revision
[ ] Audit records step/risk/approval/result
[ ] no secrets in logs/DB/UI/Audit
```

### Regression

```text
[ ] cargo check --lib passes
[ ] frontend TypeScript check passes
[ ] plugin tests pass
[ ] runory.hello still passes
```

### Real E2E

```text
[ ] Deploy V1
[ ] Update V2
[ ] Broken V3 auto-recovers to V2
[ ] Rollback to V1
```

---

## 44. Prompt to Give Codex

```text
Implement `runory.django-deployer` according to the Runory Django Deployer
Plugin Development Guide.

The Runory Plugin System V1 is already implemented through P1–P5 and includes:
- Manifest validation
- secure ZIP extraction
- independent plugins.db
- permission enforcement
- Workflow AST / Plan / Execute
- system.detect
- ssh.command
- remote.fs.*
- template.render
- structured RemoteCommand::dynamic_program
- plugin IPC
- Settings → Plugins
- Host menu contribution injection
- workflow Plan/Execute UI
- runory.hello E2E fixture

Before modifying code, inspect and reuse the existing implementations for:
- SSH / SFTP
- RemoteCommand::dynamic_program
- BastionProvider
- Permission
- Workflow planner/executor
- ChangeSet
- Policy
- Audit
- plugins.db
- plugin frontend UI

Do not create parallel subsystems.

Proceed through D1–D9 in order. Add generic Core Actions only when required by
the Django plugin. Do not add Django-specific Core Actions when the operation can
be represented generically.

Critical security requirements:
- plugin never receives SSH credentials
- plugin never receives secret plaintext
- all remote execution uses existing Runory SSH/BastionProvider
- user-controlled command values use structured arguments
- plugin cannot control quoting
- permissions are checked again at execution time
- stale Plan is rejected if plugin is disabled or version changed
- database migration is always R4 and requires explicit approval
- database migration is never automatically reversed during rollback

Deployment layout:
/opt/runory/apps/{projectName}/
  releases/{releaseId}/
  shared/
  current -> releases/{releaseId}

Target stack:
Ubuntu 22.04/24.04
Python 3.10+
venv
requirements.txt
Gunicorn
systemd
Nginx

Do not implement:
- Marketplace
- WASM
- Node.js/Python plugin runtime
- custom React plugin UI
- Docker
- Poetry/PDM/uv
- Celery
- multi-host deployment

After each phase:
1. run cargo check --lib;
2. run relevant Rust tests;
3. run frontend TypeScript check;
4. report files changed;
5. report tests added and results;
6. report architecture gaps without automatically expanding scope.

Do not refactor unrelated code.
```

---

## 45. Architectural Outcome

When complete, the path should remain:

```text
User
 ↓
Runory Host
 ↓
Django Plugin Command
 ↓
Workflow Form
 ↓
Planner
 ↓
ChangeSet
 ↓
Policy
 ↓
Approval
 ↓
Executor
 ↓
Generic Core Actions
 ↓
Runory SSH / Remote FS
 ↓
BastionProvider
 ↓
Ubuntu Server
 ↓
Release-based Django Deployment
```

The Django plugin is the first production reference plugin. Once it passes all E2E and security tests, use what was learned to freeze the Plugin API and Workflow DSL 1.0 candidates before implementing Marketplace or third-party publishing.
