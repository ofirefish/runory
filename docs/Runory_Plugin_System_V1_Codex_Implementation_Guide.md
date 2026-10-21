# Runory Plugin System V1 — Codex Development Implementation Guide

> Version: 1.0  
> Target: Runory Desktop  
> Stack: Tauri 2 + Rust + React + TypeScript + SQLite  
> Scope: Phase P1–P5  
> Goal: Build the minimum secure plugin foundation before Django Deployment Plugin.

---

# 1. Development Goal

This document converts the architectural design of the Runory Plugin System into an implementation plan suitable for Codex.

The first implementation milestone is **not** the plugin marketplace and **not** Django Deployment.

The first milestone is:

```text
Local .runory-plugin
        ↓
Plugin Manager
        ↓
Manifest Validation
        ↓
Permission Approval
        ↓
Workflow Parsing
        ↓
Plan / ChangeSet
        ↓
Policy
        ↓
Approval
        ↓
Execution through Runory Core
        ↓
Audit
```

The initial end-to-end test plugin is:

```text
runory.hello
```

It must be able to:

1. install from a local `.runory-plugin` file;
2. request `host.read` and `ssh.exec`;
3. register one host-menu command;
4. execute a read-only workflow;
5. run `uname -a` on the selected host;
6. render execution output;
7. produce an audit record;
8. never receive SSH credentials.

After this works, implement:

```text
runory.nginx-manager
```

Then use those foundations to implement:

```text
runory.django-deployer
```

---

# 2. Non-negotiable Security Boundaries

Codex must preserve these boundaries during all implementation phases.

## 2.1 Plugin is untrusted

Never assume that an installed plugin is trusted.

A plugin may:

- contain malformed manifests;
- request excessive permissions;
- attempt command injection;
- try to access secrets;
- try to overwrite arbitrary files;
- try to bypass the ChangeSet layer.

Every plugin call must therefore pass through Runory-owned validation.

---

## 2.2 Plugin must not receive credentials

Never expose:

```text
SSH password
SSH private key bytes
JumpServer secret
Teleport token
Boundary credential
Secret plaintext
```

Plugin workflow input may only contain secret references:

```text
secret://...
```

The executor resolves the secret as late as possible.

---

## 2.3 Plugin must not create its own SSH connection

All remote execution must use the existing Runory connection stack:

```text
Plugin Workflow
  ↓
Runory Executor
  ↓
Connection Service
  ↓
BastionProvider
  ↓
Target Host
```

No plugin module may instantiate a raw SSH connection.

---

## 2.4 Planner and Executor must be separate

A workflow with mutable operations must first produce a plan.

Do not implement:

```text
parse workflow
↓
execute immediately
```

Implement:

```text
parse
↓
validate
↓
plan
↓
ChangeSet
↓
policy
↓
approval
↓
execute
```

---

# 3. Suggested Source Tree

## 3.1 Rust / Tauri

```text
src-tauri/src/

plugins/
├── mod.rs
├── api.rs
├── error.rs
├── manifest/
│   ├── mod.rs
│   ├── model.rs
│   ├── parser.rs
│   ├── validator.rs
│   └── compatibility.rs
├── registry/
│   ├── mod.rs
│   ├── model.rs
│   ├── repository.rs
│   └── service.rs
├── installer/
│   ├── mod.rs
│   ├── package.rs
│   ├── unpack.rs
│   ├── validator.rs
│   └── service.rs
├── permissions/
│   ├── mod.rs
│   ├── capability.rs
│   ├── grant.rs
│   ├── policy.rs
│   └── service.rs
├── contributions/
│   ├── mod.rs
│   ├── command.rs
│   ├── host_menu.rs
│   ├── workflow.rs
│   └── registry.rs
├── workflow/
│   ├── mod.rs
│   ├── ast.rs
│   ├── parser.rs
│   ├── validator.rs
│   ├── context.rs
│   ├── expressions.rs
│   ├── planner.rs
│   ├── executor.rs
│   ├── state.rs
│   └── actions/
│       ├── mod.rs
│       ├── system.rs
│       ├── ssh.rs
│       ├── remote_fs.rs
│       └── template.rs
├── changeset/
│   ├── mod.rs
│   ├── model.rs
│   ├── builder.rs
│   └── integration.rs
└── audit/
    ├── mod.rs
    └── integration.rs
```

Do not put all plugin logic in one large `manager.rs`.

---

## 3.2 React

```text
src/features/plugins/

api/
├── pluginApi.ts
└── workflowApi.ts

components/
├── PluginCard.tsx
├── PluginPermissionList.tsx
├── PluginInstallDialog.tsx
├── PluginDetail.tsx
├── PluginCommandLauncher.tsx
├── WorkflowForm.tsx
├── WorkflowPlan.tsx
├── WorkflowExecution.tsx
└── WorkflowStepStatus.tsx

pages/
├── PluginMarketplacePage.tsx
├── InstalledPluginsPage.tsx
└── PluginDetailPage.tsx

store/
├── pluginStore.ts
└── workflowStore.ts

types/
├── plugin.ts
├── permission.ts
├── workflow.ts
└── changeset.ts
```

