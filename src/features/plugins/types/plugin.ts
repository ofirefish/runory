export type RiskLevel = "R0" | "R1" | "R2" | "R3" | "R4";

export interface PluginPermissionDto {
  permission: string;
  granted: boolean;
  hosts: string[];
}

export interface PluginCommandDto {
  id: string;
  title: string;
  category?: string | null;
}

export interface PluginHostMenuDto {
  pluginId: string;
  command: string;
  group?: string | null;
  when?: string | null;
  title: string;
  workflowId?: string | null;
}

export interface PluginWorkflowDto {
  id: string;
  file: string;
}

export type PluginWorkspaceCategory =
  | "terminal"
  | "files"
  | "dashboard"
  | "operations"
  | "deployment";

export interface PluginDto {
  id: string;
  name: string;
  version: string;
  publisherId: string;
  enabled: boolean;
  description?: string | null;
  category?: PluginWorkspaceCategory | null;
  logo?: string | null;
  logoUrl?: string | null;
  permissions: PluginPermissionDto[];
  commands: PluginCommandDto[];
  hostMenus: PluginHostMenuDto[];
  workflows: PluginWorkflowDto[];
}

export interface PluginPackageInspection {
  reviewId: string;
  trust: "official" | "verifiedPublisher" | "communitySigned" | "unsigned";
  installedVersion: string | null;
  permissionAdditions: string[];
  manifest: {
    schemaVersion: number;
    id: string;
    name: string;
    version: string;
    description?: string | null;
    publisher: { id: string; name: string };
    apiVersion: string;
    permissions: (string | { permission: string; hosts?: string[] })[];
  };
  fileCount: number;
  compressedSize: number;
}

export interface PluginInstallRequest {
  reviewId: string;
  grantedPermissions: string[];
}

export interface MarketplacePlugin {
  installedVersion?: string | null;
  updateAvailable?: boolean;
  id: string;
  publisherId: string;
  name: string;
  description?: string | null;
  category?: string | null;
  latestVersion: string;
  apiVersion: string;
  verified: boolean;
}

export interface WorkflowPlanRequest {
  pluginId: string;
  workflowId: string;
  hostId?: string | null;
  sessionId?: string | null;
  inputs?: unknown;
}

export interface WorkflowStepPlanDto {
  stepId: string;
  actionId: string;
  plan: {
    actionId: string;
    risk: RiskLevel;
    summary: string;
    requiresApproval: boolean;
  };
  hasRuntimeValues: boolean;
}

export type PluginPrivilegeMode = "none" | "sudo";

export type PluginPlannedOperation =
  | {
      kind: "remoteCommand";
      executable: string;
      args: string[];
      privilege: PluginPrivilegeMode;
      summary: string;
    }
  | { kind: "remoteFileWrite"; path: string; summary: string }
  | { kind: "remoteFileDelete"; path: string; summary: string }
  | { kind: "other"; summary: string };

export interface PluginChangeSetSummary {
  planId: string;
  pluginId: string;
  pluginVersion: string;
  workflowId: string;
  risk: RiskLevel;
  requiresChangeset: boolean;
  requiresApproval: boolean;
  operations: PluginPlannedOperation[];
  changeSetId?: string | null;
  changeSetVersion?: number | null;
}

export interface WorkflowPlanDto {
  id: string;
  pluginId: string;
  pluginVersion: string;
  workflowId: string;
  hostId?: string | null;
  risk: RiskLevel;
  requiresApproval: boolean;
  steps: WorkflowStepPlanDto[];
  changeset?: PluginChangeSetSummary | null;
}

export interface WorkflowStepStatusDto {
  stepId: string;
  actionId: string;
  state: string;
  summary?: string | null;
  errorCode?: string | null;
  stdout?: string | null;
  stderr?: string | null;
  exitCode?: number | null;
  outputs?: unknown;
}

export interface WorkflowRunDto {
  runId: string;
  pluginId: string;
  pluginVersion: string;
  workflowId: string;
  hostId?: string | null;
  state: string;
  steps: WorkflowStepStatusDto[];
  errorCode?: string | null;
}
