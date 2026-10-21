import { invoke } from "@tauri-apps/api/core";
import type {
  PluginDto,
  MarketplacePlugin,
  PluginHostMenuDto,
  PluginInstallRequest,
  PluginPackageInspection,
  WorkflowPlanDto,
  WorkflowPlanRequest,
  WorkflowRunDto,
} from "../../features/plugins/types/plugin";

export const pluginListInstalled = () => invoke<PluginDto[]>("plugin_list_installed");

export const pluginInspectPackage = (path: string) =>
  invoke<PluginPackageInspection>("plugin_inspect_package", { path });

export const pluginInstall = (request: PluginInstallRequest) =>
  invoke<PluginDto>("plugin_install", { request });

export const pluginDiscardInstallReview = (reviewId: string) =>
  invoke<void>("plugin_discard_install_review", { reviewId });

export const pluginSetEnabled = (pluginId: string, enabled: boolean) =>
  invoke<void>("plugin_set_enabled", { pluginId, enabled });

export const pluginUninstall = (pluginId: string) =>
  invoke<void>("plugin_uninstall", { pluginId });

export const pluginHostMenus = (hostOs?: string | null, hostArch?: string | null) =>
  invoke<PluginHostMenuDto[]>("plugin_host_menus", { hostOs: hostOs ?? null, hostArch: hostArch ?? null });

export type { PluginWorkspaceCategory, PluginWorkspaceEntry } from "../../features/plugins/workspace/types";
export { pluginWorkspaceEntries } from "../../features/plugins/workspace/api";

export const pluginWorkflowGetForm = (pluginId: string, workflowId: string) =>
  invoke<Record<string, unknown>>("plugin_workflow_get_form", { pluginId, workflowId });

export const pluginWorkflowPlan = (request: WorkflowPlanRequest) =>
  invoke<WorkflowPlanDto>("plugin_workflow_plan", { request });

export const pluginWorkflowExecute = (planId: string) =>
  invoke<WorkflowRunDto>("plugin_workflow_execute", { planId });

export const pluginWorkflowCancel = (runId: string) =>
  invoke<void>("plugin_workflow_cancel", { runId });

/** Bind secret:// ref in Core memory. Value never returned by list APIs. */
export const pluginSecretBind = (secretRef: string, value: string) =>
  invoke<void>("plugin_secret_bind", { request: { secretRef, value } });

export const pluginSecretUnbind = (secretRef: string) =>
  invoke<void>("plugin_secret_unbind", { request: { secretRef } });

export type SecretBindingMeta = {
  secretRef: string;
  redacted: string;
};

export const pluginSecretList = () => invoke<SecretBindingMeta[]>("plugin_secret_list");

export type { MarketplacePlugin } from "../../features/plugins/types/plugin";

export const pluginMarketplaceList = () =>
  invoke<MarketplacePlugin[]>("plugin_marketplace_list");

export const pluginMarketplaceSyncRevocations = () =>
  invoke<number>("plugin_marketplace_sync_revocations");

export const pluginMarketplacePrepareInstall = (
  pluginId: string,
  version?: string | null,
) =>
  invoke<PluginPackageInspection>("plugin_marketplace_prepare_install", {
    pluginId,
    version: version ?? null,
  });

export const pluginAgentTools = () => invoke<unknown[]>("plugin_agent_tools");

export const pluginActivateVersion = (pluginId: string, version: string) =>
  invoke<PluginDto>("plugin_activate_version", { pluginId, version });

export type UpdateCheckResult = {
  pluginId: string;
  installedVersion: string;
  availableVersion: string;
  permissionAdditions: string[];
  requiresPermissionReview: boolean;
};

export const pluginCheckUpdate = (
  pluginId: string,
  availableVersion: string,
  candidatePermissions: string[],
) =>
  invoke<UpdateCheckResult>("plugin_check_update", {
    pluginId,
    availableVersion,
    candidatePermissions,
  });