`PluginMarketplacePage` may remain a placeholder in V1.

---

# 4. Phase P1 — Plugin Core

---

# 4.1 Rust Manifest Model

Create:

```rust
// plugins/manifest/model.rs

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginManifest {
    pub schema_version: u32,
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: Option<String>,
    pub publisher: PluginPublisher,
    pub api_version: String,
    pub min_runory_version: Option<String>,
    pub max_runory_version: Option<String>,
    pub icon: Option<String>,
    #[serde(default)]
    pub permissions: Vec<PermissionRequest>,
    #[serde(default)]
    pub contributes: PluginContributions,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginPublisher {
    pub id: String,
    pub name: String,
}
```

---

# 4.2 Plugin Contributions

```rust
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginContributions {
    #[serde(default)]
    pub commands: Vec<CommandContribution>,

    #[serde(default)]
    pub host_menus: Vec<HostMenuContribution>,

    #[serde(default)]
    pub workflows: Vec<WorkflowContribution>,

    #[serde(default)]
    pub agent_tools: Vec<AgentToolContribution>,
}
```

---

# 4.3 Command Contribution

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandContribution {
    pub id: String,
    pub title: String,
    pub category: Option<String>,
    pub icon: Option<String>,
}
```

---

# 4.4 Host Menu Contribution

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostMenuContribution {
    pub command: String,
    pub group: Option<String>,
    pub when: Option<String>,
}
```

V1 `when` must support only a small safe subset.

Recommended initial expressions:

```text
host.os == "linux"
host.os == "windows"
host.arch == "x86_64"
host.tags contains "production"
```

Do not execute arbitrary JavaScript expressions.

---

# 4.5 Workflow Contribution

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowContribution {
    pub id: String,
    pub file: String,
    pub input_schema: Option<String>,
}
```

---

# 4.6 Agent Tool Contribution

The model may be defined now, but actual Agent registration can be deferred until P8.

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentToolContribution {
    pub name: String,
    pub description: String,
    pub workflow: String,
    pub risk: RiskLevel,
    #[serde(default)]
    pub requires_approval: bool,
}
```

---

# 5. Manifest Validation

Implement strict validation before installation.

Minimum rules:

```text
schemaVersion == 1
id matches reverse-domain-like pattern
version is valid semver
publisher.id is non-empty
apiVersion is supported
workflow paths stay inside plugin package
icon path stays inside plugin package
duplicate command IDs rejected
duplicate workflow IDs rejected
permissions must be known capabilities
```

Recommended plugin ID regex:

```regex
^[a-z0-9][a-z0-9.-]{2,127}$
```

Reject:

```text
../evil
C:\evil
/runory/plugin
runory plugin
```

---

# 6. Package Safety

`.runory-plugin` is ZIP.

During unpack:

1. reject absolute paths;
2. reject `../`;
3. reject symlinks in V1;
4. reject hard links;
5. enforce total extracted size limit;
6. enforce per-file size limit;
7. reject excessive file count.

Suggested defaults:

```text
Max package size: 50 MB
Max extracted size: 200 MB
Max single file: 20 MB
Max files: 2,000
```

These values should be constants and configurable later.

---

# 7. Local Plugin Directory

Add a helper:

```rust
pub fn plugin_root_dir() -> Result<PathBuf>
```

Use platform application data directories.

Store:

```text
plugins/
  runory.hello/
    1.0.0/
      manifest.json
      workflows/
      ...
    current.json
```

`current.json`:

```json
{
  "version": "1.0.0"
}
```

Do not overwrite an existing installed version blindly.

---

# 8. SQLite Schema

Create migrations.

## 8.1 plugins

```sql
CREATE TABLE plugins (
    id TEXT NOT NULL,
    version TEXT NOT NULL,
    name TEXT NOT NULL,
    publisher_id TEXT NOT NULL,
    install_path TEXT NOT NULL,
    enabled INTEGER NOT NULL DEFAULT 1,
    installed_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    PRIMARY KEY (id, version)
);
```

---

## 8.2 plugin_active_versions

```sql
CREATE TABLE plugin_active_versions (
    plugin_id TEXT PRIMARY KEY,
    version TEXT NOT NULL,
    FOREIGN KEY (plugin_id, version)
        REFERENCES plugins(id, version)
);
```

---

## 8.3 plugin_permissions

```sql
CREATE TABLE plugin_permissions (
    plugin_id TEXT NOT NULL,
    permission TEXT NOT NULL,
    granted INTEGER NOT NULL,
    granted_at INTEGER,
    PRIMARY KEY (plugin_id, permission)
);
```

---

## 8.4 plugin_execution_runs

```sql
CREATE TABLE plugin_execution_runs (
    id TEXT PRIMARY KEY,
    plugin_id TEXT NOT NULL,
    plugin_version TEXT NOT NULL,
    workflow_id TEXT NOT NULL,
    host_id TEXT,
    state TEXT NOT NULL,
    started_at INTEGER NOT NULL,
    completed_at INTEGER,
    error_code TEXT,
    error_message TEXT
);
```

---

