# Runory Plugin Platform 1.0 — Development Implementation Guide

> Version: 1.0  
> Target: Runory Desktop / Plugin Ecosystem  
> Audience: Codex / Runory Developers  
> Prerequisite: Plugin System V1 + Django Deployer completed  
> Status: Implementation Guide

---

# 1. Objective

Runory already has:

- Plugin Core
- Manifest validation
- Secure plugin package extraction
- `plugins.db`
- Permission model
- Workflow AST / Planner / Executor
- ChangeSet / Policy / Audit integration
- Core Actions
- `runory.hello`
- `runory.django-deployer`

The next milestone is no longer "support plugins".

The next milestone is:

```text
Runory Plugin Platform 1.0
```

The platform must allow a third-party developer to:

```text
Create Plugin
↓
Validate Plugin
↓
Package Plugin
↓
Sign Plugin
↓
Publish Plugin
↓
Discover Plugin in Marketplace
↓
Install Plugin
↓
Review Permissions
↓
Run Plugin
↓
Update Plugin
↓
Re-authorize New Permissions
↓
Disable / Uninstall
```

Runory must additionally be able to:

```text
verify publisher identity
verify package integrity
reject tampered packages
revoke vulnerable/malicious plugins
keep Plugin API backward-compatible
expose plugin Agent Tools dynamically
```

---

# 2. Platform Principles

## 2.1 Core stays small

From this stage forward, server-domain features should default to plugins.

Examples:

```text
Django → Plugin
Docker → Plugin
Nginx → Plugin
MySQL → Plugin
PostgreSQL → Plugin
Redis → Plugin
Backup → Plugin
Security Audit → Plugin
Monitoring → Plugin
```

Runory Core should mainly contain:

```text
SSH / SFTP
Host
BastionProvider
Secret
Workflow Engine
Permission
Policy
ChangeSet
Audit
Plugin Runtime
Agent Runtime
Marketplace Client
```

Do not add domain-specific product features directly to Core unless they are required as generic reusable primitives.

## 2.2 Plugin API must become stable

The Django plugin has already exercised the V1 plugin runtime.

Before publishing third-party plugins, freeze:

```text
Manifest Schema
Workflow DSL
Permission Capabilities
Core Action contracts
Risk model
Contribution Points
Secret reference semantics
Plugin lifecycle
Agent Tool metadata
```

After 1.0 freeze, internal refactoring must not casually break API 1 plugins.

## 2.3 Marketplace is not a trust boundary

Marketplace publication does not make a plugin trusted.

Every installed plugin still goes through:

```text
Manifest Validation
Signature Verification
Permission
Policy
ChangeSet
Audit
Runtime Enforcement
```

A signed plugin can still be buggy or malicious.

---

# 3. Overall Roadmap

Implement in this order:

```text
PP1 — Plugin API 1.0 Freeze
PP2 — Docker Manager Validation Plugin
PP3 — Plugin SDK + CLI
PP4 — Publisher Identity + Plugin Signing
PP5 — Marketplace Backend
PP6 — Marketplace Desktop UI
PP7 — Update / Permission Diff / Rollback
PP8 — Security Revocation
PP9 — Agent Tool Integration
PP10 — Platform 1.0 Release Gate
```

Do not start PP5 before PP1–PP4 are stable.

---

# 4. PP1 — Plugin API 1.0 Freeze

## 4.1 Version identifiers

Formalize:

```text
Plugin API: 1.0
Manifest Schema: 1
Workflow DSL: runory/v1
```

Manifest example:

```json
{
  "schemaVersion": 1,
  "apiVersion": "1",
  "minRunoryVersion": "0.9.0"
}
```

## 4.2 Compatibility policy

Plugin API `1` means:

```text
Runory may add optional fields and actions.
Runory must not change the meaning of existing required fields.
Runory must not silently change permission semantics.
Runory must not lower existing risk semantics.
Runory must not rename existing contribution types.
```

Breaking changes require:

```text
apiVersion: "2"
```

## 4.3 Freeze Manifest Schema

Document every supported field:

```text
schemaVersion
id
name
version
description
publisher
apiVersion
minRunoryVersion
maxRunoryVersion
permissions
contributes
icon
homepage
license
```

Freeze contribution types currently supported:

```text
commands
hostMenus
workflows
agentTools
templates
```

Anything experimental should be marked explicitly and excluded from API 1 compatibility guarantees.

## 4.4 Freeze Permission Capabilities

Create one canonical permission list.

Suggested API 1 set:

```text
host.read
host.metadata.read

ssh.exec
ssh.exec.sudo
ssh.shell

remote.fs.read
remote.fs.write
remote.fs.upload
remote.fs.delete
remote.fs.chmod
remote.fs.chown

secret.reference

network.https
```

