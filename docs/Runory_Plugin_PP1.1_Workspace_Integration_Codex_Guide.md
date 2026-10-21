# Runory Plugin Platform PP1.1 — Workspace Integration
## Codex Development Implementation Guide

> Version: 1.0  
> Scope: Plugin Logo + Workspace Category + Workspace Entry Integration  
> Target: Runory Desktop / Plugin Platform 1.0  
> Audience: Codex / Runory Developers  
> Status: Ready for implementation

---

# 1. Objective

Runory currently has five top-level workspace areas:

```text
终端
文件
仪表盘
运维
部署
```

PP1.1 adds:

1. Plugin-defined logo
2. Plugin-defined workspace category
3. `workspaceEntries`, used to expose plugin actions in the selected workspace

The top-level navigation remains fixed and owned by Runory. Plugins may only contribute entries inside these five workspaces.

Final model:

```text
Plugin
├── Identity
│   ├── name
│   ├── logo
│   └── publisher
├── Workspace
│   ├── category
│   └── workspaceEntries
├── Commands
├── Workflows
└── Agent Tools
```

---

# 2. API Changes

Extend the plugin manifest:

```json
{
  "logo": "assets/logo.png",
  "category": "deployment"
}
```

Supported `category` values:

```text
terminal
files
dashboard
operations
deployment
```

Both fields are optional for backward compatibility.

Extend `contributes`:

```json
{
  "workspaceEntries": [
    {
      "id": "django.main",
      "title": "Django",
      "description": "部署和管理 Django 项目",
      "command": "django.deploy",
      "actions": [
        "django.inspect",
        "django.deploy",
        "django.update",
        "django.rollback",
        "django.logs"
      ],
      "order": 100
    }
  ]
}
```

---

# 3. Rust Models

Add:

```rust
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    Serialize,
    Deserialize
)]
#[serde(rename_all = "snake_case")]
pub enum PluginWorkspaceCategory {
    Terminal,
    Files,
    Dashboard,
    Operations,
    Deployment,
}
```

Extend `PluginManifest`:

```rust
pub struct PluginManifest {
    // existing fields...

    pub logo: Option<String>,

    pub category: Option<PluginWorkspaceCategory>,

    #[serde(default)]
    pub contributes: PluginContributions,
}
```

Add:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceEntryContribution {
    pub id: String,
    pub title: String,
    pub description: Option<String>,
    pub command: String,
    #[serde(default)]
    pub actions: Vec<String>,
    pub order: Option<i32>,
}
```

Extend:

```rust
pub struct PluginContributions {
    // existing...
    #[serde(default)]
    pub workspace_entries: Vec<WorkspaceEntryContribution>,
}
```

---

# 4. Validation Rules

Validate during manifest/package validation.

## Category

Only allow:

```text
terminal
files
dashboard
operations
deployment
```

Unknown values fail validation.

## Workspace Entry

Rules:

```text
entry id unique inside plugin
title non-empty
command must reference an existing command contribution
every actions[] item must reference an existing command contribution
order must be a valid integer
```

Recommended entry ID regex:

```regex
^[A-Za-z0-9][A-Za-z0-9._-]{0,63}$
```

Example failure:

```text
PLUGIN_MANIFEST_INVALID
workspaceEntries[0].command references unknown command "django.foo"
```

If a plugin defines workspace entries but no category, keep the manifest valid but do not expose those entries in any workspace.

Do not guess the category.

---

# 5. Logo Rules

PP1.1 supports:

```text
PNG
WebP
```

Do not support SVG yet.

Reasons include:

```text
script
foreignObject
external references
parser differences
```

Requirements:

```text
relative path only
must stay inside plugin root
file must exist
allowed extension only
max size 512 KB
remote URLs forbidden
absolute filesystem paths forbidden
```

Reject:

```text
../logo.png
C:\logo.png
/etc/passwd
file:///tmp/logo.png
https://example.com/logo.png
```

Recommended source image:

```text
256 × 256 or larger
1:1 aspect ratio
transparent background allowed
```

Runory should render the same image at common sizes such as:

```text
24 × 24
32 × 32
48 × 48
64 × 64
```

---

# 6. Plugin Asset Resolver

Do not expose plugin install paths directly to React.

Add or reuse a Core-owned asset resolver:

```rust
pub struct PluginAssetResolver {
    registry: Arc<PluginRegistryService>,
}
```

Suggested method:

```rust
pub async fn resolve_asset(
    &self,
    plugin_id: &str,
    asset_path: &str,
) -> Result<ResolvedPluginAsset>;
```

Suggested result:

```rust
pub struct ResolvedPluginAsset {
    pub mime_type: String,
    pub bytes: Vec<u8>,
}
```

Use the existing package path-safety logic.

The frontend may receive a safe asset URL/data representation, but never an arbitrary local path.

---

# 7. Workspace Registry

Do not let Terminal, Files, Dashboard, Operations and Deployment parse plugin manifests independently.

Introduce a single normalized registry.

Frontend type:

```ts
export type PluginWorkspaceCategory =
  | "terminal"
  | "files"
  | "dashboard"
  | "operations"
  | "deployment";