## 8.5 plugin_execution_steps

```sql
CREATE TABLE plugin_execution_steps (
    id TEXT PRIMARY KEY,
    run_id TEXT NOT NULL,
    step_id TEXT NOT NULL,
    action TEXT NOT NULL,
    state TEXT NOT NULL,
    risk TEXT NOT NULL,
    started_at INTEGER,
    completed_at INTEGER,
    exit_code INTEGER,
    FOREIGN KEY (run_id)
        REFERENCES plugin_execution_runs(id)
);
```

Never store secret values in these tables.

---

# 9. Plugin Registry API

Define a service:

```rust
pub struct PluginRegistryService {
    repository: PluginRegistryRepository,
}
```

Methods:

```rust
pub async fn list_installed(&self) -> Result<Vec<InstalledPlugin>>;

pub async fn get_plugin(
    &self,
    plugin_id: &str
) -> Result<Option<InstalledPlugin>>;

pub async fn set_enabled(
    &self,
    plugin_id: &str,
    enabled: bool
) -> Result<()>;

pub async fn uninstall(
    &self,
    plugin_id: &str
) -> Result<()>;
```

---

# 10. Tauri Commands — P1

Expose only DTOs, not Rust internal objects.

```rust
#[tauri::command]
pub async fn plugin_list_installed(...) -> Result<Vec<PluginDto>, ApiError>;
```

```rust
#[tauri::command]
pub async fn plugin_inspect_package(
    path: String,
    ...
) -> Result<PluginPackageInspectionDto, ApiError>;
```

```rust
#[tauri::command]
pub async fn plugin_install(
    request: PluginInstallRequest,
    ...
) -> Result<PluginDto, ApiError>;
```

```rust
#[tauri::command]
pub async fn plugin_set_enabled(
    plugin_id: String,
    enabled: bool,
    ...
) -> Result<(), ApiError>;
```

```rust
#[tauri::command]
pub async fn plugin_uninstall(
    plugin_id: String,
    ...
) -> Result<(), ApiError>;
```

---

# 11. Install UX

Install must be a two-stage flow.

Do NOT:

```text
select package
↓
install immediately
```

Do:

```text
select package
↓
inspect package
↓
show:
  plugin identity
  publisher
  version
  requested permissions
  compatibility
↓
user confirms
↓
install
```

This means:

```text
plugin_inspect_package()
```

must not modify application state.

---

# 12. Phase P2 — Permission Model

---

# 12.1 Capability Enum

```rust
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Hash,
    Serialize,
    Deserialize
)]
#[serde(rename_all = "snake_case")]
pub enum PluginCapability {
    HostRead,
    HostMetadataRead,

    SshExec,
    SshExecSudo,
    SshShell,

    RemoteFsRead,
    RemoteFsWrite,
    RemoteFsUpload,
    RemoteFsDelete,

    SecretReference,

    NetworkHttps,
}
```

Do not use arbitrary strings internally after parsing.

Unknown capabilities must fail manifest validation.

---

# 12.2 Permission Request

