import { Ban, ChevronRight, Download, Eye, FileQuestion, Folder, FolderClock, FolderPlus, Link, Pencil, RefreshCw, RotateCcw, Trash2, Upload, X } from "lucide-react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "../../components/ui/button";
import { ConfirmDialog } from "../../components/ui/confirm-dialog";
import { ContextMenu } from "../../components/ui/context-menu";
import { DialogShell } from "../../components/ui/dialog-shell";
import { Input } from "../../components/ui/input";
import { appErrorCode } from "../../lib/app-error";
import { acceptLatestUploadDrop, cancelTransfer, changeDirectory, createRemoteDirectory, deleteRemoteEntry, listTransfers, listUploadDirectories, openSftp, refreshDirectory, renameRemoteEntry, retryTransfer, selectDownloadTarget, selectUploadFiles, startDownload, startUpload, subscribeTransfers } from "../../lib/tauri/ssh";
import type { LocalFileSelection, SftpDirectory, SftpEntry, SftpEntryKind, TransferJob, UploadDirectoryHistoryEntry } from "../../types/session";
import { FileTypeIcon } from "./FileTypeIcon";
import { RemoteImagePreview, type RemoteFileRequest } from "./RemoteImagePreview";
import { RemoteTextPreview } from "./RemoteTextPreview";
import { clearLastRemotePath, readLastRemotePath, saveLastRemotePath } from "./remote-path-history";
import { breadcrumbPaths, formatFileSize, formatPermissions } from "./sftp-format";

const entryIcon = (kind: SftpEntryKind, name: string) => {
  if (kind === "directory") return <Folder aria-hidden size={17} className="text-blue-500" />;
  if (kind === "symlink") return <Link aria-hidden size={17} className="text-violet-500" />;
  if (kind === "file") return <FileTypeIcon fileName={name} />;
  return <FileQuestion aria-hidden size={17} className="text-[hsl(var(--muted))]" />;
};

function NameDialog({ title, label, initial = "", onSave, onClose }: { title: string; label: string; initial?: string; onSave: (name: string) => Promise<void>; onClose: () => void }) {
  const { t } = useTranslation();
  const [name, setName] = useState(initial);
  const [busy, setBusy] = useState(false);
  const [failed, setFailed] = useState(false);
  const save = async () => {
    if (!name.trim()) return;
    setBusy(true); setFailed(false);
    try { await onSave(name.trim()); onClose(); } catch { setFailed(true); setBusy(false); }
  };
  return <DialogShell title={title} onClose={onClose}>
    <label className="grid gap-2 text-sm"><span>{label}</span><Input autoFocus value={name} maxLength={255} onChange={(event) => setName(event.target.value)} onKeyDown={(event) => { if (event.key === "Enter") void save(); }} /></label>
    {failed && <p className="mt-3 text-sm text-red-500">{t("files.operationFailed")}</p>}
    <div className="mt-5 flex justify-end gap-2"><Button variant="ghost" onClick={onClose}>{t("common.cancel")}</Button><Button disabled={busy || !name.trim()} onClick={() => void save()}>{t("common.save")}</Button></div>
  </DialogShell>;
}

