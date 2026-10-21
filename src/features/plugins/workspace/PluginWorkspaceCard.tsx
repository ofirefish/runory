import { useTranslation } from "react-i18next";
import { Button } from "../../../components/ui/button";
import { PluginLogo } from "./PluginLogo";
import { PluginWorkspaceActionMenu } from "./PluginWorkspaceActionMenu";
import type { PluginWorkspaceEntry } from "./types";

function primaryLabel(entry: PluginWorkspaceEntry): string {
  const index = entry.actions.indexOf(entry.command);
  if (index >= 0 && entry.actionTitles[index]) {
    return entry.actionTitles[index];
  }
  return entry.title;
}

export function PluginWorkspaceCard({
  entry,
  onRun,
}: {
  entry: PluginWorkspaceEntry;
  onRun: (command: string) => void;
}) {
  const { t } = useTranslation();
  return (
    <article className="flex flex-col gap-3 rounded-lg border border-border bg-[hsl(var(--surface))] p-4">
      <header className="flex items-start gap-3">
        <PluginLogo title={entry.title} logoUrl={entry.logoUrl} size="md" />
        <div className="min-w-0 flex-1">
          <h3 className="truncate text-sm font-medium">{entry.title}</h3>
          {entry.description ? (
            <p className="mt-1 line-clamp-2 text-xs text-[hsl(var(--secondary))]">
              {entry.description}
            </p>
          ) : null}
        </div>
      </header>
      <div className="mt-auto flex items-center gap-2">
        <Button type="button" size="sm" onClick={() => onRun(entry.command)}>
          {primaryLabel(entry)}
        </Button>
        <div className="ml-auto">
          <PluginWorkspaceActionMenu entry={entry} onAction={onRun} />
        </div>
      </div>
      <p className="sr-only">
        {t("plugins.workspace.cardMeta", {
          plugin: entry.pluginId,
          version: entry.pluginVersion,
        })}
      </p>
    </article>
  );
}