export interface PluginWorkspaceEntry {
  pluginId: string;
  pluginVersion: string;

  entryId: string;

  title: string;
  description?: string;

  logoUrl?: string;

  category: PluginWorkspaceCategory;

  command: string;
  actions: string[];

  order?: number;

  publisherId: string;
}
```

Add a Tauri query similar to:

```rust
#[tauri::command]
pub async fn plugin_workspace_entries(
    category: Option<PluginWorkspaceCategory>,
    state: State<'_, AppState>,
) -> Result<Vec<PluginWorkspaceEntryDto>, ApiError>;
```

Return only entries from plugins that are:

```text
enabled
active
compatible
non-revoked
valid
```

Sorting:

```text
order ascending
then title
then plugin ID
```

If `order` is absent, use a default such as `1000`.

---

# 8. Workspace Placement

## Terminal

Category:

```text
terminal
```

Initial PP1.1 placement can be a generic plugin-tools section or secondary toolbar entry.

Future specialized contribution types may be added later.

## Files

Category:

```text
files
```

Initial placement:

```text
File Tools
```

Do not add file-context-menu contribution types in PP1.1 unless they already exist.

## Dashboard

Category:

```text
dashboard
```

Initial placement:

```text
Dashboard Tools
```

Do not implement arbitrary dashboard widgets in this task.

## Operations

Category:

```text
operations
```

Render as a plugin card grid.

Typical plugins:

```text
Docker
Nginx
MySQL
Redis
Backup
Security Audit
```

## Deployment

Category:

```text
deployment
```

Render as a deployment card grid.

Typical plugins:

```text
Django
Node.js
Docker App
Static Site
```

---

# 9. Generic UI Components

Create reusable components:

```text
PluginLogo
PluginWorkspaceCard
PluginWorkspaceGrid
PluginWorkspaceActionMenu
PluginWorkspaceSection
```

Suggested card:

```text
┌──────────────────────────┐
│ [Logo] Django            │
│                          │
│ 部署和管理 Django 项目    │
│                          │
│ [部署]               [⋯] │
└──────────────────────────┘
```

Primary button executes:

```text
entry.command
```

through the existing plugin command/workflow execution path.

The overflow menu exposes:

```text
entry.actions
```

Recommended UI behavior:

```text
if primary command also exists in actions[]
hide the duplicate from overflow menu
```

---

# 10. Frontend Structure

Suggested:

```text
src/features/plugins/workspace/

├── types.ts
├── api.ts
├── hooks.ts
├── PluginLogo.tsx
├── PluginWorkspaceCard.tsx
├── PluginWorkspaceGrid.tsx
├── PluginWorkspaceActionMenu.tsx
└── PluginWorkspaceSection.tsx
```

Hook:

```ts
usePluginWorkspaceEntries(category)
```

Example:

```ts
const entries = usePluginWorkspaceEntries("deployment");
```

Integrate into existing pages rather than replacing them.

Example:

```tsx
<DeploymentPage>
  <NativeDeploymentContent />

  <PluginWorkspaceSection
    category="deployment"
    title="部署工具"
  />