function RemotePathBar({ path, loading, uploadHistory, uploadHistoryLoading, onNavigate, onOpenUploadHistory, onSelectUploadDirectory }: {
  path: string;
  loading: boolean;
  uploadHistory: UploadDirectoryHistoryEntry[];
  uploadHistoryLoading: boolean;
  onNavigate: (path: string) => void;
  onOpenUploadHistory: () => void;
  onSelectUploadDirectory: (remoteDirectory: string) => void;
}) {
  const { t, i18n } = useTranslation();
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState(path);
  const [historyOpen, setHistoryOpen] = useState(false);
  const historyRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!editing) setDraft(path);
  }, [editing, path]);

  useEffect(() => {
    if (!historyOpen) return;
    const close = (event: PointerEvent) => {
      if (!historyRef.current?.contains(event.target as Node)) setHistoryOpen(false);
    };
    window.addEventListener("pointerdown", close);
    return () => window.removeEventListener("pointerdown", close);
  }, [historyOpen]);

  const startEditing = () => {
    if (loading) return;
    setDraft(path);
    setEditing(true);
  };
  const submit = () => {
    const nextPath = draft.trim();
    if (!nextPath) return;
    setEditing(false);
    if (nextPath !== path) onNavigate(nextPath);
  };

  const crumbs = breadcrumbPaths(path);
  return <div className="relative flex min-w-0 flex-1 items-center">
    {editing ? <Input
      autoFocus
      autoCapitalize="none"
      aria-label={t("files.pathInput")}
      className="h-8 min-w-0 flex-1 font-mono"
      disabled={loading}
      spellCheck={false}
      value={draft}
      onBlur={() => setEditing(false)}
      onChange={(event) => setDraft(event.target.value)}
      onFocus={(event) => event.currentTarget.select()}
      onKeyDown={(event) => {
        if (event.key === "Enter") {
          event.preventDefault();
          submit();
        } else if (event.key === "Escape") {
          event.preventDefault();
          setEditing(false);
        }
      }}
    /> : <nav
      className="flex min-w-0 flex-1 cursor-text items-center overflow-x-auto font-mono text-sm"
      aria-label={t("files.breadcrumb")}
      onClick={startEditing}
    >
      {crumbs.map((crumb, index) => {
        const current = index === crumbs.length - 1;
        return <span key={crumb.path} className="flex shrink-0 items-center">
          {index > 0 && <ChevronRight aria-hidden size={14} className="mx-0.5 text-[hsl(var(--muted))]" />}
          <button
            type="button"
            className="rounded px-1.5 py-1 hover:bg-[hsl(var(--elevated))] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blue-500"
            aria-label={current ? t("files.editPath") : undefined}
            title={current ? t("files.editPath") : undefined}
            onClick={(event) => {
              event.stopPropagation();
              if (current) startEditing();
              else onNavigate(crumb.path);
            }}
          >
            {crumb.label}
          </button>
        </span>;
      })}
    </nav>}
    <div ref={historyRef} className="relative ml-1 shrink-0">
      <button
        type="button"
        className="grid h-8 w-8 place-items-center rounded hover:bg-[hsl(var(--elevated))] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blue-500 disabled:cursor-not-allowed disabled:opacity-40"
        aria-label={t("files.uploadHistory")}
        aria-expanded={historyOpen}
        title={t("files.uploadHistory")}
        disabled={loading}
        onClick={(event) => {
          event.stopPropagation();
          const nextOpen = !historyOpen;
          setHistoryOpen(nextOpen);
          if (nextOpen) onOpenUploadHistory();
        }}
      >
        <FolderClock aria-hidden size={16} />
      </button>
      {historyOpen && <div className="absolute right-0 top-full z-40 mt-1 w-80 overflow-hidden rounded-md border border-[hsl(var(--border))] bg-[hsl(var(--surface))] p-1 shadow-lg" role="menu" aria-label={t("files.uploadHistory")}>
        {uploadHistoryLoading && <p className="px-3 py-2 text-sm text-[hsl(var(--muted))]">{t("common.loading")}</p>}
        {!uploadHistoryLoading && uploadHistory.length === 0 && <p className="px-3 py-2 text-sm text-[hsl(var(--muted))]">{t("files.noUploadHistory")}</p>}
        {!uploadHistoryLoading && uploadHistory.map((entry) => <button
          key={entry.remoteDirectory}
          type="button"
          role="menuitem"
          className="flex w-full min-w-0 items-center gap-2 rounded px-2.5 py-2 text-left hover:bg-[hsl(var(--elevated))] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blue-500"
          title={t("files.openUploadDirectory", { path: entry.remoteDirectory })}
          onClick={() => {
            setHistoryOpen(false);
            onSelectUploadDirectory(entry.remoteDirectory);
          }}
        >
          <Folder aria-hidden size={15} className="shrink-0 text-blue-500" />
          <span className="min-w-0 flex-1 truncate font-mono text-xs">{entry.remoteDirectory}</span>
          {entry.lastUploadedAtMs > 0 && <span className="shrink-0 text-[10px] text-[hsl(var(--muted))]">{new Intl.DateTimeFormat(i18n.language, { dateStyle: "short", timeStyle: "short" }).format(entry.lastUploadedAtMs)}</span>}
        </button>)}
      </div>}
    </div>
  </div>;
}

