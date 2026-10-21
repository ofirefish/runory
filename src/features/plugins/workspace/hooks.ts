import { useCallback, useEffect, useState } from "react";
import { usePluginStore } from "../store/pluginStore";
import { pluginWorkspaceEntries } from "./api";
import type { PluginWorkspaceCategory, PluginWorkspaceEntry } from "./types";

/**
 * Normalized workspace entries for a fixed Runory workspace category.
 * Refreshes when installed plugins change (install/enable/disable/uninstall).
 */
export function usePluginWorkspaceEntries(category: PluginWorkspaceCategory) {
  const plugins = usePluginStore((store) => store.plugins);
  const [entries, setEntries] = useState<PluginWorkspaceEntry[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState(false);

  const refresh = useCallback(async () => {
    setLoading(true);
    setError(false);
    try {
      setEntries(await pluginWorkspaceEntries(category));
    } catch {
      setEntries([]);
      setError(true);
    } finally {
      setLoading(false);
    }
  }, [category]);

  useEffect(() => {
    void refresh();
  }, [refresh, plugins]);

  return { entries, loading, error, refresh };
}
