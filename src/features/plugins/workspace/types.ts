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
  description?: string | null;
  logoUrl?: string | null;
  category: PluginWorkspaceCategory;
  command: string;
  actions: string[];
  order: number;
  publisherId: string;
  workflowId?: string | null;
  actionTitles: string[];
}