function TransferQueue({ sessionId, jobs, onCancel, onRetry, onClose }: { sessionId: string; jobs: TransferJob[]; onCancel: (id: string) => void; onRetry: (id: string, overwrite: boolean) => void; onClose: () => void }) {
  const { t, i18n } = useTranslation();
  const visible = jobs.filter((job) => job.sessionId === sessionId).slice().reverse();
  return <aside className="flex max-h-64 shrink-0 flex-col border-t bg-[hsl(var(--surface))]" aria-label={t("transfers.title")}>
    <div className="flex h-10 shrink-0 items-center border-b px-3"><h3 className="text-sm font-medium">{t("transfers.title")}</h3><span className="ml-2 rounded-full bg-[hsl(var(--elevated))] px-2 py-0.5 text-xs">{visible.length}</span><Button variant="ghost" size="icon" className="ml-auto h-7 w-7" aria-label={t("transfers.close")} onClick={onClose}><X size={15} /></Button></div>
    <div className="overflow-auto">
      {visible.length === 0 && <div className="px-4 py-6 text-center text-sm text-[hsl(var(--muted))]">{t("transfers.empty")}</div>}
      {visible.map((job) => {
        const percent = job.totalBytes === 0 ? (job.state === "completed" ? 100 : 0) : Math.min(100, Math.round(job.transferredBytes / job.totalBytes * 100));
        return <div key={job.id} className="grid grid-cols-[minmax(0,1fr)_auto] gap-3 border-b border-[hsl(var(--border-soft))] px-4 py-2.5">
          <div className="min-w-0"><div className="flex items-center gap-2 text-sm">{job.direction === "upload" ? <Upload size={14} /> : <Download size={14} />}<span className="truncate">{job.name}</span><span className="ml-auto text-xs text-[hsl(var(--secondary))]">{t(`transfers.state.${job.state}`)}</span></div>
            <div className="mt-1.5 h-1.5 overflow-hidden rounded bg-[hsl(var(--elevated))]"><div className={`h-full ${job.state === "failed" ? "bg-red-500" : job.state === "cancelled" ? "bg-slate-400" : "bg-blue-500"}`} style={{ width: `${percent}%` }} /></div>
            <div className="mt-1 flex justify-between font-mono text-[11px] text-[hsl(var(--muted))]"><span>{percent}%</span><span>{formatFileSize(job.transferredBytes, i18n.language)} / {formatFileSize(job.totalBytes, i18n.language)}</span></div>
            {job.errorCode && <p className="mt-1 text-xs text-red-500">{t(`connection.errors.${job.errorCode}`)}</p>}
          </div>
          <div className="flex items-center gap-1">{(job.state === "queued" || job.state === "running") && <Button variant="ghost" size="icon" className="h-7 w-7" aria-label={t("transfers.cancel")} onClick={() => onCancel(job.id)}><Ban size={14} /></Button>}{(job.state === "failed" || job.state === "cancelled") && <Button variant="ghost" size="icon" className="h-7 w-7" aria-label={t("transfers.retry")} title={job.errorCode === "SFTP_ALREADY_EXISTS" ? t("transfers.retryOverwrite") : t("transfers.retry")} onClick={() => onRetry(job.id, job.errorCode === "SFTP_ALREADY_EXISTS")}><RotateCcw size={14} /></Button>}</div>
        </div>;
      })}
    </div>
  </aside>;
}

const imageFile = (entry: { name: string } | null) => Boolean(entry && /\.(?:png|jpe?g|webp|gif)$/i.test(entry.name));

