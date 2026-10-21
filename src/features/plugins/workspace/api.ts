import { invoke } from "@tauri-apps/api/core";
import type { PluginWorkspaceCategory, PluginWorkspaceEntry } from "./types";

export const pluginWorkspaceEntries = (category?: PluginWorkspaceCategory | null) =>
  invoke<PluginWorkspaceEntry[]>("plugin_workspace_entries", {
    category: category ?? null,
  });