</DeploymentPage>
```

Repeat appropriately for all five workspaces.

---

# 11. Installed Plugin UI

Settings → Plugins should also show:

```text
Logo
Workspace Category
```

User-facing labels:

```text
terminal    → 终端
files       → 文件
dashboard   → 仪表盘
operations  → 运维
deployment  → 部署
```

If no logo:

```text
use generic plugin icon
```

If no category:

```text
未分类
```

---

# 12. Database

Continue using:

```text
plugins.db
```

Do not add another plugin state database.

Preferred architecture:

```text
Manifest is source of truth.
```

If the existing plugin registry already caches manifest metadata, it is acceptable to cache:

```text
category
logo_path
```

but cached values must always be synchronized from the active manifest.

Do not allow DB values to become independent configuration.

---

# 13. JSON Schema

Update canonical:

```text
plugin-sdk/schemas/v1/manifest.schema.json
```

Add:

```json
"logo": {
  "type": "string"
}
```

Add:

```json
"category": {
  "type": "string",
  "enum": [
    "terminal",
    "files",
    "dashboard",
    "operations",
    "deployment"
  ]
}
```

Add `workspaceEntries` schema.

Example:

```json
{
  "type": "object",
  "required": [
    "id",
    "title",
    "command"
  ],
  "properties": {
    "id": {
      "type": "string"
    },
    "title": {
      "type": "string"
    },
    "description": {
      "type": "string"
    },
    "command": {
      "type": "string"
    },
    "actions": {
      "type": "array",
      "items": {
        "type": "string"
      },
      "default": []
    },
    "order": {
      "type": "integer"
    }
  },
  "additionalProperties": false
}
```

---

# 14. CLI Changes

Update `runory-plugin`.

## `init`

Add prompts:

```text
Workspace category:
terminal / files / dashboard / operations / deployment / none