Do not allow arbitrary plugin-defined permissions.

Unknown permissions fail validation.

## 4.5 Freeze Workflow DSL

Formal API 1 syntax should include only features that are already stable.

Recommended:

```text
metadata
inputs
steps
outputs

uses
with
if
timeout
retry
onFailure

input interpolation
step outputs
host context
plugin context
workflow runtime context
secret-ref

checkpoint
rollback strategy
```

Keep out of API 1 unless already implemented and tested:

```text
parallel
foreach
matrix
while
custom functions
arbitrary expressions
JavaScript
Python
dynamic action loading
```

## 4.6 Freeze Core Action contracts

Document all public Core Actions.

For each action define:

```text
ID
Input schema
Output schema
Required permission
Risk floor
Planning behavior
Cancellation behavior
Secret handling
```

Examples:

```text
system.detect
system.command_exists
system.disk
system.memory

ssh.command

remote.fs.read
remote.fs.write
remote.fs.mkdir
remote.fs.symlink

template.render

git.clone
git.rev_parse

python.venv
python.pip.install

systemd.restart
systemd.reload

nginx.test
nginx.reload

http.check

database.migration
```

## 4.7 Generate JSON Schemas

Create a versioned SDK schema directory:

```text
plugin-sdk/
└── schemas/
    └── v1/
        ├── manifest.schema.json
        ├── workflow.schema.json
        └── input.schema.json
```

Schemas should be usable by:

```text
VS Code
Cursor
Codex
CI
runory-plugin CLI
Marketplace backend
```

There must be one canonical schema source.

Avoid duplicated hand-maintained schemas.

## 4.8 API reference documentation

Create:

```text
docs/plugin-api/v1/
```

Suggested files:

```text
overview.md
manifest.md
permissions.md
workflow.md
actions.md
risk-model.md
secrets.md
contributions.md
lifecycle.md
agent-tools.md
compatibility.md
```

## 4.9 PP1 Definition of Done

```text
[ ] Plugin API version policy documented
[ ] Manifest Schema 1 frozen
[ ] Workflow DSL runory/v1 frozen
[ ] permission catalog frozen
[ ] Core Action public contracts documented
[ ] JSON Schema files generated
[ ] runory.hello validates against schemas
[ ] Django Deployer validates against schemas
[ ] compatibility tests exist
```

---

# 5. PP2 — Docker Manager Validation Plugin

The second production plugin must exercise a different workload from Django.

Create:

```text
runory.docker-manager
```

Purpose:

```text
validate Plugin API against interactive operational tasks
```

## 5.1 Initial Docker features

Implement:

```text
docker.inspect-host
docker.list
docker.inspect
docker.logs
docker.start
docker.stop
docker.restart
docker.pull
```

Defer:

```text
docker exec interactive terminal
Docker Compose
image build
registry login
container file browser
container stats streaming
```

until API 1 is validated.

## 5.2 Risk mapping

Recommended:

```text
docker version     → R0
docker ps          → R0
docker inspect     → R0
docker logs        → R0

docker start       → R2
docker stop        → R3
docker restart     → R3
docker pull        → R3
docker rm          → R4
```

Do not expose `docker rm` in first release if unnecessary.

## 5.3 Docker plugin structure

```text
tests/fixtures/plugins/runory.docker-manager/

├── manifest.json
├── workflows/
│   ├── inspect-host.yaml
│   ├── list.yaml
│   ├── inspect.yaml
│   ├── logs.yaml
│   ├── start.yaml
│   ├── stop.yaml
│   ├── restart.yaml
│   └── pull.yaml
├── ui/
│   ├── container.schema.json
│   └── image.schema.json
└── README.md
```

## 5.4 Generic Core Actions

Do not create an entire Docker API inside Core unless necessary.

The plugin can initially use:

```text
ssh.command
```

with structured arguments.

Example:

```yaml
uses: ssh.command

with:
  executable: docker
  args:
    - ps
    - --format
    - "{{json .}}"
```

If structured parsing becomes common, introduce a generic parser facility later.

Avoid Docker-specific Core Actions unless required by security or generic orchestration.

## 5.5 API validation goal

Docker Manager should answer:

```text
Can Plugin API support resource listing?
Can it handle command output parsing?
Can it display operational results?
Can it perform R2/R3 actions?
Can Agent Tools represent operational commands?
Can permissions and ChangeSet remain generic?
```

If yes, API 1 abstraction is likely stable.

## 5.6 PP2 DoD

```text
[ ] Docker plugin installs without Core architecture changes
[ ] list / inspect / logs work
[ ] start / stop / restart work
[ ] R2/R3 ChangeSet works
[ ] Audit identifies container operation
[ ] permissions are correctly enforced
[ ] Django plugin remains unaffected
[ ] Plugin API does not need breaking changes
```