Some permissions may need scopes.

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PermissionRequest {
    Simple(String),
    Scoped {
        permission: String,
        #[serde(default)]
        hosts: Vec<String>,
    },
}
```

Example:

```json
{
  "permission": "network.https",
  "hosts": [
    "api.github.com"
  ]
}
```

---

# 12.3 Permission Grant

```rust
pub struct PermissionGrant {
    pub plugin_id: String,
    pub capability: PluginCapability,
    pub granted: bool,
}
```

At runtime every privileged action must call:

```rust
permission_service.require(
    plugin_id,
    PluginCapability::SshExec
)?;
```

Permission checking must occur in the action executor itself.

Never rely only on UI checks.

---

# 13. Permission Mapping

Initial action mapping:

| Action | Required Capability |
|---|---|
| `system.detect` | `host.read` |
| `ssh.command` | `ssh.exec` |
| `ssh.command` with sudo | `ssh.exec.sudo` |
| `ssh.shell` | `ssh.shell` |
| `remote.fs.read` | `remote.fs.read` |
| `remote.fs.write` | `remote.fs.write` |
| `remote.fs.upload` | `remote.fs.upload` |
| `remote.fs.remove` | `remote.fs.delete` |
| Secret reference usage | `secret.reference` |

This mapping belongs to Runory Core, not the plugin.

---

# 14. Phase P3 — Workflow AST

---

# 14.1 Workflow Model

```rust
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowDefinition {
    pub api_version: String,
    pub kind: String,
    pub metadata: WorkflowMetadata,
    pub inputs: Option<serde_json::Value>,
    pub steps: Vec<WorkflowStep>,
    pub outputs: Option<serde_json::Value>,
}
```

---

# 14.2 Metadata

```rust
#[derive(Debug, Clone, Deserialize)]
pub struct WorkflowMetadata {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
}
```

---

# 14.3 Workflow Step

```rust
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowStep {
    pub id: String,

    pub uses: String,

    #[serde(default)]
    pub with: serde_json::Value,

    #[serde(rename = "if")]
    pub condition: Option<String>,

    pub timeout: Option<String>,

    pub retry: Option<RetryPolicy>,

    pub on_failure: Option<FailurePolicy>,
}
```

---

# 14.4 Retry Policy

```rust
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RetryPolicy {
    pub max_attempts: u32,
    pub delay: Option<String>,
}
```

Hard-limit maximum attempts in Core:

```text
maxAttempts <= 5
```

even if manifest says larger.

---

# 15. Workflow Execution Context

```rust
pub struct WorkflowContext {
    pub run_id: String,
    pub plugin_id: String,
    pub plugin_version: String,
    pub workflow_id: String,
    pub host_id: Option<String>,
    pub inputs: serde_json::Value,
    pub step_outputs: HashMap<String, serde_json::Value>,
}
```

Do not place secret plaintext in `WorkflowContext`.

Use opaque secret refs only.

---

# 16. Variable Resolution

Support:

```text
{{ inputs.name }}
{{ host.id }}
{{ host.os }}
{{ steps.check.outputs.version }}
```

V1 should NOT implement a full template language.

Only support:

```text
property lookup
simple equality
simple boolean checks
contains
```

No function calls.

No shell substitution.

No JavaScript.

No arbitrary code evaluation.

---

# 17. Workflow Parser

Use `serde_yaml`.

Pipeline:

```text
YAML
↓
Deserialize AST
↓
Structural Validation
↓
Semantic Validation
↓
ValidatedWorkflow
```

Semantic checks:

```text
duplicate step id
unknown action
invalid timeout
invalid retry
invalid variable reference
unknown workflow API version
empty workflow
cyclic references if future dependencies are introduced
```

---

# 18. Workflow State Machine

Define:

```rust
pub enum WorkflowRunState {
    Created,
    Planning,
    AwaitingApproval,
    Running,
    Succeeded,
    Failed,
    Cancelled,
    RollingBack,
    RolledBack,
}
```

Step state:

```rust
pub enum WorkflowStepState {
    Pending,
    Skipped,
    Planned,
    Running,
    Succeeded,
    Failed,
    Cancelled,
}
```

Persist state transitions.

---

# 19. Phase P4 — Action Interface

Define one Core-owned trait.

```rust
#[async_trait]
pub trait WorkflowAction: Send + Sync {
    fn id(&self) -> &'static str;

    fn required_capabilities(
        &self,
        input: &serde_json::Value
    ) -> Result<Vec<PluginCapability>>;

    fn risk(
        &self,
        input: &serde_json::Value
    ) -> Result<RiskLevel>;

    async fn plan(
        &self,
        ctx: &ActionContext,
        input: &serde_json::Value
    ) -> Result<ActionPlan>;

    async fn execute(
        &self,
        ctx: &ActionContext,
        input: &serde_json::Value
    ) -> Result<ActionResult>;
}
```

Important:

```text
Action implementation belongs to Runory.
Plugin only references action ID.
```

---

# 20. Action Registry

```rust
pub struct ActionRegistry {
    actions: HashMap<String, Arc<dyn WorkflowAction>>,
}
```

Register:

```text
system.detect
ssh.command
remote.fs.read
remote.fs.write
template.render
```

P1–P5 should not dynamically load executable action code from plugins.

---

# 21. ActionContext

```rust
pub struct ActionContext {
    pub run_id: String,
    pub plugin_id: String,
    pub plugin_version: String,
    pub host_id: Option<String>,

    pub permission_service: Arc<PermissionService>,
    pub connection_service: Arc<ConnectionService>,
    pub remote_fs_service: Arc<RemoteFsService>,
    pub audit_service: Arc<AuditService>,
}
```

This context is created by Core.

Plugins never receive it directly.

---

# 22. `system.detect`

Risk:

```text
R0
```

Permission:

```text
host.read
```

Output example:

```json
{
  "os": "linux",
  "distribution": "ubuntu",
  "version": "24.04",
  "arch": "x86_64"
}
```

---

# 23. `ssh.command`

Input schema:

```yaml
uses: ssh.command

with:
  executable: uname
  args:
    - -a
```

Rust model:

```rust
pub struct SshCommandInput {
    pub executable: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: HashMap<String, EnvValue>,
    #[serde(default)]
    pub privilege: PrivilegeMode,
    pub cwd: Option<String>,
}
```

---

# 24. Do Not Build Shell Strings Naively

Bad:

```rust
format!(
    "{} {}",
    executable,
    args.join(" ")
)
```

The existing SSH execution layer should receive a structured command if possible.

If the current SSH layer only supports strings, add a central shell-escaping utility and keep it inside Runory Core.

The plugin must not control quoting behavior.

---

# 25. Environment Values

```rust
pub enum EnvValue {
    Literal(String),
    SecretRef(String),
}
```

YAML example:

```yaml
env:
  DJANGO_SETTINGS_MODULE:
    value: config.settings.production

  DB_PASSWORD:
    secretRef: secret://django/prod-db