Logo path:
assets/logo.png
```

Generate a default workspace entry if appropriate.

## `validate`

Add checks:

```text
logo path
logo extension
logo size
category enum
workspace command references
workspace action references
duplicate workspace entry IDs
```

## `inspect`

Display:

```text
Logo
Workspace Category
Workspace Entries
```

## `pack`

Ensure logo is packaged and validation runs before archive creation.

---

# 15. Official Plugin Updates

## Django

Manifest additions:

```json
{
  "logo": "assets/logo.png",
  "category": "deployment"
}
```

Workspace entry:

```json
{
  "id": "django.main",
  "title": "Django",
  "description": "部署和管理 Django 项目",
  "command": "django.deploy",
  "actions": [
    "django.inspect",
    "django.deploy",
    "django.update",
    "django.rollback",
    "django.logs"
  ],
  "order": 100
}
```

## Docker

Manifest additions:

```json
{
  "logo": "assets/logo.png",
  "category": "operations"
}
```

Workspace entry:

```json
{
  "id": "docker.main",
  "title": "Docker",
  "description": "管理 Docker 容器和镜像",
  "command": "docker.list",
  "actions": [
    "docker.list",
    "docker.inspect",
    "docker.logs",
    "docker.restart",
    "docker.pull"
  ],
  "order": 100
}
```

---

# 16. Backward Compatibility

Existing plugins without the new fields must remain valid.

Example:

```json
{
  "id": "runory.hello",
  "name": "Runory Hello"
}
```

Expected behavior:

```text
installs normally
host-menu contributions still work
commands still work
Agent Tools still work
no workspace card appears
```

This is an additive Plugin API 1 change.

---

# 17. Runtime Refresh

Workspace entries must refresh on:

```text
plugin install
plugin update
enable
disable
uninstall
revocation
active version switch
```

No application restart should be required if the current plugin lifecycle already supports dynamic contribution refresh.

Example:

```text
Docker 1.0 category = operations
Docker 1.1 category = dashboard
```

After update:

```text
Docker disappears from 运维
Docker appears in 仪表盘
```

---

# 18. Security Tests

Add tests for:

```text
logo path traversal
absolute logo path
remote logo URL
file URL
unsupported extension
oversized logo
missing logo file
invalid category
duplicate workspace entry ID
unknown primary command
unknown additional action
disabled plugin hidden
revoked plugin hidden
```

Also verify:

```text
workspace integration does not bypass Permission
workspace integration does not bypass ChangeSet
workspace integration does not bypass Policy
workspace integration does not alter Agent Tool semantics
```

---

# 19. UI Tests

Add tests for:

```text
deployment plugin appears in 部署
operations plugin appears in 运维
category isolation works
logo renders
fallback logo renders
title/description render
primary command executes
overflow menu works
sorting works
disable removes entry
uninstall removes entry
update refreshes title/logo/category/actions
```

---

# 20. Performance

Do not re-read plugin manifests on every React render.

Use:

```text
Plugin Registry
→ normalized workspace entries
→ frontend query/cache
```

Refresh cache only on plugin lifecycle/security events.

---

# 21. Explicitly Out of Scope

Do not implement in PP1.1:

```text
new top-level tabs
Marketplace categories
dashboard widgets
file context-menu contributions
terminal tab creation
plugin i18n
remote logos
SVG logos
custom plugin React UI
new Agent semantics
```

These can be separate future work packages.

---

# 22. Work Packages

## W1 — Manifest Model

Implement:

```text
logo
category
workspaceEntries
```

## W2 — Validator

Implement:

```text
logo validation
category validation
workspace entry validation
command/action reference validation
```

## W3 — Schema + CLI

Update:

```text
manifest.schema.json
runory-plugin init
validate
inspect
pack
```

## W4 — Asset Resolver

Implement safe packaged logo resolution.

## W5 — Workspace Registry

Implement:

```text
plugin_workspace_entries(category?)
```

## W6 — Frontend Components

Implement:

```text
PluginLogo
PluginWorkspaceCard
PluginWorkspaceGrid
PluginWorkspaceActionMenu
PluginWorkspaceSection
```

## W7 — Five Workspace Integrations

Integrate:

```text
terminal
files
dashboard
operations
deployment
```

## W8 — Official Plugin Fixtures

Update Django and Docker.

## W9 — Tests

Add Rust, schema, CLI, frontend and E2E tests.

---

# 23. Definition of Done

## Manifest

```text
[ ] logo supported
[ ] category supported
[ ] workspaceEntries supported
[ ] old plugins still valid
```

## Logo

```text
[ ] PNG supported
[ ] WebP supported
[ ] remote URL rejected
[ ] absolute path rejected
[ ] path traversal rejected
[ ] max 512 KB enforced
[ ] missing file rejected
[ ] fallback icon works
```

## Category

```text
[ ] terminal
[ ] files
[ ] dashboard
[ ] operations
[ ] deployment
[ ] invalid values rejected
```

## Workspace Entries

```text
[ ] primary command
[ ] additional actions
[ ] reference validation
[ ] duplicate IDs rejected
[ ] ordering
```

## Runtime

```text
[ ] enabled plugin appears
[ ] disabled plugin disappears
[ ] revoked plugin hidden
[ ] update refreshes placement
[ ] uninstall removes placement
```

## UI

```text
[ ] logo displayed
[ ] title displayed
[ ] description displayed
[ ] primary action works
[ ] additional actions work
[ ] all five workspace areas wired
```

## Compatibility

```text
[ ] runory.hello still works
[ ] Django still works
[ ] Docker still works
[ ] existing hostMenus unaffected
[ ] Agent Tools unaffected
```

## Build

```text
[ ] cargo check --lib
[ ] relevant cargo tests
[ ] frontend TypeScript check
[ ] CLI tests
```

---

# 24. Codex Prompt

Copy the following directly to Codex:

```text
Implement Runory Plugin Platform PP1.1 — Workspace Integration.