---

# 6. PP3 — Plugin SDK

Create a distributable developer SDK repository or workspace:

```text
runory-plugin-sdk/
```

Suggested layout:

```text
runory-plugin-sdk/

├── schemas/
│   └── v1/
│       ├── manifest.schema.json
│       ├── workflow.schema.json
│       └── input.schema.json
│
├── templates/
│   ├── hello/
│   ├── workflow-plugin/
│   └── server-ops-plugin/
│
├── examples/
│   ├── hello/
│   ├── nginx/
│   ├── django/
│   └── docker/
│
├── docs/
│   ├── getting-started.md
│   ├── permissions.md
│   ├── workflow.md
│   ├── publishing.md
│   └── security.md
│
└── README.md
```

## 6.1 Developer experience

A plugin developer should be able to start without reading Runory source code.

Target flow:

```text
Install CLI
↓
runory-plugin init
↓
edit manifest/workflow
↓
runory-plugin validate
↓
runory-plugin pack
↓
install local package in Runory
```

---

# 7. PP3 — `runory-plugin` CLI

Build:

```text
runory-plugin
```

Recommended implementation:

```text
Rust
```

because:

- schemas and validation can share logic;
- signing uses safe native crypto;
- cross-platform single binary;
- no Node runtime dependency.

## 7.1 CLI commands

Minimum:

```bash
runory-plugin init
runory-plugin validate
runory-plugin pack
runory-plugin inspect
```

Later:

```bash
runory-plugin keygen
runory-plugin sign
runory-plugin publish
```

## 7.2 `init`

Example:

```bash
runory-plugin init my-plugin
```

Interactive prompts:

```text
Plugin ID:
Publisher ID:
Name:
Description:
Plugin API version:
```

Generate:

```text
my-plugin/
├── manifest.json
├── workflows/
│   └── hello.yaml
└── README.md
```

## 7.3 `validate`

Checks:

```text
Manifest Schema
Workflow Schema
Permission catalog
contribution references
workflow path safety
template path safety
duplicate IDs
version compatibility
input schema
```

Output:

```text
✓ manifest valid
✓ 3 workflows valid
✓ permissions valid
✓ paths valid

Plugin is valid.
```

Failure should include:

```text
file
JSON/YAML path
error code
human-readable message
```

## 7.4 `pack`

Input:

```bash
runory-plugin pack
```

Output:

```text
dist/my-plugin-1.0.0.runory-plugin
```

Packing rules must match desktop installer rules.

Use deterministic archive generation where practical.

## 7.5 `inspect`

```bash
runory-plugin inspect dist/plugin.runory-plugin
```

Display:

```text
Plugin
Version
Publisher
API version
Permissions
Workflows
Package SHA-256
Signature status
```

## 7.6 PP3 DoD

```text
[ ] CLI builds on Windows/macOS/Linux
[ ] init generates valid plugin
[ ] validate uses canonical API 1 schemas
[ ] pack creates installable package
[ ] inspect reports package metadata
[ ] hello plugin can be created from CLI and installed
```

---

# 8. PP4 — Publisher Identity

Marketplace requires stable publisher identities.

Publisher model:

```text
publisher ID
display name
public keys
verification state
metadata
```

Example:

```json
{
  "id": "runory",
  "name": "Runory",
  "verified": true
}
```

## 8.1 Publisher ID

Rules:

```text
globally unique
immutable after creation
lowercase
URL-safe
```

Example:

```text
runory
acme
example-labs
```

## 8.2 Public keys

A publisher may have multiple signing keys.

Model:

```text
publisher_keys

id
publisher_id
algorithm
public_key
status
created_at
revoked_at
```

This allows key rotation.

---

# 9. PP4 — Plugin Signing

Use:

```text
SHA-256
+
Ed25519
```

## 9.1 Signing process

```text
plugin source
↓
pack deterministic archive payload
↓
SHA-256
↓
Ed25519 sign SHA-256
↓
signature metadata
↓
final .runory-plugin
```

## 9.2 Signature metadata

Preferred package layout:

```text
manifest.json
workflows/
templates/
assets/

META-INF/
  signature.json
```

Example:

```json
{
  "formatVersion": 1,
  "algorithm": "ed25519",
  "publisher": "runory",
  "keyId": "runory-2026-main",
  "packageSha256": "...",
  "signature": "..."
}
```

The exact canonicalization/signature procedure must be fully documented.

Do not sign an archive in a way that changes the signed bytes after embedding the signature.

Preferred approach:

```text
calculate a deterministic digest over all package entries except META-INF/signature.json
```

