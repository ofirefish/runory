import { ArrowLeft, Folder, FolderPlus, RefreshCw, Search } from "lucide-react";
import { useCallback, useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "../../components/ui/button";
import { Dialog, DialogContent, DialogHeader, DialogTitle } from "../../components/ui/dialog";
import { Input } from "../../components/ui/input";
import { Label } from "../../components/ui/label";
import { createRemoteDirectory, listDirectory, openSftp } from "../../lib/tauri/ssh";
import type { SftpDirectory, SftpEntry } from "../../types/session";
import { breadcrumbPaths } from "./sftp-format";

function parentPath(path: string): string | null {
  if (!path || path === "/") return null;
  const trimmed = path.replace(/\/+$/, "");
  const index = trimmed.lastIndexOf("/");
  if (index <= 0) return "/";
  return trimmed.slice(0, index);
}

function NameDialog({
  title,
  label,
  onSave,
  onClose,
}: {
  title: string;
  label: string;
  onSave: (name: string) => Promise<void>;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const [name, setName] = useState("");
  const [busy, setBusy] = useState(false);
  const [failed, setFailed] = useState(false);
  const save = async () => {
    if (!name.trim()) return;
    setBusy(true);
    setFailed(false);
    try {
      await onSave(name.trim());
      onClose();
    } catch {
      setFailed(true);
      setBusy(false);
    }
  };
  return (
    <Dialog open onOpenChange={(open) => { if (!open && !busy) onClose(); }}>
      <DialogContent closeDisabled={busy} onInteractOutside={(event) => { if (busy) event.preventDefault(); }} onEscapeKeyDown={(event) => { if (busy) event.preventDefault(); }}>
      <DialogHeader><DialogTitle>{title}</DialogTitle></DialogHeader>
      <label className="grid gap-2 text-sm">
        <span>{label}</span>
        <Input
          autoFocus
          value={name}
          maxLength={255}
          onChange={(event) => setName(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === "Enter") void save();
          }}
        />
      </label>
      {failed && <p className="mt-3 text-sm text-red-500">{t("files.operationFailed")}</p>}
      <div className="mt-5 flex justify-end gap-2">
        <Button type="button" variant="ghost" disabled={busy} onClick={onClose}>
          {t("common.cancel")}
        </Button>
        <Button type="button" disabled={busy || !name.trim()} onClick={() => void save()}>
          {t("common.save")}
        </Button>
      </div>
      </DialogContent>
    </Dialog>
  );
}

export function RemoteFolderPickerDialog({
  sessionId,
  initialPath,
  titleKey = "remoteFolderPicker.title",
  allowCreateDirectory = true,
  onClose,
  onSelect,
}: {
  sessionId: string;
  initialPath?: string;
  titleKey?: string;
  allowCreateDirectory?: boolean;
  onClose: () => void;
  onSelect: (absolutePath: string) => void;
}) {
  const { t, i18n } = useTranslation();
  const [directory, setDirectory] = useState<SftpDirectory | null>(null);
  const [selectedPath, setSelectedPath] = useState("/");
  const [query, setQuery] = useState("");
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState(false);
  const [mkdirOpen, setMkdirOpen] = useState(false);

  const load = useCallback(
    async (path: string, selectPath = path) => {
      setLoading(true);
      setError(false);
      try {
        await openSftp(sessionId);
        const next = await listDirectory(sessionId, path);
        setDirectory(next);
        setSelectedPath(selectPath || next.path);
      } catch {
        setError(true);
        setDirectory(null);
      } finally {
        setLoading(false);
      }
    },
    [sessionId],
  );

  useEffect(() => {
    const start = initialPath?.trim();
    void (async () => {
      if (start && start.startsWith("/") && !start.split("/").includes("..")) {
        try {
          await load(start);
          return;
        } catch {
          // Fall through to home/root.
        }
      }
      try {
        setLoading(true);
        const opened = await openSftp(sessionId);
        await load(opened.path);
      } catch {
        setError(true);
        setLoading(false);
      }
    })();
  }, [initialPath, load, sessionId]);

  const folders = useMemo(() => {
    const entries = directory?.entries ?? [];
    const onlyFolders = entries.filter((entry) => entry.kind === "directory");
    const needle = query.trim().toLowerCase();
    if (!needle) return onlyFolders;
    return onlyFolders.filter((entry) => entry.name.toLowerCase().includes(needle));
  }, [directory, query]);

  const goUp = () => {
    if (!directory) return;
    const parent = parentPath(directory.path);
    if (parent) void load(parent);
  };

  const confirm = () => {
    if (!selectedPath.startsWith("/") || selectedPath.split("/").includes("..")) return;
    onSelect(selectedPath.replace(/\/+$/, "") || "/");
    onClose();
  };

  return (
    <>
      <Dialog open onOpenChange={(open) => { if (!open) onClose(); }}>
        <DialogContent className="gap-3 overflow-y-auto sm:max-w-4xl">
        <DialogHeader><DialogTitle>{t(titleKey)}</DialogTitle></DialogHeader>
        <div className="flex flex-wrap items-center gap-2">
          <Button
            type="button"
            variant="ghost"
            size="icon"
            disabled={loading || !directory || directory.path === "/"}
            aria-label={t("remoteFolderPicker.up")}
            onClick={goUp}
          >
            <ArrowLeft size={16} />
          </Button>
          <nav className="flex min-w-0 flex-1 flex-wrap items-center gap-1 text-sm" aria-label={t("remoteFolderPicker.breadcrumbs")}>
            {directory
              ? breadcrumbPaths(directory.path).map((crumb, index) => (
                  <span key={crumb.path} className="flex min-w-0 items-center gap-1">
                    {index > 0 && <span className="text-[hsl(var(--muted))]">/</span>}
                    <button
                      type="button"
                      className="truncate rounded px-1 text-[hsl(var(--secondary))] hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[hsl(var(--primary))]"
                      disabled={loading}
                      onClick={() => void load(crumb.path)}
                    >
                      {crumb.label === "/" ? t("remoteFolderPicker.root") : crumb.label}
                    </button>
                  </span>
                ))
              : null}
          </nav>
          <Button
            type="button"
            variant="ghost"
            size="icon"
            disabled={loading || !directory}
            aria-label={t("common.refresh")}
            onClick={() => directory && void load(directory.path, selectedPath)}
          >
            <RefreshCw size={16} className={loading ? "animate-spin" : undefined} />
          </Button>
          <div className="relative min-w-[12rem] flex-1 sm:max-w-xs">
            <Search size={14} className="pointer-events-none absolute left-2.5 top-1/2 -translate-y-1/2 text-[hsl(var(--muted))]" />
            <Input
              className="h-8 pl-8"
              value={query}
              placeholder={t("remoteFolderPicker.search")}
              onChange={(event) => setQuery(event.target.value)}
            />
          </div>
        </div>

        {allowCreateDirectory && (
          <div>
            <Button type="button" variant="ghost" size="sm" disabled={loading || !directory} onClick={() => setMkdirOpen(true)}>
              <FolderPlus size={15} />
              {t("files.newFolder")}
            </Button>
          </div>
        )}

        {error && (
          <div className="rounded-lg border border-red-500/30 bg-red-500/10 p-3 text-sm text-red-500">
            {t("remoteFolderPicker.loadFailed")}
          </div>
        )}

        <div className="min-h-[18rem] flex-1 overflow-auto rounded-lg border">
          <table className="w-full text-left text-sm">
            <thead className="sticky top-0 bg-[hsl(var(--elevated))] text-xs text-[hsl(var(--secondary))]">
              <tr>
                <th className="px-3 py-2 font-medium">{t("remoteFolderPicker.name")}</th>
                <th className="px-3 py-2 font-medium">{t("remoteFolderPicker.modified")}</th>
              </tr>
            </thead>
            <tbody>
              {folders.map((entry: SftpEntry) => {
                const active = selectedPath === entry.path;
                return (
                  <tr
                    key={entry.path}
                    className={`border-t hover:bg-[hsl(var(--elevated))]/70 ${active ? "bg-[hsl(var(--primary)/.12)]" : ""}`}
                  >
                    <td className="px-3 py-1.5">
                      <button
                        type="button"
                        className="flex w-full items-center gap-2 rounded px-1 py-1.5 text-left focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[hsl(var(--primary))]"
                        onClick={() => setSelectedPath(entry.path)}
                        onDoubleClick={() => void load(entry.path)}
                      >
                        <Folder size={16} className="shrink-0 text-blue-500" aria-hidden />
                        <span className="truncate">{entry.name}</span>
                      </button>
                    </td>
                    <td className="whitespace-nowrap px-3 py-1.5 text-xs text-[hsl(var(--secondary))]">
                      {entry.modified == null
                        ? "—"
                        : new Intl.DateTimeFormat(i18n.language, { dateStyle: "medium", timeStyle: "short" }).format(entry.modified * 1000)}
                    </td>
                  </tr>
                );
              })}
              {!loading && folders.length === 0 && (
                <tr>
                  <td colSpan={2} className="px-3 py-8 text-center text-sm text-[hsl(var(--secondary))]">
                    {t("remoteFolderPicker.empty")}
                  </td>
                </tr>
              )}
            </tbody>
          </table>
        </div>

        <div className="grid gap-1.5">
          <Label htmlFor="remote-folder-selected">{t("remoteFolderPicker.selected")}</Label>
          <Input id="remote-folder-selected" readOnly className="font-mono text-xs" value={selectedPath} />
        </div>

        <div className="flex justify-end gap-2">
          <Button type="button" variant="ghost" onClick={onClose}>
            {t("common.cancel")}
          </Button>
          <Button type="button" disabled={!selectedPath || loading || error} onClick={confirm}>
            {t("remoteFolderPicker.confirm")}
          </Button>
        </div>
        </DialogContent>
      </Dialog>

      {mkdirOpen && directory && (
        <NameDialog
          title={t("files.newFolder")}
          label={t("files.folderName")}
          onClose={() => setMkdirOpen(false)}
          onSave={async (name) => {
            await createRemoteDirectory(sessionId, directory.path, name);
            await load(directory.path, `${directory.path.replace(/\/$/, "")}/${name}`);
          }}
        />
      )}
    </>
  );
}
