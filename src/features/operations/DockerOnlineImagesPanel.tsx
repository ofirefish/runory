import { RefreshCw, Search } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { Button } from "../../components/ui/button";
import { DialogShell } from "../../components/ui/dialog-shell";
import { Input } from "../../components/ui/input";
import { Label } from "../../components/ui/label";
import { SelectControl } from "../../components/ui/select-control";
import { searchDockerImages } from "../../lib/tauri/infrastructure";
import { cn } from "../../lib/utils";
import type { DockerImage, DockerImageAction, DockerOnlineImage } from "../../types/infrastructure";

const PAGE_SIZE_OPTIONS = ["10", "20", "50"] as const;
const SEARCH_LIMIT = 100;
const OFFICIAL_LIMIT = 300;
const FALLBACK_TAG = "latest";

function parsePublishPorts(value: string): string[] {
  return value
    .split(/[\s,]+/)
    .map((item) => item.trim())
    .filter(Boolean);
}

function tagsForImage(item: DockerOnlineImage): string[] {
  return item.tags?.length ? item.tags : [FALLBACK_TAG];
}

function imageReference(name: string, tag: string): string {
  return `${name}:${tag}`;
}

function matchLocalImagesForTag(images: DockerImage[], onlineName: string, tag: string): DockerImage[] {
  return matchLocalImages(images, onlineName).filter((image) => {
    const imageTag = image.name.includes(":") ? image.name.slice(image.name.indexOf(":") + 1) : FALLBACK_TAG;
    return imageTag === tag;
  });
}

/** Match Hub search name to local `repository:tag` (mirrors / library/ prefix). */
export function matchLocalImages(images: DockerImage[], onlineName: string): DockerImage[] {
  const needle = onlineName.trim().toLowerCase();
  if (!needle) return [];
  return images.filter((image) => {
    const repo = image.name.split(":")[0]?.trim().toLowerCase() ?? "";
    if (!repo || repo === "<none>") return false;
    if (repo === needle) return true;
    if (!needle.includes("/") && (repo === `library/${needle}` || repo.endsWith(`/${needle}`))) return true;
    if (!repo.includes("/") && needle === `library/${repo}`) return true;
    return false;
  });
}

function pageWindow(current: number, total: number): number[] {
  if (total <= 7) return Array.from({ length: total }, (_, index) => index + 1);
  const start = Math.max(1, Math.min(current - 2, total - 4));
  const end = Math.min(total, start + 4);
  return Array.from({ length: end - start + 1 }, (_, index) => start + index);
}