Sort file paths lexicographically and hash:

```text
path length
path bytes
file length
file bytes
```

Define the exact algorithm in API docs.

## 9.3 CLI signing

Commands:

```bash
runory-plugin keygen
```

Generate local Ed25519 private/public key.

Do not upload private key.

Then:

```bash
runory-plugin sign \
  --key publisher-key.pem \
  dist/plugin.runory-plugin
```

## 9.4 Private key handling

CLI should support:

```text
password-protected private key file
OS secure storage later
hardware signing later
```

Never log private key contents.

## 9.5 Desktop verification

Install flow:

```text
package
↓
calculate canonical digest
↓
read signature metadata
↓
resolve publisher public key
↓
verify Ed25519
↓
continue install
```

Verification result:

```text
Official
Verified Publisher
Community Signed
Unsigned
Invalid Signature
Revoked Key
```

## 9.6 Production policy

Recommended:

```text
Marketplace install:
signature required

Local install:
signature required by default

Developer Mode:
allow unsigned
```

Unsigned plugin must be clearly marked.

## 9.7 PP4 DoD

```text
[ ] CLI can generate signing key
[ ] CLI can sign package
[ ] desktop verifies signature
[ ] tampered file fails verification
[ ] unknown publisher handled
[ ] revoked key handled
[ ] unsigned package restricted outside developer mode
```

---

# 10. PP5 — Marketplace Backend

Recommended service:

```text
market.runory.app
```

Backend technology may follow the team's preferred stack.

The protocol matters more than framework choice.

## 10.1 Core entities

Suggested tables:

```text
users
publishers
publisher_members
publisher_keys

plugins
plugin_versions
plugin_files
plugin_permissions

plugin_categories
plugin_category_links

plugin_reviews
plugin_downloads

plugin_security_advisories
```

## 10.2 `plugins`

Fields:

```text
id
publisher_id
name
slug
description
icon_url
homepage
repository_url
license
status
created_at
updated_at
```

Plugin ID should remain canonical:

```text
runory.django-deployer
```

## 10.3 `plugin_versions`

Fields:

```text
plugin_id
version
api_version
min_runory_version
max_runory_version
package_sha256
signature_algorithm
signing_key_id
published_at
status
changelog
```

Version status:

```text
draft
published
deprecated
revoked
```

## 10.4 Plugin file storage

Packages should be stored in object storage.

Example:

```text
plugins/{plugin_id}/{version}/plugin.runory-plugin
```

Marketplace database stores:

```text
path
size
sha256
```

Do not trust object-store metadata as the cryptographic source of truth.

## 10.5 Required APIs

Public:

```http
GET /api/v1/plugins
GET /api/v1/plugins/{id}
GET /api/v1/plugins/{id}/versions
GET /api/v1/plugins/{id}/versions/{version}
GET /api/v1/plugins/{id}/download
GET /api/v1/publishers/{id}
```

Authenticated publisher:

```http
POST /api/v1/plugins
POST /api/v1/plugins/{id}/versions
POST /api/v1/plugins/{id}/publish
```

Security:

```http
GET /api/v1/security/revocations
```

## 10.6 Search

Initial search fields:

```text
name
plugin ID
description
publisher
category
```

Do not overbuild recommendation ranking initially.

## 10.7 Categories

Suggested initial categories:

```text
Deployment
Containers
Web Servers
Databases
Backup
Monitoring
Security
Development
Networking
Automation
```

## 10.8 Publication validation

Marketplace must revalidate package independently.

On upload:

```text
parse package
verify manifest
verify schemas
verify signature
verify publisher matches signer
verify permissions
verify compatibility metadata
calculate SHA-256
```

Do not trust CLI validation alone.

## 10.9 Version immutability

Once:

```text
plugin@1.2.3
```

is published, its package bytes must never change.

To fix:

```text
publish 1.2.4
```

Do not allow replacing 1.2.3.

This is essential for supply-chain integrity.

## 10.10 PP5 DoD

```text
[ ] publisher can upload signed plugin
[ ] server independently validates package
[ ] published version immutable
[ ] plugin metadata query works
[ ] version query works
[ ] package download works
[ ] SHA-256 matches downloaded package
[ ] revoked versions can be marked
```

---

# 11. PP6 — Marketplace Desktop UI

Upgrade existing:

```text
Settings → Plugins
```

to:

```text
Plugins
├── Discover
├── Installed
└── Updates
```

## 11.1 Discover

Features:

```text
search
categories
featured official plugins
publisher badges
compatibility
install
```

Initial cards:

```text
Django Deployment
Docker Manager
Nginx Manager
```

## 11.2 Plugin details

Tabs:

```text
Overview
Permissions
Versions
Changelog
Security
```

Display:

```text
publisher
verification status
latest version
installed version
Plugin API
Runory compatibility
downloads
permissions
security status
```

## 11.3 Permission presentation

Do not show only raw capability names.

Example:

```text
ssh.exec.sudo

→ Execute privileged commands on remote servers
```

Permissions should be grouped:

```text
Remote Execution
Remote Files
Secrets
Network
```

## 11.4 Install flow

```text
Install
↓
download metadata
↓
download package
↓
verify SHA-256
↓
verify signature
↓
manifest validation
↓
compatibility check
↓
permission review
↓
install
↓
register contributions
```

Any validation failure stops installation.

## 11.5 Install states

UI should distinguish:

```text
Downloading
Verifying
Review Permissions
Installing
Installed
Failed
```

## 11.6 Installed tab

Show:

```text
enabled
disabled
version
publisher
permissions
update available
security warning
```

Actions:

```text
Enable
Disable
Uninstall
Update
View details
```

## 11.7 PP6 DoD

```text
[ ] Discover can query Marketplace
[ ] details page works
[ ] signed plugin installs
[ ] permission review works
[ ] installed plugins show status
[ ] install errors are readable
```

---

# 12. PP7 — Plugin Update Model

Updates are security-sensitive.

Update flow:

```text
Installed 1.0.0
↓
Marketplace 1.1.0
↓
fetch metadata
↓
compare compatibility
↓
compare permissions
↓
download
↓
verify
↓
install new version side-by-side
↓
switch active version
```

Do not overwrite the old version before successful validation.

## 12.1 Permission diff

Example:

```text
1.0 permissions:
host.read
ssh.exec

1.1 permissions:
host.read
ssh.exec
ssh.exec.sudo
remote.fs.delete
```

UI:

```text
New permissions requested

+ Execute privileged commands
+ Delete remote files

Review required before update.
```

No automatic update if permission set expands.

## 12.2 No permission expansion

If permissions are unchanged or reduced:

```text
normal update allowed
```

Whether to auto-update should remain a user/product setting, not forced initially.

## 12.3 Update rollback

Keep previous version until new version is active.

Local layout:

```text
plugins/plugin.id/

1.0.0/
1.1.0/

current.json
```

If activation fails:

```text
restore previous active version
```

## 12.4 Workflow stale plan handling

If a workflow plan was generated by:

```text
plugin@1.0.0
```

and plugin active version becomes:

```text
1.1.0
```

old plan must fail:

```text
PLUGIN_PLAN_STALE
```

This is already part of the runtime security model and must remain enforced.

## 12.5 PP7 DoD

```text
[ ] update detected
[ ] package verified before activation
[ ] permission diff calculated
[ ] permission expansion requires approval
[ ] side-by-side install works
[ ] activation rollback works
[ ] stale plans rejected
```

---

# 13. PP8 — Security Revocation

Runory manages production servers, so plugin revocation is mandatory.

Security state:

```text
normal
deprecated
vulnerable
revoked
```

## 13.1 Revocation targets

Support revoking:

```text
publisher key
plugin version
entire plugin
```

Example:

```text
plugin:
thirdparty.example-plugin

version:
2.1.3

status:
revoked
```

## 13.2 Revocation feed

Desktop periodically fetches:

```http
GET /api/v1/security/revocations
```

Response should be cacheable and signed/server-authenticated through normal HTTPS.

Persist last-known security state locally.

## 13.3 Runtime behavior

`vulnerable`:

```text
warn strongly
allow based on policy
```

`revoked`:

```text
disable plugin
block workflows
block Agent Tools
show security alert
```

Do not silently uninstall.

Preserve files for audit and investigation.

## 13.4 Offline behavior

If Runory is offline:

```text
use last-known revocation state
```

Do not treat "cannot reach server" as "safe".

## 13.5 PP8 DoD

```text
[ ] revocation feed works
[ ] revoked version disabled
[ ] workflows blocked
[ ] Agent Tools removed/blocked
[ ] warning displayed
[ ] offline last-known state preserved
```

---

# 14. PP9 — Agent Tool Integration

Installed plugins may contribute Agent Tools.

Example Django:

```text
django.inspect
django.deploy
django.rollback
django.logs
```

Docker:

```text
docker.list
docker.inspect
docker.logs
docker.restart
```

## 14.1 Dynamic Agent Tool Registry

Desired model:

```text
Core Agent Tools
+
Enabled Plugin Agent Tools
```

When plugin:

```text
installed
enabled
disabled
updated
revoked
uninstalled
```

Agent registry must refresh.

## 14.2 Agent Tool metadata

Manifest contribution:

```json
{
  "name": "docker.restart",
  "description": "Restart a Docker container.",
  "workflow": "docker.restart",
  "risk": "R3",
  "requiresApproval": true
}
```

