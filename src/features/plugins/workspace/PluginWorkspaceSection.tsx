import { useState } from "react";
import { useTranslation } from "react-i18next";
import { WorkflowExecutionDialog } from "../components/WorkflowExecutionDialog";
import { usePluginWorkspaceEntries } from "./hooks";
import { PluginWorkspaceGrid } from "./PluginWorkspaceGrid";
import type { PluginWorkspaceCategory, PluginWorkspaceEntry } from "./types";

export function PluginWorkspaceSection({
  category,
  titleKey,
  hostId,
  sessionId,
}: {
  category: PluginWorkspaceCategory;
  titleKey: string;
  hostId?: string | null;
  sessionId?: string | null;
}) {
  const { t } = useTranslation();
  const { entries, loading, error } = usePluginWorkspaceEntries(category);
  const [active, setActive] = useState<{
    pluginId: string;
    workflowId: string;
  } | null>(null);

  const run = (entry: PluginWorkspaceEntry, command: string) => {
    const workflowId =
      command === entry.command && entry.workflowId ? entry.workflowId : command;
    setActive({ pluginId: entry.pluginId, workflowId });
  };

  if (!loading && !error && entries.length === 0) {
    return null;
  }

  return (
    <section className="mt-4 space-y-3 border-t border-border pt-4" aria-label={t(titleKey)}>
      <div>
        <h3 className="text-sm font-medium">{t(titleKey)}</h3>
        <p className="text-xs text-[hsl(var(--muted))]">{t("plugins.workspace.sectionHint")}</p>
      </div>
      {loading ? (
        <p className="text-sm text-[hsl(var(--muted))]">{t("plugins.loading")}</p>
      ) : null}
      {error ? (
        <p className="text-sm text-red-500">{t("plugins.workspace.loadFailed")}</p>
      ) : null}
      {!loading && !error ? <PluginWorkspaceGrid entries={entries} onRun={run} /> : null}
      {active ? (
        <WorkflowExecutionDialog
          open
          pluginId={active.pluginId}
          workflowId={active.workflowId}
          hostId={hostId ?? undefined}
          sessionId={sessionId ?? undefined}
          onClose={() => setActive(null)}
        />
      ) : null}
    </section>
  );
}
