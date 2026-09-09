import { Pencil, Plus, Trash2 } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "../../components/ui/button";
import type { DeploymentApp } from "../../types/infrastructure";

export function DeploymentAppList({
  apps,
  selectedId,
  onSelect,
  onCreate,
  onEdit,
  onDelete,
}: {
  apps: DeploymentApp[];
  selectedId: string | null;
  onSelect: (id: string) => void;
  onCreate: () => void;
  onEdit: (app: DeploymentApp) => void;
  onDelete: (app: DeploymentApp) => void;
}) {
  const { t } = useTranslation();
  return (
    <aside className="flex w-48 shrink-0 flex-col border-r" aria-label={t("deployment.apps")}>
      <div className="flex items-center gap-2 border-b p-2">
        <div className="min-w-0 flex-1">
          <p className="truncate text-sm font-medium">{t("deployment.apps")}</p>
          <p className="truncate text-[11px] text-[hsl(var(--muted))]">{t("deployment.appsHint")}</p>
        </div>
        <Button variant="ghost" size="icon" aria-label={t("deployment.createApp")} onClick={onCreate}>
          <Plus size={16} />
        </Button>
      </div>
      <div className="min-h-0 flex-1 space-y-1 overflow-auto p-2">
        {apps.length === 0 ? (
          <p className="px-1 py-3 text-xs text-[hsl(var(--muted))]">{t("deployment.appsEmpty")}</p>
        ) : (
          apps.map((app) => {
            const selected = app.id === selectedId;
            return (
              <div key={app.id} className={`group flex items-center gap-1 rounded-md ${selected ? "bg-[hsl(var(--elevated))]" : ""}`}>
                <Button
                  variant={selected ? "secondary" : "ghost"}
                  className="min-w-0 flex-1 justify-start px-2"
                  aria-pressed={selected}
                  onClick={() => onSelect(app.id)}
                >
                  <span className="truncate">{app.name}</span>
                </Button>
                <Button
                  variant="ghost"
                  size="icon"
                  className="opacity-0 group-hover:opacity-100 focus-visible:opacity-100"
                  aria-label={t("deployment.editApp")}
                  onClick={() => onEdit(app)}
                >
                  <Pencil size={14} />
                </Button>
                <Button
                  variant="ghost"
                  size="icon"
                  className="text-red-500 opacity-0 group-hover:opacity-100 focus-visible:opacity-100"
                  aria-label={t("common.delete")}
                  onClick={() => onDelete(app)}
                >
                  <Trash2 size={14} />
                </Button>
              </div>
            );
          })
        )}
      </div>
    </aside>
  );
}