Core risk is still authoritative.

Plugin metadata may not downgrade it.

## 14.3 Agent execution flow

Example user request:

```text
The Django website is down. Check it and recover it.
```

Agent:

```text
django.inspect
↓
django.logs
↓
nginx test
↓
systemd status
↓
generate recovery ChangeSet
↓
approval
↓
restart
↓
http check
```

Agent must not bypass plugin approval or Policy.

## 14.4 Tool exposure

Do not expose tools from:

```text
disabled plugin
incompatible plugin
revoked plugin
permission-denied plugin
```

## 14.5 PP9 DoD

```text
[ ] plugin Agent Tools dynamically register
[ ] disable removes tools
[ ] uninstall removes tools
[ ] update refreshes metadata
[ ] revoked plugin tools blocked
[ ] approval still enforced
[ ] Audit records Agent-triggered plugin execution
```

---

# 15. Developer Mode

Developer Mode is needed for local plugin development.

Add:

```text
Settings
→ Developer
→ Plugin Developer Mode
```

Capabilities:

```text
install unsigned local package
show unsigned warning
reload local plugin
open plugin directory
```

Potential future:

```text
dev directory linking
hot reload workflow definitions
```

Do not enable unsigned install by default.

---

# 16. Plugin Trust UI

Trust status should be explicit.

Recommended states:

```text
Runory Official
Verified Publisher
Signed Community Plugin
Unsigned Development Plugin
Invalid Signature
Revoked
```

Do not use only color.

Always include text/icon status for accessibility.

---

# 17. Security Hardening

Before Marketplace launch, complete a plugin-specific security review.

Required tests:

```text
ZIP Slip
symlink archive escape
archive bomb
huge file count
manifest parser abuse
path traversal
signature tamper
publisher mismatch
revoked key
permission bypass
stale plan
secret leakage
command injection
malicious template path
oversized stdout/stderr
workflow cancellation
plugin crash recovery
marketplace metadata spoofing
download hash mismatch
```

---

# 18. Marketplace Download Security

Installation should use both:

```text
Marketplace metadata SHA-256
Package embedded signature
```

Flow:

```text
HTTPS download
↓
SHA-256 match Marketplace
↓
Ed25519 package verification
↓
publisher verification
```

If either fails:

```text
reject
```

---

# 19. Supply Chain Rules

Mandatory rules:

```text
Published version is immutable.
Package hash is immutable.
Signature is required.
Publisher ID cannot be changed for a published plugin.
Signing key rotation must be tracked.
Revoked signing key cannot publish new versions.
```

---

# 20. Telemetry and Privacy

If plugin download counts or diagnostics are collected:

Do not send:

```text
hostnames
server addresses
SSH usernames
commands
workflow inputs
secret references
project source URLs
```

without a clear product reason and policy.

Marketplace analytics should default to minimal metadata:

```text
plugin ID
plugin version
Runory version
install/update event
```

subject to product privacy policy.

---

# 21. Local Database Changes

Continue using:

```text
plugins.db
```

Do not introduce another desktop plugin-state DB.

Suggested additional tables:

```sql
CREATE TABLE plugin_publishers (
    publisher_id TEXT PRIMARY KEY,
    display_name TEXT NOT NULL,
    verification_status TEXT NOT NULL,
    updated_at INTEGER NOT NULL
);
```

```sql
CREATE TABLE plugin_versions_security (
    plugin_id TEXT NOT NULL,
    version TEXT NOT NULL,
    security_status TEXT NOT NULL,
    advisory_id TEXT,
    checked_at INTEGER NOT NULL,
    PRIMARY KEY (plugin_id, version)
);
```

```sql
CREATE TABLE plugin_marketplace_cache (
    plugin_id TEXT PRIMARY KEY,
    metadata_json TEXT NOT NULL,
    cached_at INTEGER NOT NULL
);
```

Reuse current installed-plugin/version tables for active state.

---

# 22. Marketplace Client Module

Suggested Rust structure:

```text
src-tauri/src/plugins/marketplace/

├── mod.rs
├── client.rs
├── model.rs
├── download.rs
├── cache.rs
├── security.rs
└── error.rs
```

Signing verification:

```text
src-tauri/src/plugins/signing/
├── mod.rs
├── digest.rs
├── verifier.rs
└── model.rs
```

Do not mix network Marketplace code into Plugin Registry.

---

# 23. Frontend Structure

Suggested:

```text
src/features/plugins/

marketplace/
├── api.ts
├── types.ts
├── DiscoverPage.tsx
├── PluginDetailsPage.tsx
├── SearchBar.tsx
├── CategoryFilter.tsx
└── PublisherBadge.tsx

installed/
├── InstalledPluginsPage.tsx
├── UpdateDialog.tsx
├── PermissionDiff.tsx
└── SecurityAlert.tsx
```

