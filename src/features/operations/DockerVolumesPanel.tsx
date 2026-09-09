import { Search, Settings, Trash2 } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { Button } from "../../components/ui/button";
import { DialogShell } from "../../components/ui/dialog-shell";
import { Input } from "../../components/ui/input";
import { Label } from "../../components/ui/label";
import { SelectControl } from "../../components/ui/select-control";
import type { DockerVolume, DockerVolumeAction } from "../../types/infrastructure";
import { cn } from "../../lib/utils";

const PAGE_SIZE_OPTIONS = ["10", "20", "50", "100"] as const;
const DRIVER_OPTIONS = ["local"] as const;

function formatCreatedAt(epochSeconds: number, locale: string): string {
  if (!epochSeconds) return "—";
  return new Intl.DateTimeFormat(locale, {
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
    hour12: false,
  }).format(new Date(epochSeconds * 1000));
}

function parseLabels(value: string): string[] {
  return value
    .split(/[\s,]+/)
    .map((item) => item.trim())
    .filter(Boolean);
}

function formatUsedBy(usedBy: string[]): string {
  return usedBy.length > 0 ? usedBy.join(", ") : "";
}

type ConfirmKind =
  | { kind: "remove"; names: string[]; label: string }
  | { kind: "prune" }
  | { kind: "bulkRemove"; names: string[] };