```

Secret must be resolved only immediately before remote execution.

Audit output must log:

```text
DB_PASSWORD=********
```

not the value.

---

# 26. `ssh.shell`

Do not implement until structured command works.

When implemented:

```text
permission: ssh.shell
minimum risk: R3
```

A shell action can never be R0.

Even if the plugin declares otherwise.

Risk is Core-owned.

---

# 27. `remote.fs.read`

Input:

```yaml
uses: remote.fs.read

with:
  path: /etc/os-release
```

Risk:

```text
R0
```

Permission:

```text
remote.fs.read
```

Output:

```json
{
  "content": "...",
  "size": 512
}
```

Add output-size limit.

Do not allow a plugin to read multi-gigabyte files into memory.

---

# 28. `remote.fs.write`

Input:

```yaml
uses: remote.fs.write

with:
  path: /tmp/runory-test.conf
  content: hello
  mode: "0644"
```

Risk:

```text
R2
```

Permission:

```text
remote.fs.write
```

Planner must show:

```text
Create or replace /tmp/runory-test.conf
```

Before execution.

---

# 29. Safe Remote Path Policy

Introduce a helper:

```rust
RemotePathPolicy
```

At minimum validate:

```text
NUL rejected
empty path rejected
path length limit
```

Do not globally ban `/etc` because legitimate deployment plugins need it.

Instead use:

```text
permission
+
risk
+
ChangeSet
+
Policy
```

to control dangerous paths.

---

# 30. `template.render`

This action renders a file from the plugin package, but does not execute code.

Recommended template engine:

```text
Handlebars
```

Disable unsafe helpers.

Input:

```yaml
uses: template.render

with:
  template: templates/nginx.conf.hbs
  destination: /tmp/nginx.conf
  variables:
    domain: "{{ inputs.domain }}"
```

Planner treats destination write as R2.

---

# 31. Action Result

```rust
pub struct ActionResult {
    pub outputs: serde_json::Value,
    pub stdout: Option<String>,
    pub stderr: Option<String>,
    pub exit_code: Option<i32>,
}
```

Before persistence or UI emission:

```text
secret redaction
ANSI sanitization
size truncation
```

must be applied.

---

# 32. Output Limits

Suggested defaults:

```text
stdout max persisted: 1 MB
stderr max persisted: 1 MB
single step combined UI stream buffer: 5 MB
```

If larger:

```text
truncate
+
show indicator
```

Do not let a remote command exhaust desktop memory.

---

# 33. Phase P5 — Planner and ChangeSet

---

# 33.1 ActionPlan

```rust
pub struct ActionPlan {
    pub action_id: String,
    pub risk: RiskLevel,
    pub summary: String,
    pub changes: Vec<PlannedChange>,
    pub requires_approval: bool,
}
```

---

# 33.2 PlannedChange

```rust
pub enum PlannedChange {
    RemoteCommand {
        executable: String,
        args: Vec<String>,
        privilege: PrivilegeMode,
    },

    RemoteFileWrite {
        path: String,
    },

    RemoteFileDelete {
        path: String,
    },

    PackageInstall {
        packages: Vec<String>,
    },

    ServiceRestart {
        service: String,
    },

    DatabaseMigration {
        description: String,
    },
}
```

V1 only needs the variants required by current actions.

---

# 34. Risk Level Ownership

Risk must be calculated by Runory.

Plugin may add a descriptive hint, but cannot downgrade risk.

Examples:

```text
system.detect -> R0
remote.fs.read -> R0
remote.fs.write -> R2
ssh.command sudo -> at least R2/R3 depending on command classification
ssh.shell -> at least R3
database migration -> R4
```

If classification is uncertain:

```text
choose higher risk
```

---

# 35. Command Risk Classifier

For `ssh.command`, create a small Core classifier.

Examples:

```text
uname, whoami, pwd, systemctl status -> R0
mkdir in user-writable temp -> R1
systemctl reload -> R2/R3
systemctl restart -> R3
apt install -> R3
rm -> R4
userdel -> R4
mkfs -> R4
iptables/nft -> R4
```

Do not attempt an AI-based classifier in V1.

Use deterministic rules.

Unknown privileged command:

```text
R3 minimum
```

Unknown destructive-looking command:

```text
R4
```

---

# 36. Planner

```rust
pub struct WorkflowPlanner {
    action_registry: Arc<ActionRegistry>,
}
```

Algorithm:

```text
for step in workflow.steps:
  resolve references that are available at plan time
  action = registry.get(step.uses)
  validate required permission declarations
  call action.plan()
  append plan
combine into ChangeSet
run Policy evaluation
return WorkflowPlan
```

Important:

Some outputs only exist at execution time.

Therefore V1 planner must support:

```text
static plan
```

and mark dynamic values:

```text
<resolved at runtime>
```

Do not fake values.

---

# 37. ChangeSet Integration

Do not create a second parallel ChangeSet system for plugins.

Adapt plugin plans into the existing Runory ChangeSet domain model.

If the existing model lacks required operation types, extend it.

Preferred architecture:

```text
Plugin PlannedChange
        ↓ adapter
Runory ChangeSet Operation
        ↓
Existing Policy
        ↓
