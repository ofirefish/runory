import { useTranslation } from "react-i18next";
import type { PluginDto } from "../types/plugin";
import { Button } from "../../../components/ui/button";
import { PluginLogo } from "../workspace/PluginLogo";

export function PluginCard({
  plugin,
  onToggle,
  onUninstall,
}: {
  plugin: PluginDto;
  onToggle: (enabled: boolean) => void;
  onUninstall: () => void;
}) {
  const { t } = useTranslation();
  const categoryLabel = plugin.category
    ? t(`plugins.workspace.category.${plugin.category}`)
    : t("plugins.workspace.category.none");
  return (
    <article className="plugin-card rounded-lg border border-border bg-card p-4">
      <div className="flex items-start justify-between gap-3">
        <div className="flex min-w-0 items-start gap-3">
          <PluginLogo title={plugin.name} logoUrl={plugin.logoUrl} size="md" />
          <div className="min-w-0">
            <h3 className="text-sm font-medium">{plugin.name}</h3>
            <p className="text-xs text-muted-foreground">
              {plugin.id} · v{plugin.version} · {plugin.publisherId}
            </p>
            <p className="mt-1 text-xs text-muted-foreground">
              {t("plugins.detail.workspaceCategory")}: {categoryLabel}
            </p>
          </div>
        </div>
        <span className="text-xs text-muted-foreground">
          {plugin.enabled ? t("plugins.status.enabled") : t("plugins.status.disabled")}
        </span>
      </div>
      <ul className="mt-3 space-y-1 text-xs text-muted-foreground">
        {plugin.permissions.map((permission) => (
          <li key={permission.permission}>
            {permission.permission}
            {permission.granted ? "" : ` (${t("plugins.permission.denied")})`}
          </li>
        ))}
      </ul>
      <div className="mt-4 flex gap-2">
        <Button type="button" variant="secondary" size="sm" onClick={() => onToggle(!plugin.enabled)}>
          {plugin.enabled ? t("plugins.action.disable") : t("plugins.action.enable")}
        </Button>
        <Button type="button" variant="danger" size="sm" onClick={onUninstall}>
          {t("plugins.action.uninstall")}
        </Button>
      </div>
    </article>
  );
}