export function DockerVolumesPanel({
  volumes,
  busy = false,
  onAction,
  onOpenSettings,
}: {
  volumes: DockerVolume[];
  busy?: boolean;
  onAction: (action: DockerVolumeAction) => Promise<void>;
  onOpenSettings?: () => void;
}) {
  const { t, i18n } = useTranslation();
  const [query, setQuery] = useState("");
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [page, setPage] = useState(1);
  const [pageSize, setPageSize] = useState<(typeof PAGE_SIZE_OPTIONS)[number]>("20");
  const [jumpPage, setJumpPage] = useState("1");
  const [bulkAction, setBulkAction] = useState<"none" | "remove">("none");
  const [createOpen, setCreateOpen] = useState(false);
  const [createName, setCreateName] = useState("");
  const [createDriver, setCreateDriver] = useState<(typeof DRIVER_OPTIONS)[number]>("local");
  const [createLabels, setCreateLabels] = useState("");
  const [confirm, setConfirm] = useState<ConfirmKind | null>(null);
  const [actionBusy, setActionBusy] = useState(false);
  const [actionFailed, setActionFailed] = useState(false);
  const [actionErrorDetail, setActionErrorDetail] = useState("");

  const volumeList = volumes ?? [];

  const filtered = useMemo(() => {
    const needle = query.trim().toLowerCase();
    if (!needle) return volumeList;
    return volumeList.filter((volume) => {
      const haystack = [
        volume.name,
        volume.driver,
        volume.mountpoint,
        volume.scope,
        volume.labels,
        formatUsedBy(volume.usedBy),
      ]
        .join(" ")
        .toLowerCase();
      return haystack.includes(needle);
    });
  }, [volumeList, query]);

  const pageSizeNumber = Number(pageSize);
  const totalPages = Math.max(1, Math.ceil(filtered.length / pageSizeNumber));
  const currentPage = Math.min(page, totalPages);
  const pageItems = filtered.slice((currentPage - 1) * pageSizeNumber, currentPage * pageSizeNumber);
  const allPageSelected = pageItems.length > 0 && pageItems.every((item) => selected.has(item.name));

  const pageNumbers = useMemo(() => {
    const windowSize = 5;
    const start = Math.max(1, Math.min(currentPage - 2, totalPages - windowSize + 1));
    const end = Math.min(totalPages, start + windowSize - 1);
    return Array.from({ length: end - start + 1 }, (_, index) => start + index);
  }, [currentPage, totalPages]);

  useEffect(() => {
    setPage(1);
  }, [query, pageSize]);

  useEffect(() => {
    setJumpPage(String(currentPage));
  }, [currentPage]);

  useEffect(() => {
    const valid = new Set(volumeList.map((volume) => volume.name));
    setSelected((previous) => {
      const next = new Set([...previous].filter((name) => valid.has(name)));
      return next.size === previous.size ? previous : next;
    });
  }, [volumeList]);

  const toggleAllPage = () => {
    setSelected((previous) => {
      const next = new Set(previous);
      if (allPageSelected) {
        for (const item of pageItems) next.delete(item.name);
      } else {
        for (const item of pageItems) next.add(item.name);
      }
      return next;
    });
  };

  const toggleOne = (name: string) => {
    setSelected((previous) => {
      const next = new Set(previous);
      if (next.has(name)) next.delete(name);
      else next.add(name);
      return next;
    });
  };

  const runAction = async (action: DockerVolumeAction) => {
    setActionBusy(true);
    setActionFailed(false);
    setActionErrorDetail("");
    try {
      await onAction(action);
      setCreateOpen(false);
      setCreateName("");
      setCreateDriver("local");
      setCreateLabels("");
      setConfirm(null);
      if (action.type === "remove") {
        setSelected((previous) => {
          const next = new Set(previous);
          for (const name of action.names) next.delete(name);
          return next;
        });
        toast.success(
          action.names.length > 1
            ? t("operations.dockerVolumes.deleteSuccessBulk", { count: action.names.length })
            : t("operations.dockerVolumes.deleteSuccess"),
        );
      } else if (action.type === "prune") {
        toast.success(t("operations.dockerVolumes.pruneSuccess"));
      } else if (action.type === "create") {
        toast.success(t("operations.dockerVolumes.createSuccess"));
      }
    } catch (error) {
      setActionFailed(true);
      setActionErrorDetail(error instanceof Error ? error.message : String(error));
    } finally {
      setActionBusy(false);
    }
  };

  const failureMessage = (
    <div className="space-y-1">
      <p className="text-sm text-red-500">{t("operations.error")}</p>
      {actionErrorDetail ? (
        <pre className="max-h-32 overflow-auto whitespace-pre-wrap rounded-md border border-red-500/20 bg-red-500/5 p-2 font-mono text-xs text-red-500/90">
          {actionErrorDetail.slice(0, 2_000)}
        </pre>
      ) : null}
    </div>
  );

  return (
    <div className="flex h-full min-h-0 flex-col gap-3">
      <div className="flex flex-wrap items-center gap-2">
        <Button
          type="button"
          size="sm"
          disabled={busy || actionBusy}
          onClick={() => {
            setActionFailed(false);
            setCreateOpen(true);
          }}
        >
          {t("operations.dockerVolumes.create")}
        </Button>
        <Button
          type="button"
          variant="secondary"
          size="sm"
          disabled={busy || actionBusy}
          onClick={() => {
            setActionFailed(false);
            setConfirm({ kind: "prune" });
          }}
        >
          {t("operations.dockerVolumes.prune")}
        </Button>
        <div className="relative ml-auto min-w-[16rem] flex-1 sm:max-w-sm">
          <Search size={14} className="pointer-events-none absolute left-3 top-1/2 -translate-y-1/2 text-[hsl(var(--muted))]" />
          <Input
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            placeholder={t("operations.dockerVolumes.searchPlaceholder")}
            aria-label={t("operations.dockerVolumes.searchPlaceholder")}
            className="pl-8"
          />
        </div>
        {onOpenSettings ? (
          <Button
            type="button"
            variant="ghost"
            size="icon"
            className="h-8 w-8"
            title={t("operations.dockerTab.settings")}
            aria-label={t("operations.dockerTab.settings")}
            onClick={onOpenSettings}
          >
            <Settings size={15} />
          </Button>
        ) : null}
      </div>

      <div className="min-h-0 flex-1 overflow-auto rounded-lg border">
        <table className="w-full text-left text-sm">
          <thead className="sticky top-0 bg-[hsl(var(--elevated))] text-xs">
            <tr>
              <th className="w-10 px-3 py-2">
                <input
                  type="checkbox"
                  checked={allPageSelected}
                  onChange={toggleAllPage}
                  aria-label={t("operations.dockerVolumes.selectAll")}
                />
              </th>
              <th className="px-3 py-2">{t("operations.dockerVolumes.name")}</th>
              <th className="px-3 py-2">{t("operations.dockerVolumes.mountpoint")}</th>
              <th className="px-3 py-2">{t("operations.dockerVolumes.usedBy")}</th>
              <th className="px-3 py-2">{t("operations.dockerVolumes.driver")}</th>
              <th className="px-3 py-2">{t("operations.dockerVolumes.createdAt")}</th>
              <th className="px-3 py-2">{t("operations.dockerVolumes.labels")}</th>
              <th className="w-20 px-3 py-2">{t("operations.actions")}</th>
            </tr>
          </thead>
          <tbody>
            {pageItems.length === 0 ? (
              <tr>
                <td colSpan={8} className="px-3 py-8 text-center text-[hsl(var(--muted))]">
                  {t("operations.dockerVolumes.empty")}
                </td>
              </tr>
            ) : (
              pageItems.map((volume) => {
                const usedBy = formatUsedBy(volume.usedBy);
                return (
                  <tr key={volume.name} className="border-t hover:bg-[hsl(var(--elevated)/.45)]">
                    <td className="px-3 py-2">
                      <input
                        type="checkbox"
                        checked={selected.has(volume.name)}
                        onChange={() => toggleOne(volume.name)}
                        aria-label={t("operations.dockerVolumes.selectRow", { name: volume.name })}
                      />
                    </td>
                    <td className="max-w-xs truncate px-3 py-2 font-mono text-xs" title={volume.name}>
                      {volume.name}
                    </td>
                    <td className="max-w-sm truncate px-3 py-2 font-mono text-xs" title={volume.mountpoint || undefined}>
                      {volume.mountpoint || "—"}
                    </td>
                    <td className="max-w-xs truncate px-3 py-2 text-xs" title={usedBy || undefined}>
                      {usedBy || "—"}
                    </td>
                    <td className="px-3 py-2 font-mono text-xs">{volume.driver || "—"}</td>
                    <td className="whitespace-nowrap px-3 py-2 tabular-nums">
                      {formatCreatedAt(volume.createdAtEpochSeconds, i18n.language)}
                    </td>
                    <td className="max-w-xs truncate px-3 py-2 text-xs" title={volume.labels || undefined}>
                      {volume.labels || "—"}
                    </td>
                    <td className="px-3 py-2">
                      <Button
                        type="button"
                        variant="ghost"
                        size="icon"
                        className="h-7 w-7 text-red-500 hover:text-red-500"
                        title={t("operations.dockerVolumes.delete")}
                        aria-label={t("operations.dockerVolumes.delete")}
                        disabled={busy || actionBusy}
                        onClick={() => {
                          setActionFailed(false);
                          setConfirm({ kind: "remove", names: [volume.name], label: volume.name });
                        }}
                      >
                        <Trash2 size={13} />
                      </Button>
                    </td>
                  </tr>
                );
              })
            )}
          </tbody>
        </table>
      </div>

      <div className="flex flex-wrap items-center gap-3">
        <div className="flex flex-wrap items-center gap-2">
          <SelectControl
            value={bulkAction}
            onValueChange={setBulkAction}
            label={t("operations.dockerVolumes.bulkAction")}
            className="w-44"
            options={[
              { value: "none", label: t("operations.dockerVolumes.bulkPlaceholder") },
              { value: "remove", label: t("operations.dockerVolumes.bulkDelete") },
            ]}
          />
          <Button
            type="button"
            size="sm"
            disabled={busy || actionBusy || bulkAction === "none" || selected.size === 0}
            onClick={() => {
              if (bulkAction !== "remove") return;
              setActionFailed(false);
              setConfirm({ kind: "bulkRemove", names: [...selected] });
            }}
          >
            {t("operations.dockerVolumes.bulkRun")}
          </Button>
        </div>

        <div className="ml-auto flex flex-wrap items-center gap-2 text-xs text-[hsl(var(--secondary))]">
          <Button type="button" variant="ghost" size="sm" disabled={currentPage <= 1} onClick={() => setPage(currentPage - 1)}>
            ‹
          </Button>
          {pageNumbers.map((value) => (
            <Button
              key={value}
              type="button"
              variant={value === currentPage ? "secondary" : "ghost"}
              size="sm"
              className={cn("min-w-8 px-2", value === currentPage && "font-semibold")}
              onClick={() => setPage(value)}
            >
              {value}
            </Button>
          ))}
          <Button
            type="button"
            variant="ghost"
            size="sm"
            disabled={currentPage >= totalPages}
            onClick={() => setPage(currentPage + 1)}
          >
            ›
          </Button>
          <SelectControl
            value={pageSize}
            onValueChange={setPageSize}
            label={t("operations.dockerVolumes.pageSize")}
            className="w-28"
            options={PAGE_SIZE_OPTIONS.map((value) => ({
              value,
              label: t("operations.dockerVolumes.pageSizeOption", { count: Number(value) }),
            }))}
          />
          <span>{t("operations.dockerVolumes.total", { count: filtered.length })}</span>
          <span className="inline-flex items-center gap-1">
            {t("operations.dockerVolumes.goto")}
            <Input
              type="number"
              min={1}
              max={totalPages}
              value={jumpPage}
              onChange={(event) => setJumpPage(event.target.value)}
              onKeyDown={(event) => {
                if (event.key !== "Enter") return;
                const next = Number(jumpPage);
                if (Number.isFinite(next)) setPage(Math.min(totalPages, Math.max(1, Math.trunc(next))));
              }}
              className="h-8 w-14 px-2"
              aria-label={t("operations.dockerVolumes.goto")}
            />
            {t("operations.dockerVolumes.page")}
          </span>
        </div>
      </div>

      {createOpen && (
        <DialogShell
          title={t("operations.dockerVolumes.createTitle")}
          onClose={() => !actionBusy && setCreateOpen(false)}
          closeDisabled={actionBusy}
          size="form"
        >
          <div className="space-y-3">
            <div className="space-y-1.5">
              <Label htmlFor="docker-volume-name">{t("operations.dockerVolumes.name")}</Label>
              <Input
                id="docker-volume-name"
                value={createName}
                placeholder={t("operations.dockerVolumes.namePlaceholder")}
                onChange={(event) => setCreateName(event.target.value)}
                disabled={actionBusy}
              />
            </div>
            <div className="space-y-1.5">
              <Label htmlFor="docker-volume-driver">{t("operations.dockerVolumes.driver")}</Label>
              <SelectControl
                id="docker-volume-driver"
                value={createDriver}
                onValueChange={setCreateDriver}
                label={t("operations.dockerVolumes.driver")}
                className="w-full"
                options={DRIVER_OPTIONS.map((value) => ({ value, label: value }))}
              />
            </div>
            <div className="space-y-1.5">
              <Label htmlFor="docker-volume-labels">{t("operations.dockerVolumes.labels")}</Label>
              <Input
                id="docker-volume-labels"
                value={createLabels}
                placeholder={t("operations.dockerVolumes.labelsPlaceholder")}
                onChange={(event) => setCreateLabels(event.target.value)}
                disabled={actionBusy}
              />
            </div>
            {actionFailed && failureMessage}
            <div className="flex justify-end gap-2">
              <Button type="button" variant="ghost" disabled={actionBusy} onClick={() => setCreateOpen(false)}>
                {t("common.cancel")}
              </Button>
              <Button
                type="button"
                disabled={actionBusy || !createName.trim()}
                onClick={() =>
                  void runAction({
                    type: "create",
                    name: createName.trim(),
                    driver: createDriver,
                    labels: parseLabels(createLabels),
                  })
                }
              >
                {t("operations.dockerVolumes.create")}
              </Button>
            </div>
          </div>
        </DialogShell>
      )}

      {confirm && (
        <DialogShell
          title={t("operations.confirmTitle")}
          onClose={() => !actionBusy && setConfirm(null)}
          closeDisabled={actionBusy}
        >
          <p className="text-sm text-[hsl(var(--secondary))]">
            {confirm.kind === "prune"
              ? t("operations.dockerVolumes.confirmPrune")
              : confirm.kind === "bulkRemove"
                ? t("operations.dockerVolumes.confirmBulkDelete", { count: confirm.names.length })
                : t("operations.confirmDescription", {
                    action: t("operations.dockerVolumes.delete"),
                    target: confirm.label,
                  })}
          </p>
          {actionFailed && <div className="mt-3">{failureMessage}</div>}
          <div className="mt-5 flex justify-end gap-2">
            <Button type="button" variant="ghost" disabled={actionBusy} onClick={() => setConfirm(null)}>
              {t("common.cancel")}
            </Button>
            <Button
              type="button"
              variant={confirm.kind === "prune" ? "default" : "danger"}
              disabled={actionBusy}
              onClick={() => {
                if (confirm.kind === "prune") void runAction({ type: "prune" });
                else void runAction({ type: "remove", names: confirm.names });
              }}
            >
              {confirm.kind === "prune"
                ? t("operations.dockerVolumes.prune")
                : t("operations.dockerVolumes.delete")}
            </Button>
          </div>
        </DialogShell>
      )}
    </div>
  );
}