Existing Approval
```

---

# 38. Workflow Execution

Once approved:

```text
WorkflowExecutor.execute(plan_id)
```

Executor must verify again:

```text
plugin still installed
plugin version unchanged
plugin enabled
permissions still granted
host still accessible
ChangeSet approval still valid
```

This prevents TOCTOU-style bypass.

---

# 39. Execution Algorithm

```text
state = Running

for step:
  evaluate condition

  if false:
      mark Skipped
      continue

  resolve variables

  re-check permission

  determine action

  run action

  redact output

  persist step result

  publish UI event

  on failure:
      apply failure policy

state = Succeeded
```

---

# 40. Cancellation

Every action should accept cancellation.

Use Rust cancellation tokens.

```rust
tokio_util::sync::CancellationToken
```

User clicking Cancel:

```text
WorkflowExecution
↓
CancellationToken.cancel()
↓
current action stops if supported
↓
remaining steps cancelled
```

Remote commands need termination behavior.

At minimum:

```text
close execution channel
```

Later add remote process-group termination where supported.

---

# 41. Event Streaming to React

Do not poll step states every second if an event mechanism already exists.

Emit events:

```text
plugin://workflow-started
plugin://workflow-step-started
plugin://workflow-step-output
plugin://workflow-step-completed
plugin://workflow-completed
```

Payloads must never contain secret plaintext.

---

# 42. Tauri Workflow Commands

```rust
#[tauri::command]
pub async fn plugin_workflow_get_form(
    plugin_id: String,
    workflow_id: String,
    ...
) -> Result<WorkflowFormDto, ApiError>;
```

```rust
#[tauri::command]
pub async fn plugin_workflow_plan(
    request: WorkflowPlanRequest,
    ...
) -> Result<WorkflowPlanDto, ApiError>;
```

```rust
#[tauri::command]
pub async fn plugin_workflow_execute(
    plan_id: String,
    ...
) -> Result<WorkflowRunDto, ApiError>;
```

```rust
#[tauri::command]
pub async fn plugin_workflow_cancel(
    run_id: String,
    ...
) -> Result<(), ApiError>;
```

---

# 43. Frontend Flow

Host context menu:

```text
Host
↓
plugin contributions
↓
Deploy / Inspect / ...
```

For `runory.hello`:

```text
Host → Runory Hello
```

Then:

```text
Load Workflow Form
↓
No inputs
↓
Plan
↓
R0 plan
↓
Execute
↓
Show output
```

For R3/R4:

```text
Plan
↓
Approval Screen
↓
Apply Changes
```

---

# 44. TypeScript DTOs

```ts
export interface PluginDto {
  id: string;
  name: string;
  version: string;
  publisherId: string;
  enabled: boolean;
  permissions: PluginPermissionDto[];
}
```

```ts
export interface WorkflowPlanDto {
  id: string;
  pluginId: string;
  pluginVersion: string;
  workflowId: string;
  hostId?: string;
  risk: RiskLevel;
  requiresApproval: boolean;
  operations: PlannedOperationDto[];
}
```

```ts
export type RiskLevel =
  | "R0"
  | "R1"
  | "R2"
  | "R3"
  | "R4";
```

---

# 45. React State

Avoid a global monolithic plugin store.

Separate:

```text
plugin inventory state
workflow execution state
```

Possible Zustand stores:

```ts
usePluginStore
useWorkflowRunStore
```

Execution logs may be large.

Do not keep unlimited historical stdout in a global reactive store.

---

# 46. Minimal Test Plugin

Create repository fixture:

```text
tests/fixtures/plugins/runory.hello/
```

Contents:

```text
manifest.json
workflows/hello.yaml
```

---

# 47. `runory.hello` Manifest

```json
{
  "schemaVersion": 1,
  "id": "runory.hello",
  "name": "Runory Hello",
  "version": "1.0.0",
  "description": "Minimal Runory plugin",
  "publisher": {
    "id": "runory",
    "name": "Runory"
  },
  "apiVersion": "1",
  "minRunoryVersion": "0.8.0",

  "permissions": [
    "host.read",
    "ssh.exec"
  ],

  "contributes": {
    "commands": [
      {
        "id": "hello.inspect",
        "title": "Inspect Host",
        "category": "Runory Hello"
      }
    ],

    "hostMenus": [
      {
        "command": "hello.inspect",
        "group": "runory",
        "when": "host.os == 'linux'"
      }
    ],

    "workflows": [
      {
        "id": "hello.inspect",
        "file": "workflows/hello.yaml"
      }
    ]
  }
}
```

---

# 48. `hello.yaml`

```yaml
apiVersion: runory/v1
kind: Workflow

metadata:
  id: hello.inspect
  name: Inspect Linux Host

steps:

  - id: detect
    uses: system.detect

  - id: uname
    uses: ssh.command
    with:
      executable: uname
      args:
        - -a

  - id: whoami
    uses: ssh.command
    with:
      executable: whoami
```

Expected risk:

```text
R0
```

No approval needed beyond installed permissions.

---

# 49. End-to-End Acceptance Test

Manual test:

```text
1. Build runory.hello.runory-plugin

2. Settings → Plugins → Install local plugin

3. Select package

4. Inspection dialog shows:
   host.read
   ssh.exec