export function FilesView({ sessionId, profileId, active }: { sessionId: string | null; profileId: string; active: boolean }) {
  const { t, i18n } = useTranslation();
  const [directory, setDirectory] = useState<SftpDirectory | null>(null);
  const [selected, setSelected] = useState<SftpEntry | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState(false);
  const [operationError, setOperationError] = useState<string | null>(null);
  const [dialog, setDialog] = useState<"mkdir" | "rename" | "delete" | null>(null);
  const [jobs, setJobs] = useState<TransferJob[]>([]);
  const [queueOpen, setQueueOpen] = useState(false);
  const [contextMenu, setContextMenu] = useState<{ x: number; y: number } | null>(null);
  const [filePreview, setFilePreview] = useState<RemoteFileRequest | null>(null);
  const [dragUploadActive, setDragUploadActive] = useState(false);
  const [uploadHistory, setUploadHistory] = useState<UploadDirectoryHistoryEntry[]>([]);
  const [uploadHistoryLoading, setUploadHistoryLoading] = useState(false);
  const pendingUploadDirectories = useRef(new Map<string, string>());

  const load = useCallback(async (operation: () => Promise<SftpDirectory>) => {
    setLoading(true); setError(false); setSelected(null);
    try {
      const nextDirectory = await operation();
      setDirectory(nextDirectory);
      saveLastRemotePath(profileId, nextDirectory.path);
    } catch { setError(true); } finally { setLoading(false); }
  }, [profileId]);
  const openInitialDirectory = useCallback(async () => {
    if (!sessionId) throw new Error("SSH session is required");
    const initialDirectory = await openSftp(sessionId);
    const savedPath = readLastRemotePath(profileId);
    if (!savedPath || savedPath === initialDirectory.path) return initialDirectory;
    try {
      return await changeDirectory(sessionId, savedPath);
    } catch {
      clearLastRemotePath(profileId);
      return initialDirectory;
    }
  }, [profileId, sessionId]);
  useEffect(() => { pendingUploadDirectories.current.clear(); setDirectory(null); setSelected(null); setContextMenu(null); setFilePreview(null); setError(false); }, [sessionId]);
  useEffect(() => { if (active && sessionId && !directory && !loading && !error) void load(openInitialDirectory); }, [active, sessionId, directory, loading, error, load, openInitialDirectory]);
  useEffect(() => {
    if (!sessionId) return;
    let current = true;
    void listTransfers(sessionId).then((items) => { if (current) setJobs(items); });
    void subscribeTransfers(sessionId, (event) => { if (!current) return; setJobs((items) => [...items.filter((job) => job.id !== event.data.job.id), event.data.job]); });
    return () => { current = false; };
  }, [sessionId]);

  const startUploads = useCallback(async (files: LocalFileSelection[]) => {
    if (!sessionId || !directory || files.length === 0) return;
    setOperationError(null);
    try {
      const uploadDirectory = directory.path;
      for (const file of files) {
        const job = await startUpload(sessionId, file.grantId, uploadDirectory);
        pendingUploadDirectories.current.set(job.id, uploadDirectory);
        setJobs((items) => items.some((item) => item.id === job.id) ? [...items] : [...items, job]);
      }
      setQueueOpen(true);
    } catch (cause) {
      setOperationError(appErrorCode(cause));
    }
  }, [directory, sessionId]);

  useEffect(() => {
    if (!sessionId || !directory || loading) return;
    let refreshCurrentDirectory = false;
    for (const job of jobs) {
      if (job.direction !== "upload" || job.state !== "completed") continue;
      const uploadDirectory = pendingUploadDirectories.current.get(job.id);
      if (!uploadDirectory) continue;
      pendingUploadDirectories.current.delete(job.id);
      if (uploadDirectory === directory.path) refreshCurrentDirectory = true;
    }
    if (refreshCurrentDirectory) void load(() => refreshDirectory(sessionId));
  }, [directory, jobs, load, loading, sessionId]);

  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    // Security invariant: native paths in the Tauri payload are intentionally ignored.
    // Rust alone turns dropped paths into scoped, single-use upload grants.
    void getCurrentWindow().onDragDropEvent(({ payload }) => {
      if (disposed) return;
      if (payload.type === "enter" || payload.type === "over") {
        setDragUploadActive(active && Boolean(directory) && !loading);
        return;
      }
      const accepted = active && Boolean(directory) && !loading;
      setDragUploadActive(false);
      if (payload.type !== "drop" || !accepted) return;
      void acceptLatestUploadDrop()
        .then(startUploads)
        .catch((cause) => setOperationError(appErrorCode(cause)));
    }).then((stop) => {
      if (disposed) stop();
      else unlisten = stop;
    });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [active, directory, loading, startUploads]);

  const navigate = (path: string) => { if (sessionId) void load(() => changeDirectory(sessionId, path)); };
  const refresh = () => { if (sessionId) void load(() => refreshDirectory(sessionId)); };
  const mutate = async (operation: () => Promise<SftpDirectory>) => { setOperationError(null); try { setDirectory(await operation()); setSelected(null); } catch (cause) { setOperationError(appErrorCode(cause)); throw cause; } };
  const upload = async () => {
    if (!sessionId || !directory) return;
    try { await startUploads(await selectUploadFiles(sessionId, directory.path)); } catch (cause) { setOperationError(appErrorCode(cause)); }
  };
  const openUploadHistory = async () => {
    if (!sessionId) return;
    setUploadHistoryLoading(true);
    try { setUploadHistory(await listUploadDirectories(sessionId)); } catch (cause) { setOperationError(appErrorCode(cause)); } finally { setUploadHistoryLoading(false); }
  };
  const download = async () => {
    if (!sessionId || selected?.kind !== "file") return;
    setOperationError(null);
    try { const target = await selectDownloadTarget(selected.name); if (target) { await startDownload(sessionId, target.grantId, selected.path); setQueueOpen(true); } } catch (cause) { setOperationError(appErrorCode(cause)); }
  };

  if (!sessionId) return <div className="grid h-full place-items-center text-sm text-[hsl(var(--muted))]">{t("files.connectRequired")}</div>;
  return <section className="relative flex h-full min-h-0 flex-col overflow-hidden bg-[hsl(var(--surface))]" aria-label={t("files.title")}>
    <div className="flex h-11 shrink-0 items-center gap-1 border-b px-3">
      <RemotePathBar path={directory?.path ?? "/"} loading={loading} uploadHistory={uploadHistory} uploadHistoryLoading={uploadHistoryLoading} onNavigate={navigate} onOpenUploadHistory={() => void openUploadHistory()} onSelectUploadDirectory={navigate} />
      <Button variant="ghost" size="sm" disabled={!directory} aria-label={t("files.upload")} onClick={() => void upload()}><Upload size={15} /><span className="hidden sm:inline">{t("files.upload")}</span></Button>
      <Button variant="ghost" size="icon" className="h-8 w-8" disabled={!directory} aria-label={t("files.newFolder")} title={t("files.newFolder")} onClick={() => setDialog("mkdir")}><FolderPlus size={16} /></Button>
      <Button variant="ghost" size="icon" className="h-8 w-8" disabled={!selected} aria-label={t("files.rename")} title={t("files.rename")} onClick={() => setDialog("rename")}><Pencil size={16} /></Button>
      <Button variant="ghost" size="icon" className="h-8 w-8" disabled={selected?.kind !== "file"} aria-label={t("files.download")} title={t("files.download")} onClick={() => void download()}><Download size={16} /></Button>
      <Button variant="ghost" size="icon" className="h-8 w-8" disabled={selected?.kind !== "file"} aria-label={t("files.previewFile")} title={t("files.previewFile")} onClick={() => { if (selected?.kind === "file") setFilePreview(selected); }}><Eye size={16} /></Button>
      <Button variant="ghost" size="icon" className="h-8 w-8 text-red-500" disabled={!selected} aria-label={t("files.delete")} title={t("files.delete")} onClick={() => setDialog("delete")}><Trash2 size={16} /></Button>
      <Button variant="ghost" size="sm" aria-label={t("transfers.title")} onClick={() => setQueueOpen((value) => !value)}><span className="hidden text-xs sm:inline">{t("transfers.title")}</span><span className="sm:hidden">{jobs.filter((job) => job.sessionId === sessionId).length}</span></Button>
      <Button variant="ghost" size="icon" className="h-8 w-8" aria-label={t("files.refresh")} title={t("files.refresh")} disabled={loading} onClick={refresh}><RefreshCw size={16} className={loading ? "animate-spin" : undefined} /></Button>
    </div>
    {operationError && <div className="border-b bg-red-500/10 px-4 py-2 text-xs text-red-500">{t(`connection.errors.${operationError}`, { defaultValue: t("files.operationFailed") })}<span className="ml-2 font-mono text-[10px] opacity-70">{operationError}</span></div>}
    {error && <div className="flex flex-1 flex-col items-center justify-center gap-3 text-sm text-red-500"><span>{t("files.error")}</span><Button variant="secondary" size="sm" onClick={() => void load(openInitialDirectory)}>{t("files.retry")}</Button></div>}
    {!error && loading && !directory && <div className="grid flex-1 place-items-center text-sm text-[hsl(var(--muted))]">{t("common.loading")}</div>}
    {!error && directory && <div className="relative min-h-0 flex-1">{dragUploadActive && <div className="pointer-events-none absolute inset-0 z-30 grid place-items-center bg-[hsl(var(--background))]/80 text-[hsl(var(--foreground))]" role="status"><span aria-hidden className="absolute left-1 top-1 h-14 w-14 rounded-tl-xl border-l-2 border-t-2 border-current" /><span aria-hidden className="absolute right-1 top-1 h-14 w-14 rounded-tr-xl border-r-2 border-t-2 border-current" /><span aria-hidden className="absolute bottom-1 left-1 h-14 w-14 rounded-bl-xl border-b-2 border-l-2 border-current" /><span aria-hidden className="absolute bottom-1 right-1 h-14 w-14 rounded-br-xl border-b-2 border-r-2 border-current" /><div className="flex -translate-y-4 flex-col items-center gap-4"><Download aria-hidden size={52} strokeWidth={1.8} /><strong className="text-base font-semibold">{t("files.dropToUpload")}</strong></div></div>}<div className="h-full overflow-auto" onContextMenu={(event) => { event.preventDefault(); setSelected(null); setContextMenu({ x: event.clientX, y: event.clientY }); }}><table className="w-full table-fixed text-left text-sm"><thead className="sticky top-0 z-10 bg-[hsl(var(--elevated))] text-xs text-[hsl(var(--secondary))]"><tr><th className="w-full px-4 py-2 font-medium md:w-[46%]">{t("files.name")}</th><th className="hidden w-[16%] px-3 py-2 font-medium sm:table-cell">{t("files.size")}</th><th className="hidden w-[24%] px-3 py-2 font-medium lg:table-cell">{t("files.modified")}</th><th className="hidden w-[14%] px-3 py-2 font-medium md:table-cell">{t("files.permissions")}</th></tr></thead><tbody>
      {directory.entries.map((entry) => <tr key={entry.path} className={`border-b border-[hsl(var(--border-soft))] hover:bg-[hsl(var(--elevated))]/70 ${selected?.path === entry.path ? "bg-blue-500/10" : ""}`} onContextMenu={(event) => { event.preventDefault(); event.stopPropagation(); setSelected(entry); setContextMenu({ x: event.clientX, y: event.clientY }); }}><td className="px-3 py-1.5"><div className="flex items-center"><button type="button" className="flex min-w-0 flex-1 items-center gap-2 rounded px-1 py-2 text-left focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blue-500" onDoubleClick={() => { if (entry.kind === "directory") navigate(entry.path); else if (entry.kind === "file") setFilePreview(entry); }} onClick={() => setSelected(entry)}>{entryIcon(entry.kind, entry.name)}<span className="truncate">{entry.name}</span></button>{entry.kind === "directory" && <button type="button" className="grid h-10 w-10 place-items-center rounded md:hidden" aria-label={t("files.openFolder", { name: entry.name })} onClick={() => navigate(entry.path)}><ChevronRight size={18} /></button>}</div></td><td className="hidden truncate px-3 py-2 font-mono text-xs text-[hsl(var(--secondary))] sm:table-cell">{entry.kind === "directory" ? "—" : formatFileSize(entry.size, i18n.language)}</td><td className="hidden truncate px-3 py-2 text-xs text-[hsl(var(--secondary))] lg:table-cell">{entry.modified === null ? "—" : new Intl.DateTimeFormat(i18n.language, { dateStyle: "medium", timeStyle: "short" }).format(entry.modified * 1000)}</td><td className="hidden px-3 py-2 font-mono text-xs text-[hsl(var(--secondary))] md:table-cell">{formatPermissions(entry.permissions)}</td></tr>)}
    </tbody></table>{directory.entries.length === 0 && <div className="grid h-32 place-items-center text-sm text-[hsl(var(--muted))]">{t("files.empty")}</div>}</div></div>}
    {queueOpen && <TransferQueue sessionId={sessionId} jobs={jobs} onClose={() => setQueueOpen(false)} onCancel={(id) => void cancelTransfer(sessionId, id)} onRetry={(id, overwrite) => void retryTransfer(sessionId, id, overwrite)} />}
    {contextMenu && <ContextMenu
      x={contextMenu.x}
      y={contextMenu.y}
      label={t("files.contextMenu")}
      onClose={() => setContextMenu(null)}
      items={[
        { label: t("files.previewFile"), icon: <Eye aria-hidden size={15} />, disabled: selected?.kind !== "file", onSelect: () => { if (selected?.kind === "file") setFilePreview(selected); } },
        { label: t("files.download"), icon: <Download aria-hidden size={15} />, disabled: selected?.kind !== "file", onSelect: () => void download() },
        { label: t("files.rename"), icon: <Pencil aria-hidden size={15} />, disabled: !selected, onSelect: () => setDialog("rename") },
        { label: t("files.delete"), icon: <Trash2 aria-hidden size={15} />, disabled: !selected, destructive: true, onSelect: () => setDialog("delete") },
        { label: t("files.addFolder"), icon: <FolderPlus aria-hidden size={15} />, onSelect: () => setDialog("mkdir") },
        { label: t("files.refresh"), icon: <RefreshCw aria-hidden size={15} />, disabled: loading, onSelect: refresh },
      ]}
    />}
    {dialog === "mkdir" && directory && <NameDialog title={t("files.newFolder")} label={t("files.folderName")} onClose={() => setDialog(null)} onSave={(name) => mutate(() => createRemoteDirectory(sessionId, directory.path, name))} />}
    {dialog === "rename" && selected && <NameDialog title={t("files.rename")} label={t("files.newName")} initial={selected.name} onClose={() => setDialog(null)} onSave={(name) => mutate(() => renameRemoteEntry(sessionId, selected.path, name))} />}
    {dialog === "delete" && selected && <ConfirmDialog title={t("files.deleteTitle")} description={t(selected.kind === "directory" ? "files.deleteDirectoryDescription" : "files.deleteFileDescription", { name: selected.name })} onClose={() => setDialog(null)} onConfirm={() => mutate(() => deleteRemoteEntry(sessionId, selected.path, selected.kind === "directory"))} />}
    {filePreview && (imageFile(filePreview)
      ? <RemoteImagePreview sessionId={sessionId} request={filePreview} onClose={() => setFilePreview(null)} />
      : <RemoteTextPreview sessionId={sessionId} request={filePreview} onClose={() => setFilePreview(null)} />)}
  </section>;
}
