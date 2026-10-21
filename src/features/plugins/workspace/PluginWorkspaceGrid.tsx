import { PluginWorkspaceCard } from "./PluginWorkspaceCard";
import type { PluginWorkspaceEntry } from "./types";

export function PluginWorkspaceGrid({
  entries,
  onRun,
}: {
  entries: PluginWorkspaceEntry[];
  onRun: (entry: PluginWorkspaceEntry, command: string) => void;
}) {
  if (entries.length === 0) {
    return null;
  }
  return (
    <div className="grid gap-3 sm:grid-cols-2 xl:grid-cols-3">
      {entries.map((entry) => (
        <PluginWorkspaceCard
          key={`${entry.pluginId}:${entry.entryId}`}
          entry={entry}
          onRun={(command) => onRun(entry, command)}
        />
      ))}
    </div>
  );
}