5. Confirm installation

6. Open a Linux host

7. Context menu shows:
   Inspect Host

8. Execute

9. UI displays:
   uname output
   whoami output

10. Audit contains:
    plugin ID
    version
    workflow
    host
    steps
    result

11. Search logs/database:
    no SSH password
    no SSH private key
```

---

# 50. Unit Tests — Manifest

Required cases:

```text
valid manifest
invalid schema version
invalid plugin id
invalid semver
duplicate command
duplicate workflow
unknown permission
path traversal workflow path
absolute workflow path
unsupported apiVersion
incompatible Runory version
```

---

# 51. Unit Tests — Package Installer

Required:

```text
valid ZIP
ZIP slip ../
absolute path
symlink
oversized archive
too many files
missing manifest
invalid manifest
existing version
atomic install failure
```

Installation must be atomic:

```text
unpack temporary
↓
validate all
↓
rename into final directory
↓
DB transaction
```

If anything fails:

```text
no partially installed plugin
```

---

# 52. Unit Tests — Permissions

Required:

```text
allowed action passes
missing permission denied
disabled plugin denied
permission revoked after planning denied at execution
sudo without ssh.exec.sudo denied
secret reference without permission denied
```

---

# 53. Unit Tests — Workflow

Required:

```text
valid workflow
duplicate step ID
unknown action
bad expression
invalid retry
invalid timeout
false condition skips step
step output reference
workflow failure stops execution
cancellation
```

---

# 54. Security Tests

Mandatory:

## Shell injection

Input:

```text
project name = foo; rm -rf /
```

Must remain a literal argument.

---

## Path traversal

Template:

```text
../../outside
```

must be rejected.

---

## Secret leakage

Test secret:

```text
RUNORY_TEST_SECRET_928371
```

Ensure it never appears in:

```text
stdout persistence
stderr persistence
UI events
audit
ChangeSet
workflow state
panic messages
```

---

## Permission bypass

A plugin declaring only:

```text
ssh.exec
```

must not execute:

```text
sudo
remote file write
shell
```

---

# 55. Logging

Add structured logging context:

```text
plugin_id
plugin_version
workflow_id
run_id
step_id
host_id
```

Never log raw:

```text
workflow input JSON
environment map
secret reference resolution
```

without sanitization.

---

# 56. Error Types

```rust
#[derive(thiserror::Error, Debug)]
pub enum PluginError {

    #[error("plugin package is invalid")]
    InvalidPackage,

    #[error("manifest validation failed: {0}")]
    InvalidManifest(String),

    #[error("plugin is incompatible with this Runory version")]
    IncompatibleVersion,

    #[error("permission denied: {0}")]
    PermissionDenied(String),

    #[error("workflow not found: {0}")]
    WorkflowNotFound(String),

    #[error("workflow validation failed: {0}")]
    WorkflowValidation(String),

    #[error("action not found: {0}")]
    ActionNotFound(String),

    #[error("policy denied operation")]
    PolicyDenied,

    #[error("workflow cancelled")]
    Cancelled,

