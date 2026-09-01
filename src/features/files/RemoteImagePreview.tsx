import { ArrowLeft, Download, Image as ImageIcon, LoaderCircle, Maximize2, RotateCcw, RotateCw, X, ZoomIn, ZoomOut } from "lucide-react";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "../../components/ui/button";
import { previewRemoteImage, selectDownloadTarget, startDownload } from "../../lib/tauri/ssh";
import type { RemoteImagePreview as RemoteImageData } from "../../types/session";

export type RemoteFileRequest = { path: string; name: string };

const errorCode = (error: unknown) => typeof error === "object" && error !== null && "code" in error && typeof error.code === "string" ? error.code : "UNKNOWN";

function imageObjectUrl(image: RemoteImageData) {
  const binary = window.atob(image.dataBase64);
  const bytes = new Uint8Array(binary.length);
  for (let index = 0; index < binary.length; index += 1) bytes[index] = binary.charCodeAt(index);
  return URL.createObjectURL(new Blob([bytes], { type: image.mimeType }));
}

export function RemoteImagePreview({ sessionId, request, onClose }: { sessionId: string; request: RemoteFileRequest; onClose: () => void }) {
  const { t, i18n } = useTranslation();
  const [image, setImage] = useState<(RemoteImageData & { url: string }) | null>(null);
  const [failure, setFailure] = useState<string | null>(null);
  const [downloadFailed, setDownloadFailed] = useState(false);
  const [downloading, setDownloading] = useState(false);
  const [zoom, setZoom] = useState(1);
  const [rotation, setRotation] = useState(0);

  useEffect(() => {
    let current = true;
    let objectUrl: string | null = null;
    setImage(null);
    setFailure(null);
    setDownloadFailed(false);
    setZoom(1);
    setRotation(0);
    void previewRemoteImage(sessionId, request.path).then((result) => {
      if (!current) return;
      objectUrl = imageObjectUrl(result);
      setImage({ ...result, url: objectUrl });
    }).catch((error) => { if (current) setFailure(errorCode(error)); });
    return () => {
      current = false;
      if (objectUrl) URL.revokeObjectURL(objectUrl);
    };
  }, [request.path, sessionId]);

  useEffect(() => {
    const closeOnEscape = (event: KeyboardEvent) => { if (event.key === "Escape") onClose(); };
    window.addEventListener("keydown", closeOnEscape);
    return () => window.removeEventListener("keydown", closeOnEscape);
  }, [onClose]);

  const download = async () => {
    setDownloadFailed(false);
    setDownloading(true);
    try {
      const target = await selectDownloadTarget(request.name);
      if (target) await startDownload(sessionId, target.grantId, request.path);
    } catch {
      setDownloadFailed(true);
    } finally {
      setDownloading(false);
    }
  };
  const formatSize = (size: number) => new Intl.NumberFormat(i18n.language, { style: "unit", unit: "kilobyte", maximumFractionDigits: 1 }).format(size / 1024);

  return <section role="dialog" aria-modal="true" aria-label={t("imagePreview.title")} className="absolute inset-0 z-30 flex min-h-0 flex-col overflow-hidden bg-[hsl(var(--background))]">
    <header className="flex min-h-14 shrink-0 items-center gap-3 border-b border-[hsl(var(--border))] bg-[hsl(var(--surface))] px-3 py-2 sm:px-4">
      <Button variant="ghost" size="sm" className="shrink-0 px-2 sm:px-3" onClick={onClose}><ArrowLeft size={16} /><span className="hidden sm:inline">{t("imagePreview.backToFiles")}</span></Button>
      <div className="h-6 w-px shrink-0 bg-[hsl(var(--border))]" />
      <span className="grid h-8 w-8 shrink-0 place-items-center rounded-lg bg-blue-500/10 text-blue-500"><ImageIcon size={17} /></span>
      <div className="min-w-0 flex-1"><h2 className="truncate text-sm font-semibold">{request.name}</h2><p className="truncate text-[11px] text-[hsl(var(--muted))]">{request.path}</p></div>
      {image && <div className="hidden items-center gap-1.5 text-[11px] text-[hsl(var(--secondary))] md:flex"><span className="rounded-full border border-[hsl(var(--border))] px-2 py-1 uppercase">{image.mimeType.replace("image/", "")}</span><span className="rounded-full border border-[hsl(var(--border))] px-2 py-1">{image.width} × {image.height}</span><span className="rounded-full border border-[hsl(var(--border))] px-2 py-1">{formatSize(image.size)}</span></div>}
      <Button variant="secondary" size="sm" disabled={downloading} onClick={() => void download()}>{downloading ? <LoaderCircle size={15} className="animate-spin" /> : <Download size={15} />}<span className="hidden sm:inline">{t("files.download")}</span></Button>
      <Button variant="ghost" size="icon" className="h-8 w-8 shrink-0" aria-label={t("imagePreview.close")} title={t("imagePreview.close")} onClick={onClose}><X size={17} /></Button>
    </header>
    {downloadFailed && <p className="border-b border-red-500/20 bg-red-500/10 px-4 py-2 text-xs text-red-500">{t("files.operationFailed")}</p>}
    <div className="relative grid min-h-0 flex-1 place-items-center overflow-auto bg-[hsl(var(--elevated))]/35 p-4 pb-24 sm:p-8 sm:pb-24">
      {!image && !failure && <div className="flex flex-col items-center gap-3 text-sm text-[hsl(var(--muted))]"><span className="grid h-12 w-12 place-items-center rounded-xl border border-[hsl(var(--border))] bg-[hsl(var(--surface))]"><LoaderCircle size={21} className="animate-spin text-blue-500" /></span>{t("imagePreview.loading")}</div>}
      {failure && <div className="mx-4 max-w-md rounded-xl border border-red-500/25 bg-[hsl(var(--surface))] p-6 text-center"><span className="mx-auto grid h-11 w-11 place-items-center rounded-xl bg-red-500/10 text-red-500"><ImageIcon size={21} /></span><p className="mt-3 text-sm font-semibold">{t("imagePreview.failed")}</p><p className="mt-1.5 text-xs leading-5 text-[hsl(var(--secondary))]">{t(`connection.errors.${failure}`, { defaultValue: t("connection.defaultError") })}</p></div>}
      {image && <div className="flex min-h-full min-w-full items-center justify-center"><div className="rounded-xl border border-[hsl(var(--border))] bg-[hsl(var(--surface))] p-2 shadow-sm"><img src={image.url} alt={request.name} draggable={false} className="max-h-[calc(100vh-13rem)] max-w-[calc(100vw-8rem)] select-none rounded-lg object-contain transition-transform duration-200" style={{ transform: `scale(${zoom}) rotate(${rotation}deg)` }} onError={() => { setImage(null); setFailure("SFTP_IMAGE_INVALID"); }} /></div></div>}
      {image && <div className="absolute bottom-4 left-1/2 flex -translate-x-1/2 items-center gap-1 rounded-xl border border-[hsl(var(--border))] bg-[hsl(var(--surface))]/95 p-1.5 shadow-sm backdrop-blur-md">
        <Button variant="ghost" size="icon" className="h-8 w-8" disabled={zoom <= 0.25} aria-label={t("imagePreview.zoomOut")} title={t("imagePreview.zoomOut")} onClick={() => setZoom((value) => Math.max(0.25, value - 0.25))}><ZoomOut size={16} /></Button>
        <button type="button" className="h-8 min-w-14 rounded-md px-2 font-mono text-xs text-[hsl(var(--secondary))] hover:bg-[hsl(var(--elevated))]" title={t("imagePreview.actualSize")} onClick={() => setZoom(1)}>{Math.round(zoom * 100)}%</button>
        <Button variant="ghost" size="icon" className="h-8 w-8" disabled={zoom >= 4} aria-label={t("imagePreview.zoomIn")} title={t("imagePreview.zoomIn")} onClick={() => setZoom((value) => Math.min(4, value + 0.25))}><ZoomIn size={16} /></Button>
        <span className="mx-1 h-5 w-px bg-[hsl(var(--border))]" />
        <Button variant="ghost" size="icon" className="h-8 w-8" aria-label={t("imagePreview.fit")} title={t("imagePreview.fit")} onClick={() => setZoom(1)}><Maximize2 size={15} /></Button>
        <Button variant="ghost" size="icon" className="hidden h-8 w-8 sm:inline-flex" aria-label={t("imagePreview.rotateLeft")} title={t("imagePreview.rotateLeft")} onClick={() => setRotation((value) => (value + 270) % 360)}><RotateCcw size={16} /></Button>
        <Button variant="ghost" size="icon" className="h-8 w-8" aria-label={t("imagePreview.rotate")} title={t("imagePreview.rotate")} onClick={() => setRotation((value) => (value + 90) % 360)}><RotateCw size={16} /></Button>
      </div>}
    </div>
  </section>;
}