export function DockerOnlineImagesPanel({
  sessionId,
  images,
  busy = false,
  onAction,
  onEnsureLocalImages,
}: {
  sessionId: string;
  images: DockerImage[];
  busy?: boolean;
  onAction: (action: DockerImageAction) => Promise<void>;
  onEnsureLocalImages?: () => void;
}) {
  const { t } = useTranslation();
  const [draftQuery, setDraftQuery] = useState("");
  const [activeQuery, setActiveQuery] = useState("");
  const [results, setResults] = useState<DockerOnlineImage[]>([]);
  const [selectedTags, setSelectedTags] = useState<Record<string, string>>({});
  const [searched, setSearched] = useState(false);
  const [searchBusy, setSearchBusy] = useState(true);
  const [searchFailed, setSearchFailed] = useState(false);
  const [page, setPage] = useState(1);
  const [pageSize, setPageSize] = useState<(typeof PAGE_SIZE_OPTIONS)[number]>("10");
  const [jumpPage, setJumpPage] = useState("1");
  const [createOpen, setCreateOpen] = useState<string | null>(null);
  const [containerName, setContainerName] = useState("");
  const [publishPorts, setPublishPorts] = useState("");
  const [confirmDelete, setConfirmDelete] = useState<{ ids: string[]; label: string } | null>(null);
  const [actionBusy, setActionBusy] = useState(false);
  const [actionFailed, setActionFailed] = useState(false);
  const [actionErrorDetail, setActionErrorDetail] = useState("");

  useEffect(() => {
    onEnsureLocalImages?.();
  }, [onEnsureLocalImages]);

  useEffect(() => {
    void runSearch("");
    // Initial official catalog load for this session context.
    // eslint-disable-next-line react-hooks/exhaustive-deps -- mount / session change only
  }, [sessionId]);

  const pageSizeNumber = Number(pageSize);
  const totalPages = Math.max(1, Math.ceil(results.length / pageSizeNumber));
  const currentPage = Math.min(page, totalPages);
  const pageItems = results.slice((currentPage - 1) * pageSizeNumber, currentPage * pageSizeNumber);
  const pageNumbers = useMemo(() => pageWindow(currentPage, totalPages), [currentPage, totalPages]);

  useEffect(() => {
    setPage(1);
  }, [activeQuery, pageSize, results.length]);

  useEffect(() => {
    setJumpPage(String(currentPage));
  }, [currentPage]);

  const runSearch = async (query: string) => {
    const trimmed = query.trim();
    setSearchBusy(true);
    setSearchFailed(false);
    try {
      const next = await searchDockerImages(
        sessionId,
        trimmed,
        trimmed ? SEARCH_LIMIT : OFFICIAL_LIMIT,
      );
      const tags: Record<string, string> = {};
      for (const item of next) {
        tags[item.name] = tagsForImage(item)[0] ?? FALLBACK_TAG;
      }
      setResults(next);
      setSelectedTags(tags);
      setActiveQuery(trimmed);
      setSearched(true);
    } catch {
      // Default official catalog should not surface a hard failure banner.
      if (!trimmed) {
        setSearchFailed(false);
        setResults([]);
        setSelectedTags({});
        setActiveQuery("");
        setSearched(false);
      } else {
        setSearchFailed(true);
        setResults([]);
        setSelectedTags({});
        setActiveQuery(trimmed);
        setSearched(true);
      }
    } finally {
      setSearchBusy(false);
    }
  };

  const runAction = async (action: DockerImageAction, successKey?: string) => {
    setActionBusy(true);
    setActionFailed(false);
    setActionErrorDetail("");
    try {
      await onAction(action);
      if (successKey) toast.success(t(successKey));
      setCreateOpen(null);
      setConfirmDelete(null);
      if (searched) await runSearch(activeQuery);
    } catch (error) {
      setActionFailed(true);
      setActionErrorDetail(error instanceof Error ? error.message : String(error));
    } finally {
      setActionBusy(false);
    }
  };

  const disabled = busy || searchBusy || actionBusy;

  return (
    <div className="flex h-full min-h-0 flex-col gap-3">
      <form
        className="flex shrink-0 flex-wrap items-center gap-2"
        onSubmit={(event) => {
          event.preventDefault();
          const form = new FormData(event.currentTarget);
          const query = String(form.get("query") ?? draftQuery);
          setDraftQuery(query);
          void runSearch(query);
        }}
      >
        <Input
          name="query"
          value={draftQuery}
          onChange={(event) => setDraftQuery(event.target.value)}
          placeholder={t("operations.dockerOnlineImages.searchPlaceholder")}
          aria-label={t("operations.dockerOnlineImages.searchPlaceholder")}
          className="min-w-56 flex-1"
          disabled={disabled}
        />
        <Button type="submit" size="sm" disabled={disabled}>
          <Search size={14} />
          {t("operations.dockerOnlineImages.search")}
        </Button>
        <Button
          type="button"
          variant="ghost"
          size="icon"
          className="h-8 w-8"
          disabled={disabled || !searched}
          aria-label={t("operations.dockerOnlineImages.refresh")}
          onClick={() => void runSearch(activeQuery)}
        >
          <RefreshCw size={14} className={searchBusy ? "animate-spin" : undefined} />
        </Button>
      </form>

      {searchFailed && (
        <div className="shrink-0 rounded-lg border border-red-500/30 bg-red-500/10 p-3 text-sm text-red-500">
          {t("operations.dockerOnlineImages.searchFailed")}
        </div>
      )}

      <div className="min-h-0 flex-1 overflow-auto rounded-lg border">
        <table className="w-full text-left text-sm">
          <thead className="sticky top-0 z-[1] bg-[hsl(var(--elevated))] text-xs">
            <tr>
              <th className="px-3 py-2">{t("operations.dockerOnlineImages.name")}</th>
              <th className="w-40 px-3 py-2">{t("operations.dockerOnlineImages.version")}</th>
              <th className="w-24 px-3 py-2">{t("operations.dockerOnlineImages.stars")}</th>
              <th className="w-24 px-3 py-2">{t("operations.dockerOnlineImages.source")}</th>
              <th className="px-3 py-2">{t("operations.dockerOnlineImages.description")}</th>
              <th className="w-56 px-3 py-2 text-right">{t("operations.actions")}</th>
            </tr>
          </thead>
          <tbody>
            {!searched ? (
              <tr>
                <td colSpan={6} className="px-3 py-8 text-center text-sm text-[hsl(var(--muted))]">
                  {t("operations.dockerOnlineImages.prompt")}
                </td>
              </tr>
            ) : pageItems.length === 0 ? (
              <tr>
                <td colSpan={6} className="px-3 py-8 text-center text-sm text-[hsl(var(--muted))]">
                  {t("operations.dockerOnlineImages.empty")}
                </td>
              </tr>
            ) : (
              pageItems.map((item) => {
                const tagOptions = tagsForImage(item);
                const tag = selectedTags[item.name] ?? tagOptions[0] ?? FALLBACK_TAG;
                const reference = imageReference(item.name, tag);
                const local = matchLocalImagesForTag(images, item.name, tag);
                const isLocal = local.length > 0;
                return (
                  <tr key={item.name} className="border-t">
                    <td className="px-3 py-2 font-medium">{item.name}</td>
                    <td className="px-3 py-2">
                      <SelectControl
                        value={tag}
                        onValueChange={(next) =>
                          setSelectedTags((prev) => ({ ...prev, [item.name]: next }))
                        }
                        label={t("operations.dockerOnlineImages.version")}
                        className="h-8 w-36"
                        disabled={disabled}
                        options={tagOptions.map((value) => ({ value, label: value }))}
                      />
                    </td>
                    <td className="px-3 py-2 tabular-nums">{item.starCount}</td>
                    <td className="px-3 py-2">
                      {item.isOfficial
                        ? t("operations.dockerOnlineImages.sourceOfficial")
                        : t("operations.dockerOnlineImages.sourceCommunity")}
                    </td>
                    <td className="max-w-md truncate px-3 py-2 text-[hsl(var(--secondary))]" title={item.description || undefined}>
                      {item.description || "—"}
                    </td>
                    <td className="px-3 py-2 text-right">
                      <div className="inline-flex flex-wrap items-center justify-end gap-x-1 text-xs">
                        <button
                          type="button"
                          className="text-[hsl(var(--primary))] hover:underline disabled:opacity-50"
                          disabled={disabled}
                          onClick={() => {
                            setActionFailed(false);
                            setContainerName("");
                            setPublishPorts("");
                            setCreateOpen(reference);
                          }}
                        >
                          {t("operations.dockerOnlineImages.createContainer")}
                        </button>
                        <span className="text-[hsl(var(--muted))]">|</span>
                        {isLocal ? (
                          <>
                            <button
                              type="button"
                              className="text-[hsl(var(--primary))] hover:underline disabled:opacity-50"
                              disabled={disabled}
                              onClick={() =>
                                void runAction(
                                  { type: "pull", reference },
                                  "operations.dockerOnlineImages.updateSuccess",
                                )
                              }
                            >
                              {t("operations.dockerOnlineImages.update")}
                            </button>
                            <span className="text-[hsl(var(--muted))]">|</span>
                            <button
                              type="button"
                              className="text-[hsl(var(--primary))] hover:underline disabled:opacity-50"
                              disabled={disabled}
                              onClick={() => {
                                setActionFailed(false);
                                setConfirmDelete({
                                  ids: local.map((image) => image.id),
                                  label: reference,
                                });
                              }}
                            >
                              {t("operations.dockerOnlineImages.delete")}
                            </button>
                          </>
                        ) : (
                          <button
                            type="button"
                            className="text-[hsl(var(--primary))] hover:underline disabled:opacity-50"
                            disabled={disabled}
                            onClick={() =>
                              void runAction(
                                { type: "pull", reference },
                                "operations.dockerOnlineImages.pullSuccess",
                              )
                            }
                          >
                            {t("operations.dockerOnlineImages.pull")}
                          </button>
                        )}
                      </div>
                    </td>
                  </tr>
                );
              })
            )}
          </tbody>
        </table>
      </div>

      <div className="flex shrink-0 flex-wrap items-center justify-end gap-2 text-xs text-[hsl(var(--secondary))]">
        <Button type="button" variant="ghost" size="sm" disabled={currentPage <= 1 || !searched} onClick={() => setPage(currentPage - 1)}>‹</Button>
        {searched && pageNumbers.map((value) => (
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
        <Button type="button" variant="ghost" size="sm" disabled={currentPage >= totalPages || !searched} onClick={() => setPage(currentPage + 1)}>›</Button>
        <SelectControl
          value={pageSize}
          onValueChange={setPageSize}
          label={t("operations.dockerOnlineImages.pageSize")}
          className="w-28"
          options={PAGE_SIZE_OPTIONS.map((value) => ({
            value,
            label: t("operations.dockerOnlineImages.pageSizeOption", { count: Number(value) }),
          }))}
        />
        <span>{t("operations.dockerOnlineImages.total", { count: searched ? results.length : 0 })}</span>
        <span className="inline-flex items-center gap-1">
          {t("operations.dockerOnlineImages.goto")}
          <Input
            type="number"
            min={1}
            max={totalPages}
            value={jumpPage}
            disabled={!searched}
            onChange={(event) => setJumpPage(event.target.value)}
            onKeyDown={(event) => {
              if (event.key !== "Enter") return;
              const next = Number(jumpPage);
              if (Number.isFinite(next)) setPage(Math.min(totalPages, Math.max(1, Math.trunc(next))));
            }}
            className="h-7 w-14"
            aria-label={t("operations.dockerOnlineImages.goto")}
          />
          {t("operations.dockerOnlineImages.page")}
        </span>
      </div>

      {createOpen && (
        <DialogShell
          title={t("operations.dockerOnlineImages.createContainerTitle")}
          onClose={() => !actionBusy && setCreateOpen(null)}
          closeDisabled={actionBusy}
          size="form"
        >
          <div className="space-y-3">
            <p className="text-sm text-[hsl(var(--secondary))]">
              {t("operations.dockerOnlineImages.createContainerImage", { image: createOpen })}
            </p>
            <div className="space-y-1.5">
              <Label htmlFor="docker-online-create-name">{t("operations.dockerOnlineImages.containerName")}</Label>
              <Input
                id="docker-online-create-name"
                value={containerName}
                onChange={(event) => setContainerName(event.target.value)}
                placeholder={t("operations.dockerOnlineImages.containerNamePlaceholder")}
                disabled={actionBusy}
              />
            </div>
            <div className="space-y-1.5">
              <Label htmlFor="docker-online-create-ports">{t("operations.dockerOnlineImages.publishPorts")}</Label>
              <Input
                id="docker-online-create-ports"
                value={publishPorts}
                onChange={(event) => setPublishPorts(event.target.value)}
                placeholder={t("operations.dockerOnlineImages.publishPortsPlaceholder")}
                disabled={actionBusy}
              />
            </div>
            {actionFailed && (
              <p className="text-sm text-red-500">
                {t("operations.error")}
                {actionErrorDetail ? ` ${actionErrorDetail}` : ""}
              </p>
            )}
            <div className="flex justify-end gap-2">
              <Button type="button" variant="ghost" disabled={actionBusy} onClick={() => setCreateOpen(null)}>
                {t("common.cancel")}
              </Button>
              <Button
                type="button"
                disabled={actionBusy || !containerName.trim()}
                onClick={() =>
                  void runAction(
                    {
                      type: "createContainer",
                      image: createOpen,
                      name: containerName.trim(),
                      publishPorts: parsePublishPorts(publishPorts),
                    },
                    "operations.dockerOnlineImages.createSuccess",
                  )
                }
              >
                {t("operations.dockerOnlineImages.createContainer")}
              </Button>
            </div>
          </div>
        </DialogShell>
      )}

      {confirmDelete && (
        <DialogShell
          title={t("operations.confirmTitle")}
          onClose={() => !actionBusy && setConfirmDelete(null)}
          closeDisabled={actionBusy}
          size="form"
        >
          <p className="text-sm text-[hsl(var(--secondary))]">
            {t("operations.dockerOnlineImages.confirmDelete", { name: confirmDelete.label })}
          </p>
          {actionFailed && (
            <p className="mt-3 text-sm text-red-500">
              {t("operations.error")}
              {actionErrorDetail ? ` ${actionErrorDetail}` : ""}
            </p>
          )}
          <div className="mt-5 flex justify-end gap-2">
            <Button type="button" variant="ghost" disabled={actionBusy} onClick={() => setConfirmDelete(null)}>
              {t("common.cancel")}
            </Button>
            <Button
              type="button"
              variant="danger"
              disabled={actionBusy}
              onClick={() =>
                void runAction(
                  { type: "remove", ids: confirmDelete.ids },
                  "operations.dockerOnlineImages.deleteSuccess",
                )
              }
            >
              {t("operations.dockerOnlineImages.delete")}
            </Button>
          </div>
        </DialogShell>
      )}
    </div>
  );
}