    #[error("remote execution failed: {0}")]
    RemoteExecution(String),
}
```

Convert to stable frontend codes.

Do not expose internal filesystem paths unnecessarily.

---

# 57. API Error Codes

Recommended:

```text
PLUGIN_PACKAGE_INVALID
PLUGIN_MANIFEST_INVALID
PLUGIN_VERSION_INCOMPATIBLE
PLUGIN_ALREADY_INSTALLED
PLUGIN_NOT_FOUND
PLUGIN_DISABLED
PLUGIN_PERMISSION_DENIED
PLUGIN_WORKFLOW_NOT_FOUND
PLUGIN_WORKFLOW_INVALID
PLUGIN_ACTION_NOT_FOUND
PLUGIN_POLICY_DENIED
PLUGIN_EXECUTION_FAILED
PLUGIN_CANCELLED
```

---

# 58. Atomicity and Recovery

Plugin installation and execution state need crash tolerance.

Installation:

```text
temp directory
↓
validation
↓
atomic move
↓
database commit
```

On startup:

```text
scan unfinished install temp directories
remove stale temp data
```

Execution:

If app crashes during workflow:

```text
Running
```

records become:

```text
Interrupted
```

on next startup.

Do not automatically resume a mutation workflow.

Show:

```text
Previous plugin workflow was interrupted.
Review target state before retrying.
```

This aligns with Runory's existing interruption-state model.

---

# 59. Migration Strategy

Do not rewrite existing Runory execution systems.

Integrate with:

```text
existing host service
existing SSH service
existing BastionProvider
existing Policy
existing ChangeSet
existing Audit
```

Add adapters where necessary.

Avoid:

```text
PluginSSHService
PluginPolicyEngine
PluginAuditDB
```

if equivalent Core systems already exist.

The plugin layer should be a consumer of Core capabilities.

---

# 60. Codex Implementation Order

Codex should implement in this exact order.

## Step 1

Create:

```text
plugins/manifest
```

with parser + validator + unit tests.

Commit target:

```text
plugin-core-01-manifest
```

---

## Step 2

Create package inspection/unpack safety.

Commit:

```text
plugin-core-02-package
```

---

## Step 3

Create DB migration + registry repository.

Commit:

```text
plugin-core-03-registry
```

---

## Step 4

Create install/enable/disable/uninstall Tauri commands.

Commit:

```text
plugin-core-04-lifecycle
```

---

## Step 5

Create permission enum/service/runtime checks.

Commit:

```text
plugin-core-05-permissions
```

---

## Step 6

Create Workflow AST/parser/validator.

Commit:

```text
plugin-core-06-workflow-parser
```

---

## Step 7

Create ActionRegistry + `system.detect`.

Commit:

```text
plugin-core-07-actions
```

---

## Step 8

Add `ssh.command` using existing Runory SSH stack.

Commit:

```text
plugin-core-08-ssh-command
```

This is a critical architecture checkpoint.

Code review must verify:

```text
plugin never receives credentials
BastionProvider path is reused
```

---

## Step 9

Add workflow planner.

Commit:

```text
plugin-core-09-planner
```

---

## Step 10

Adapt plans into existing ChangeSet.

Commit:

```text
plugin-core-10-changeset
```

---

## Step 11

Create workflow executor + state persistence.

Commit:

```text
plugin-core-11-executor
```

---

## Step 12

Create basic Plugin UI.

Commit:

```text
plugin-ui-01-manager
```

---

## Step 13

Build and run `runory.hello`.

Commit:

```text
plugin-e2e-01-hello
```

At this point do not start Django yet.

---

# 61. Architecture Review Gate

Before P4 expands further, verify:

```text
[ ] Plugin cannot access credential plaintext
[ ] SSH uses Runory Core connection path
[ ] Permission enforcement is server-side/Rust-side
[ ] Plan and execute are separate
[ ] R2+ operations can integrate with ChangeSet
[ ] Audit records plugin identity
[ ] Workflow outputs are bounded
[ ] Cancellation works
[ ] Install is atomic
[ ] ZIP traversal is blocked
[ ] No arbitrary plugin executable code exists
```

If any item fails, fix architecture first.

---

# 62. Next Plugin: Nginx Manager

After Hello, create:

```text
runory.nginx-manager
```

Functions:

```text
Inspect nginx
Test config
View sites
Reload nginx
```

This validates:

```text
remote.fs.read
ssh.command
sudo
R3 approval
ChangeSet
```

Example:

```yaml
- id: nginx-test
  uses: ssh.command
  with:
    executable: nginx
    args:
      - -t
    privilege: sudo
```

Then:

```yaml
- id: nginx-reload
  uses: ssh.command
  with:
    executable: systemctl
    args:
      - reload
      - nginx
    privilege: sudo
```

Expected:

```text
nginx -t      → read/check oriented
reload nginx  → R2/R3
```

---

# 63. Only Then Start Django Deployer

Django requires more action types:

```text
git.clone
python.venv
python.pip.install
remote.fs.symlink
systemd.install
http.check
database.migration
```

Do not implement those until Hello + Nginx demonstrate the plugin foundation is sound.

---

# 64. Suggested Codex Prompt

Use the following as the initial implementation prompt:

```text
You are implementing Phase P1 of the Runory Plugin System.

Read the existing Runory architecture before changing code, especially:
- host management
- SSH connection execution
- BastionProvider abstraction
- Policy
- ChangeSet
- Audit
- database migrations
- Tauri command patterns
- frontend API conventions

Do not create parallel SSH, Policy, ChangeSet, or Audit subsystems if existing ones can be reused.

Implement only:
1. Plugin manifest Rust models
2. JSON manifest parser
3. strict manifest validation
4. compatibility validation
5. unit tests

Do not implement:
- plugin marketplace
- WASM
- JavaScript plugin runtime
- Django deployment
- workflow execution
- native plugins

Security requirements:
- plugin paths must not escape the plugin root
- unknown permissions must fail validation
- duplicate contribution IDs must fail validation
- plugin IDs and versions must be validated
- no plugin executable code is run during parsing

Keep new code inside a dedicated src-tauri/src/plugins module where possible.

Before coding:
1. inspect relevant existing repository structure;
2. identify reusable types/services;
3. propose the concrete files to add/change.

Then implement the smallest coherent patch and tests.

Do not refactor unrelated Runory code.
```

---

# 65. Definition of Done for P1–P5

P1–P5 are complete only when all of the following work:

```text
Local plugin package installation
Manifest validation
Plugin registry
Enable/disable/uninstall
Permission approval
Runtime permission enforcement
Workflow parser
Workflow validator
Condition handling
system.detect
ssh.command
remote.fs.read
remote.fs.write
template.render
Workflow plan
ChangeSet integration
Policy integration
Approval
Workflow execution
Cancellation
Audit
runory.hello end-to-end
```

The milestone is not "code compiles."

The milestone is:

```text
An untrusted local plugin can safely request and use Runory capabilities
without obtaining direct access to credentials or bypassing policy.
```

That is the foundation on which Django Deployment, Docker, Nginx, databases,
backups, diagnostics and Agent tools can safely be built.