Reuse existing plugin components where possible.

---

# 24. Error Codes

Add stable error codes.

Marketplace:

```text
PLUGIN_MARKETPLACE_UNAVAILABLE
PLUGIN_NOT_FOUND
PLUGIN_VERSION_NOT_FOUND
PLUGIN_DOWNLOAD_FAILED
PLUGIN_HASH_MISMATCH
```

Signing:

```text
PLUGIN_SIGNATURE_MISSING
PLUGIN_SIGNATURE_INVALID
PLUGIN_SIGNING_KEY_UNKNOWN
PLUGIN_SIGNING_KEY_REVOKED
PLUGIN_PUBLISHER_MISMATCH
```

Update:

```text
PLUGIN_UPDATE_PERMISSION_REVIEW_REQUIRED
PLUGIN_UPDATE_INCOMPATIBLE
PLUGIN_PLAN_STALE
```

Security:

```text
PLUGIN_REVOKED
PLUGIN_VERSION_REVOKED
PLUGIN_SECURITY_STATUS_UNKNOWN
```

---

# 25. Testing Strategy

## Unit

```text
schema validation
semver compatibility
permission diff
canonical package digest
Ed25519 verification
revocation matching
Marketplace DTO parsing
update selection
```

## Integration

```text
publish → query → download
download → hash verify → signature verify
install → update
update with new permissions
revoke → desktop disable
```

## E2E

```text
Discover Django
Install
Run workflow
Update
Disable
Enable
Uninstall
```

Docker should also be included.

---

# 26. Compatibility Test Suite

Create persistent fixtures:

```text
tests/plugin-api/v1/
```

Include:

```text
hello
django-deployer
docker-manager
```

Every future Runory change should validate:

```text
parse
install
plan
execute mock
```

against API 1 fixtures.

This prevents accidental API breakage.

---

# 27. CI Requirements

Add CI steps:

```text
cargo check --lib
cargo test plugin
frontend tsc
Plugin API fixture validation
CLI build
CLI validate examples
CLI pack examples
signature verification tests
```

Marketplace server CI:

```text
API tests
package validation tests
signature tests
immutable-version tests
revocation tests
```

---

# 28. Release Strategy

Before opening third-party publishing:

```text
Stage A
Official plugins only

Stage B
Invite-only verified publishers

Stage C
Public publisher registration
```

Do not begin with unrestricted public publishing.

This gives time to validate:

```text
moderation
revocation
signing
security response
API stability
```

---

# 29. Official Plugins at Platform 1.0

Recommended minimum official catalog:

```text
Runory Hello
Django Deployment
Docker Manager
Nginx Manager
```

Optional:

```text
MySQL Backup
PostgreSQL Backup
```

Marketplace should not launch completely empty.

---

# 30. Documentation for Third-party Developers

Create a public developer manual with this sequence:

```text
1. What is a Runory Plugin?
2. Create your first plugin
3. Manifest
4. Workflow DSL
5. Permissions
6. Core Actions
7. Risk / ChangeSet
8. Secrets
9. Testing
10. Packaging
11. Signing
12. Publishing
13. Updates
14. Security guidelines
15. Compatibility rules
```

Use real examples.

Avoid requiring developers to understand Runory internals.

---

# 31. Marketplace Moderation

At minimum store:

```text
publisher
source/repository link if supplied
permissions
package hash
signature
scan status
security status
```

Possible automated publication checks:

```text
schema validation
signature validation
known dangerous permission review
package size
file count
workflow static analysis
Core Action whitelist
```

Do not claim automated review proves a plugin is safe.

---

# 32. Static Plugin Analyzer

A useful pre-publication feature:

```text
runory-plugin analyze
```

or backend equivalent.

Analyze:

```text
requested permissions
uses ssh.shell?
uses sudo?
writes /etc?
deletes files?
uses database.migration?
uses network access?
contains R4 actions?
```

Output example:

```text
Plugin Risk Summary

Remote execution:
Yes

Sudo:
Yes

Remote deletion:
No

Database mutation:
Yes

Highest operation risk:
R4
```

This can later feed Marketplace UI.

---

# 33. API Evolution After 1.0

Future additive changes can include:

```text
new Core Actions
new optional manifest fields
new optional contribution points
```

Breaking changes require:

```text
Plugin API 2
Workflow DSL runory/v2
```

Runory should ideally support API 1 and API 2 side-by-side for a transition period.

---

# 34. PP10 — Platform 1.0 Release Gate

Before declaring Plugin Platform 1.0 complete:

## API

