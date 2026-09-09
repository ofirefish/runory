import { Box, Search, Trash2 } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { Button } from "../../components/ui/button";
import { DialogShell } from "../../components/ui/dialog-shell";
import { Input } from "../../components/ui/input";
import { Label } from "../../components/ui/label";
import { SelectControl } from "../../components/ui/select-control";
import type { DockerImage, DockerImageAction } from "../../types/infrastructure";
import { formatFileSize } from "../files/sftp-format";
import { cn } from "../../lib/utils";

const PAGE_SIZE_OPTIONS = ["10", "20", "50", "100"] as const;

function shortImageId(id: string): string {
  const raw = id.startsWith("sha256:") ? id.slice("sha256:".length) : id;
  return raw.slice(0, 12) || id;
}

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

function parsePublishPorts(value: string): string[] {
  return value
    .split(/[\s,]+/)
    .map((item) => item.trim())
    .filter(Boolean);
}

type ConfirmKind =
  | { kind: "remove"; ids: string[]; label: string }
  | { kind: "prune" }
  | { kind: "bulkRemove"; ids: string[] };

export function DockerImagesPanel({
  images,
  busy = false,
  onAction,
}: {
  images: DockerImage[];
  busy?: boolean;
  onAction: (action: DockerImageAction) => Promise<void>;
}) {
  const { t, i18n } = useTranslation();
  const [query, setQuery] = useState("");
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [page, setPage] = useState(1);
  const [pageSize, setPageSize] = useState<(typeof PAGE_SIZE_OPTIONS)[number]>("20");
  const [jumpPage, setJumpPage] = useState("1");
  const [bulkAction, setBulkAction] = useState<"none" | "remove">("none");
  const [pullOpen, setPullOpen] = useState(false);
  const [pullReference, setPullReference] = useState("");
  const [createOpen, setCreateOpen] = useState<DockerImage | null>(null);
  const [containerName, setContainerName] = useState("");
  const [publishPorts, setPublishPorts] = useState("");
  const [confirm, setConfirm] = useState<ConfirmKind | null>(null);
  const [actionBusy, setActionBusy] = useState(false);
  const [actionFailed, setActionFailed] = useState(false);
  const [actionErrorDetail, setActionErrorDetail] = useState("");

  const filtered = useMemo(() => {
    const needle = query.trim().toLowerCase();
    if (!needle) return images;
    return images.filter((image) => {
      const haystack = [
        image.id,
        shortImageId(image.id),
        image.name,
        ...image.usedBy,
      ]
        .join(" ")
        .toLowerCase();
      return haystack.includes(needle);
    });
  }, [images, query]);

  const pageSizeNumber = Number(pageSize);
  const totalPages = Math.max(1, Math.ceil(filtered.length / pageSizeNumber));
  const currentPage = Math.min(page, totalPages);
  const pageItems = filtered.slice((currentPage - 1) * pageSizeNumber, currentPage * pageSizeNumber);
  const allPageSelected = pageItems.length > 0 && pageItems.every((item) => selected.has(item.id));

  useEffect(() => {
    setPage(1);
  }, [query, pageSize]);

  useEffect(() => {
    setJumpPage(String(currentPage));
  }, [currentPage]);

  useEffect(() => {
    const valid = new Set(images.map((image) => image.id));
    setSelected((previous) => {
      const next = new Set([...previous].filter((id) => valid.has(id)));
      return next.size === previous.size ? previous : next;
    });
  }, [images]);

  const toggleAllPage = () => {
    setSelected((previous) => {
      const next = new Set(previous);
      if (allPageSelected) {
        for (const item of pageItems) next.delete(item.id);
      } else {
        for (const item of pageItems) next.add(item.id);
      }
      return next;
    });
  };

  const toggleOne = (id: string) => {
    setSelected((previous) => {
      const next = new Set(previous);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  };

  const runAction = async (action: DockerImageAction) => {
    setActionBusy(true);
    setActionFailed(false);
    setActionErrorDetail("");
    try {
      await onAction(action);
      setPullOpen(false);
      setPullReference("");
      setCreateOpen(null);
      setContainerName("");
      setPublishPorts("");
      setConfirm(null);
      if (action.type === "remove") {
        setSelected((previous) => {
          const next = new Set(previous);
          for (const id of action.ids) next.delete(id);
          return next;
        });
        toast.success(
          action.ids.length > 1
            ? t("operations.dockerImages.deleteSuccessBulk", { count: action.ids.length })
            : t("operations.dockerImages.deleteSuccess"),
        );
      }
    } catch (error) {
      setActionFailed(true);
      const detail = error instanceof Error ? error.message.trim() : "";
      setActionErrorDetail(detail && detail !== "docker image action failed" ? detail : "");
    } finally {
      setActionBusy(false);
    }
  };

  const pageNumbers = useMemo(() => {
    const pages: number[] = [];
    const start = Math.max(1, currentPage - 2);
    const end = Math.min(totalPages, start + 4);
    for (let value = Math.max(1, end - 4); value <= end; value += 1) pages.push(value);
    return pages;
  }, [currentPage, totalPages]);

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
        <Button type="button" variant="secondary" size="sm" disabled={busy || actionBusy} onClick={() => { setActionFailed(false); setPullOpen(true); }}>
          {t("operations.dockerImages.pull")}
        </Button>
        <Button type="button" variant="secondary" size="sm" disabled={busy || actionBusy} onClick={() => { setActionFailed(false); setConfirm({ kind: "prune" }); }}>
          {t("operations.dockerImages.prune")}
        </Button>
        <div className="relative ml-auto min-w-[16rem] flex-1 sm:max-w-sm">
          <Search size={14} className="pointer-events-none absolute left-3 top-1/2 -translate-y-1/2 text-[hsl(var(--muted))]" />
          <Input
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            placeholder={t("operations.dockerImages.searchPlaceholder")}
            aria-label={t("operations.dockerImages.searchPlaceholder")}
            className="pl-8"
          />
        </div>
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
                  aria-label={t("operations.dockerImages.selectAll")}
                />
              </th>
              <th className="px-3 py-2">{t("operations.dockerImages.id")}</th>
              <th className="px-3 py-2">{t("operations.dockerImages.name")}</th>
              <th className="px-3 py-2">{t("operations.dockerImages.size")}</th>
              <th className="px-3 py-2">{t("operations.dockerImages.createdAt")}</th>
              <th className="px-3 py-2">{t("operations.dockerImages.usedBy")}</th>
              <th className="w-28 px-3 py-2">{t("operations.actions")}</th>
            </tr>
          </thead>
          <tbody>
            {pageItems.length === 0 ? (
              <tr>
                <td colSpan={7} className="px-3 py-8 text-center text-[hsl(var(--muted))]">
                  {t("operations.dockerImages.empty")}
                </td>
              </tr>
            ) : (
              pageItems.map((image) => (
                <tr key={image.id} className="border-t hover:bg-[hsl(var(--elevated)/.45)]">
                  <td className="px-3 py-2">
                    <input
                      type="checkbox"
                      checked={selected.has(image.id)}
                      onChange={() => toggleOne(image.id)}
                      aria-label={t("operations.dockerImages.selectRow", { name: image.name })}
                    />
                  </td>
                  <td className="px-3 py-2 font-mono text-xs" title={image.id}>{shortImageId(image.id)}</td>
                  <td className="max-w-xs truncate px-3 py-2 font-mono text-xs" title={image.name}>{image.name}</td>
                  <td className="px-3 py-2 tabular-nums">{formatFileSize(image.sizeBytes, i18n.language)}</td>
                  <td className="whitespace-nowrap px-3 py-2 tabular-nums">{formatCreatedAt(image.createdAtEpochSeconds, i18n.language)}</td>
                  <td className="px-3 py-2">
                    {image.usedBy.length === 0 ? (
                      <span className="text-[hsl(var(--muted))]">—</span>
                    ) : (
                      <div className="flex flex-wrap gap-1">
                        {image.usedBy.map((name) => (
                          <span
                            key={name}
                            className="rounded border border-[hsl(var(--primary)/.25)] bg-[hsl(var(--primary)/.1)] px-1.5 py-0.5 text-xs text-[hsl(var(--primary))]"
                          >
                            {name}
                          </span>
                        ))}
                      </div>
                    )}
                  </td>
                  <td className="px-3 py-2">
                    <div className="flex gap-1">
                      <Button
                        type="button"
                        variant="ghost"
                        size="icon"
                        className="h-7 w-7"
                        title={t("operations.dockerImages.createContainer")}
                        aria-label={t("operations.dockerImages.createContainer")}
                        disabled={busy || actionBusy || image.name === "<none>"}
                        onClick={() => {
                          setActionFailed(false);
                          setContainerName("");
                          setPublishPorts("");
                          setCreateOpen(image);
                        }}
                      >
                        <Box size={13} />
                      </Button>
                      <Button
                        type="button"
                        variant="ghost"
                        size="icon"
                        className="h-7 w-7 text-red-500 hover:text-red-500"
                        title={t("operations.dockerImages.delete")}
                        aria-label={t("operations.dockerImages.delete")}
                        disabled={busy || actionBusy}
                        onClick={() => {
                          setActionFailed(false);
                          setConfirm({ kind: "remove", ids: [image.id], label: image.name });
                        }}
                      >
                        <Trash2 size={13} />
                      </Button>
                    </div>
                  </td>
                </tr>
              ))
            )}
          </tbody>
        </table>
      </div>

      <div className="flex flex-wrap items-center gap-3">
        <div className="flex flex-wrap items-center gap-2">
          <SelectControl
            value={bulkAction}
            onValueChange={setBulkAction}
            label={t("operations.dockerImages.bulkAction")}
            className="w-44"
            options={[
              { value: "none", label: t("operations.dockerImages.bulkPlaceholder") },
              { value: "remove", label: t("operations.dockerImages.bulkDelete") },
            ]}
          />
          <Button
            type="button"
            size="sm"
            disabled={busy || actionBusy || bulkAction === "none" || selected.size === 0}
            onClick={() => {
              if (bulkAction !== "remove") return;
              setActionFailed(false);
              setConfirm({ kind: "bulkRemove", ids: [...selected] });
            }}
          >
            {t("operations.dockerImages.bulkRun")}
          </Button>
        </div>

        <div className="ml-auto flex flex-wrap items-center gap-2 text-xs text-[hsl(var(--secondary))]">
          <Button type="button" variant="ghost" size="sm" disabled={currentPage <= 1} onClick={() => setPage(currentPage - 1)}>‹</Button>
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
          <Button type="button" variant="ghost" size="sm" disabled={currentPage >= totalPages} onClick={() => setPage(currentPage + 1)}>›</Button>
          <SelectControl
            value={pageSize}
            onValueChange={setPageSize}
            label={t("operations.dockerImages.pageSize")}
            className="w-28"
            options={PAGE_SIZE_OPTIONS.map((value) => ({
              value,
              label: t("operations.dockerImages.pageSizeOption", { count: Number(value) }),
            }))}
          />
          <span>{t("operations.dockerImages.total", { count: filtered.length })}</span>
          <span className="inline-flex items-center gap-1">
            {t("operations.dockerImages.goto")}
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
              aria-label={t("operations.dockerImages.goto")}
            />
            {t("operations.dockerImages.page")}
          </span>
        </div>
      </div>

      {pullOpen && (
        <DialogShell title={t("operations.dockerImages.pullTitle")} onClose={() => !actionBusy && setPullOpen(false)} closeDisabled={actionBusy} size="form">
          <div className="space-y-3">
            <div className="space-y-1.5">
              <Label htmlFor="docker-image-pull-ref">{t("operations.dockerImages.reference")}</Label>
              <Input
                id="docker-image-pull-ref"
                value={pullReference}
                placeholder={t("operations.dockerImages.referencePlaceholder")}
                onChange={(event) => setPullReference(event.target.value)}
                disabled={actionBusy}
              />
            </div>
            {actionFailed && failureMessage}
            <div className="flex justify-end gap-2">
              <Button type="button" variant="ghost" disabled={actionBusy} onClick={() => setPullOpen(false)}>{t("common.cancel")}</Button>
              <Button
                type="button"
                disabled={actionBusy || !pullReference.trim()}
                onClick={() => void runAction({ type: "pull", reference: pullReference.trim() })}
              >
                {t("operations.dockerImages.pull")}
              </Button>
            </div>
          </div>
        </DialogShell>
      )}

      {createOpen && (
        <DialogShell title={t("operations.dockerImages.createContainerTitle")} onClose={() => !actionBusy && setCreateOpen(null)} closeDisabled={actionBusy} size="form">
          <div className="space-y-3">
            <p className="truncate text-sm text-[hsl(var(--secondary))]" title={createOpen.name}>
              {t("operations.dockerImages.createContainerImage", { image: createOpen.name })}
            </p>
            <div className="space-y-1.5">
              <Label htmlFor="docker-create-name">{t("operations.dockerImages.containerName")}</Label>
              <Input
                id="docker-create-name"
                value={containerName}
                placeholder={t("operations.dockerImages.containerNamePlaceholder")}
                onChange={(event) => setContainerName(event.target.value)}
                disabled={actionBusy}
              />
            </div>
            <div className="space-y-1.5">
              <Label htmlFor="docker-create-ports">{t("operations.dockerImages.publishPorts")}</Label>
              <Input
                id="docker-create-ports"
                value={publishPorts}
                placeholder={t("operations.dockerImages.publishPortsPlaceholder")}
                onChange={(event) => setPublishPorts(event.target.value)}
                disabled={actionBusy}
              />
            </div>
            {actionFailed && failureMessage}
            <div className="flex justify-end gap-2">
              <Button type="button" variant="ghost" disabled={actionBusy} onClick={() => setCreateOpen(null)}>{t("common.cancel")}</Button>
              <Button
                type="button"
                disabled={actionBusy || !containerName.trim()}
                onClick={() => void runAction({
                  type: "createContainer",
                  image: createOpen.name === "<none>" ? createOpen.id : createOpen.name,
                  name: containerName.trim(),
                  publishPorts: parsePublishPorts(publishPorts),
                })}
              >
                {t("operations.dockerImages.createContainer")}
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
              ? t("operations.dockerImages.confirmPrune")
              : confirm.kind === "bulkRemove"
                ? t("operations.dockerImages.confirmBulkDelete", { count: confirm.ids.length })
                : t("operations.confirmDescription", {
                    action: t("operations.dockerImages.delete"),
                    target: confirm.label,
                  })}
          </p>
          {actionFailed && <div className="mt-3">{failureMessage}</div>}
          <div className="mt-5 flex justify-end gap-2">
            <Button type="button" variant="ghost" disabled={actionBusy} onClick={() => setConfirm(null)}>{t("common.cancel")}</Button>
            <Button
              type="button"
              variant={confirm.kind === "prune" ? "default" : "danger"}
              disabled={actionBusy}
              onClick={() => {
                if (confirm.kind === "prune") void runAction({ type: "prune" });
                else void runAction({ type: "remove", ids: confirm.ids });
              }}
            >
              {confirm.kind === "prune" ? t("operations.dockerImages.prune") : t("operations.dockerImages.delete")}
            </Button>
          </div>
        </DialogShell>
      )}
    </div>
  );
}