Current state:
- Plugin System V1 is complete.
- Plugin API 1 exists.
- runory.hello works.
- runory.django-deployer works.
- runory.docker-manager may already exist.
- plugins.db remains the canonical plugin state store.
- command contributions, workflows, Agent Tools, permissions, ChangeSet,
  Policy and Audit already exist.

Goal:
Add:
1. packaged plugin logo
2. workspace category
3. workspaceEntries

Manifest additive fields:

logo?: string

category?:
  terminal
  files
  dashboard
  operations
  deployment

contributes.workspaceEntries[]:
  id
  title
  description?
  command
  actions[]
  order?

Requirements:

1. Keep top-level navigation fixed:
   terminal / files / dashboard / operations / deployment

2. Plugins add entries inside those areas.
   Plugins cannot create top-level tabs.

3. Logo:
   - package-local only
   - PNG/WebP only
   - max 512 KB
   - relative path only
   - reject absolute path
   - reject ../ traversal
   - reject http/https/file URLs
   - reuse existing plugin package path safety
   - do not expose arbitrary local filesystem paths to React

4. Category:
   - strict enum
   - optional for compatibility
   - missing category means no workspace placement
   - do not infer category

5. Workspace entries:
   - unique IDs
   - primary command must reference existing contributed command
   - each actions[] item must reference existing contributed command
   - invalid references fail manifest validation

6. Add one normalized Plugin Workspace Registry.
   Do not let each page parse plugin manifests.

7. Expose a query/Tauri command equivalent to:
   plugin_workspace_entries(category?)

8. Only return entries from enabled, active, compatible, non-revoked plugins.

9. Add reusable React components:
   PluginLogo
   PluginWorkspaceCard
   PluginWorkspaceGrid
   PluginWorkspaceActionMenu
   PluginWorkspaceSection

10. Primary action must execute entry.command through the existing plugin
    command/workflow execution pipeline.

11. Additional actions appear in overflow menu.

12. Integrate all five existing Runory workspace areas without replacing
    existing native content.

13. Update Plugin API 1 canonical JSON Schema.

14. Update runory-plugin CLI:
    init
    validate
    inspect
    pack

15. Update official fixtures:
    Django → category deployment
    Docker → category operations

16. Existing plugins without these new fields must remain valid.

17. Workspace entries are UI contributions only.
    Do not change Agent Tool semantics.

18. Continue using plugins.db.
    Do not add another plugin database.

19. Do not implement Marketplace categories in PP1.1.

20. Do not support SVG logos yet.

Required tests:
- logo traversal
- absolute logo path
- remote logo URL
- unsupported extension
- oversized logo
- missing logo
- invalid category
- unknown workspace command
- unknown workspace action
- duplicate workspace entry
- disabled plugin hidden
- revoked plugin hidden
- correct category placement
- primary action execution
- overflow actions
- update refresh
- uninstall cleanup

After implementation:
- run cargo check --lib
- run relevant cargo tests
- run frontend TypeScript check
- run CLI tests
- report files changed
- report migrations if any
- report tests added
- report Plugin API compatibility concerns

Do not refactor unrelated Runory code.
```

---

# 25. Target UX

After implementation, Runory keeps:

```text
终端   文件   仪表盘   运维   部署
```

Inside `部署`:

```text
部署工具

┌────────────────────────┐
│ [Django Logo] Django   │
│ 部署和管理 Django 项目  │
│ [部署]             [⋯] │
└────────────────────────┘
```

Inside `运维`:

```text
运维工具

┌────────────────────────┐
│ [Docker Logo] Docker   │
│ 管理 Docker 容器和镜像  │
│ [查看]             [⋯] │
└────────────────────────┘
```

This preserves Runory-owned product navigation while allowing plugins to become
first-class capabilities inside the relevant workspace.