```text
[ ] Plugin API 1 documented
[ ] schemas frozen
[ ] compatibility tests stable
```

## Plugins

```text
[ ] hello works
[ ] Django works
[ ] Docker works
```

## SDK

```text
[ ] SDK available
[ ] CLI init works
[ ] validate works
[ ] pack works
[ ] sign works
```

## Security

```text
[ ] package signature verified
[ ] tamper detection works
[ ] publisher keys work
[ ] revocation works
[ ] unsigned policy works
```

## Marketplace

```text
[ ] discover works
[ ] plugin details work
[ ] install works
[ ] update works
[ ] permission diff works
[ ] uninstall works
```

## Agent

```text
[ ] plugin Agent Tools register dynamically
[ ] Policy/Approval remains enforced
```

## Supply Chain

```text
[ ] published versions immutable
[ ] hashes immutable
[ ] revoked versions blocked
```

## Build

```text
[ ] cargo check --lib
[ ] cargo test
[ ] frontend tsc
[ ] CLI CI
[ ] Marketplace CI
```

---

# 35. Recommended Codex Execution Order

Give Codex these work packages sequentially.

## Work Package 1

```text
Plugin API Freeze + JSON Schemas
```

Do not change runtime architecture unless needed for compatibility bugs.

## Work Package 2

```text
runory.docker-manager
```

Use it to discover API gaps.

## Work Package 3

```text
runory-plugin CLI
init / validate / pack / inspect
```

## Work Package 4

```text
Ed25519 signing
keygen / sign / verify
```

## Work Package 5

```text
Marketplace Backend
```

## Work Package 6

```text
Marketplace Desktop UI
```

## Work Package 7

```text
Update / Permission Diff / Version Rollback
```

## Work Package 8

```text
Revocation
```

## Work Package 9

```text
Agent Tool dynamic integration
```

## Work Package 10

```text
Platform 1.0 architecture review and freeze
```

---

# 36. Initial Codex Prompt

Use this prompt for the next development cycle:

```text
Continue development of Runory as Plugin Platform 1.0.

Current state:
- Plugin System P1–P5 is complete.
- runory.hello works.
- runory.django-deployer is complete.
- plugins.db is the canonical plugin state store.
- Plugin Workflow Engine, Permission, ChangeSet, Policy, Audit and BastionProvider
  integration already exist.

Do not redesign the plugin runtime.

Implement PP1 first: Plugin API 1.0 Freeze.

Tasks:

1. Inspect current Plugin Manifest, Workflow AST, Permission capabilities,
   Core Actions, Contribution Points, Risk model, Secret references,
   lifecycle and Agent Tool definitions.

2. Produce canonical Plugin API 1.0 contracts matching the implementation.

3. Create versioned schemas:
   plugin-sdk/schemas/v1/manifest.schema.json
   plugin-sdk/schemas/v1/workflow.schema.json
   plugin-sdk/schemas/v1/input.schema.json

4. Add compatibility tests using:
   runory.hello
   runory.django-deployer

5. Create API reference documentation under:
   docs/plugin-api/v1/

6. Do not add new DSL language features unless required to accurately describe
   already-shipped behavior.

7. Do not introduce breaking changes to existing plugins.

After PP1 passes, implement PP2:
   runory.docker-manager

The Docker plugin should validate Plugin API 1.0 using:
- docker version
- docker ps
- docker inspect
- docker logs
- docker start
- docker stop
- docker restart
- docker pull

Prefer existing generic `ssh.command` with structured arguments instead of
adding Docker-specific Core Actions.

Security:
- no raw shell interpolation
- no credential exposure
- risk remains Core-owned
- R2/R3 actions go through ChangeSet / Policy / Approval
- Audit must record plugin and target operation

At the end of each work package:
- run cargo check --lib
- run relevant cargo tests
- run frontend TypeScript check
- report files changed
- report tests added
- report API gaps discovered
- do not refactor unrelated code

Do not implement Marketplace until PP1–PP4 are complete.
```

---

# 37. Long-term Product Architecture

The target architecture is:

```text
                     Runory
                       │
        ┌──────────────┼──────────────┐
        │              │              │
       Core          Agent       Marketplace
        │              │              │
        └────── Plugin Runtime ────────┘
                       │
        ┌──────────────┼───────────────┐
        │              │               │
     Django          Docker          Database
     Plugin          Plugin           Plugins
        │              │               │
        └──────── Generic Actions ──────┘
                       │
             SSH / SFTP / Bastion
                       │
                   Servers
```

The long-term product distinction is:

```text
Runory Core provides safe operational primitives.

Plugins provide domain capabilities.

Agent composes those capabilities.

Marketplace distributes them.
```

That separation should remain the guiding rule for all future Runory development.
