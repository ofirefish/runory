import { ArrowLeft, Braces, Check, ChevronDown, ChevronUp, Copy, Download, FileCode2, LoaderCircle, Search, WrapText, X } from "lucide-react";
import { Fragment, useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "../../components/ui/button";
import { Input } from "../../components/ui/input";
import { previewRemoteText, selectDownloadTarget, startDownload } from "../../lib/tauri/ssh";
import type { RemoteTextPreview as RemoteTextData } from "../../types/session";
import type { RemoteFileRequest } from "./RemoteImagePreview";

const MAX_RENDERED_MATCHES = 1_000;
const errorCode = (error: unknown) => typeof error === "object" && error !== null && "code" in error && typeof error.code === "string" ? error.code : "UNKNOWN";

function jsonFormat(content: string, language: string) {
  if (language !== "json") return null;
  try { return JSON.stringify(JSON.parse(content), null, 2); } catch { return null; }
}

function matchesIn(content: string, query: string) {
  if (!query) return { positions: [] as number[], total: 0 };
  const source = content.toLocaleLowerCase();
  const needle = query.toLocaleLowerCase();
  const positions: number[] = [];
  let total = 0;
  let position = 0;
  while ((position = source.indexOf(needle, position)) !== -1) {
    if (positions.length < MAX_RENDERED_MATCHES) positions.push(position);
    total += 1;
    position += Math.max(needle.length, 1);
  }
  return { positions, total };
}

export function RemoteTextPreview({ sessionId, request, onClose }: { sessionId: string; request: RemoteFileRequest; onClose: () => void }) {
  const { t, i18n } = useTranslation();
  const viewer = useRef<HTMLDivElement>(null);
  const searchInput = useRef<HTMLInputElement>(null);
  const [file, setFile] = useState<RemoteTextData | null>(null);
  const [failure, setFailure] = useState<string | null>(null);
  const [query, setQuery] = useState("");
  const [activeMatch, setActiveMatch] = useState(0);
  const [wrap, setWrap] = useState(false);
  const [formatted, setFormatted] = useState(false);
  const [copied, setCopied] = useState(false);
  const [downloading, setDownloading] = useState(false);
  const [downloadFailed, setDownloadFailed] = useState(false);

  useEffect(() => {
    let current = true;
    setFile(null); setFailure(null); setQuery(""); setFormatted(false); setCopied(false); setDownloadFailed(false);
    void previewRemoteText(sessionId, request.path).then((result) => { if (current) setFile(result); }).catch((error) => { if (current) setFailure(errorCode(error)); });
    return () => { current = false; };
  }, [request.path, sessionId]);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "f") { event.preventDefault(); searchInput.current?.focus(); }
      else if (event.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [onClose]);

  const formattedJson = useMemo(() => file ? jsonFormat(file.content, file.language) : null, [file]);
  const content = formatted && formattedJson !== null ? formattedJson : file?.content ?? "";
  const lineCount = content ? content.split("\n").length : 1;
  const lineNumbers = useMemo(() => Array.from({ length: lineCount }, (_, index) => index + 1).join("\n"), [lineCount]);
  const matches = useMemo(() => matchesIn(content, query), [content, query]);

  useEffect(() => {
    setActiveMatch(0);
  }, [query, content]);
  useEffect(() => {
    viewer.current?.querySelector(`[data-text-match="${activeMatch}"]`)?.scrollIntoView({ block: "center", inline: "nearest" });
  }, [activeMatch, matches.positions]);

  const highlighted = useMemo(() => {
    if (!query || matches.positions.length === 0) return content;
    const nodes = [];
    let cursor = 0;
    for (let index = 0; index < matches.positions.length; index += 1) {
      const position = matches.positions[index];
      nodes.push(<Fragment key={`text-${position}`}>{content.slice(cursor, position)}</Fragment>);
      nodes.push(<mark key={`match-${position}`} data-text-match={index} className={index === activeMatch ? "rounded-sm bg-amber-400 text-slate-950 outline outline-2 outline-blue-500" : "rounded-sm bg-amber-300/70 text-slate-950"}>{content.slice(position, position + query.length)}</mark>);
      cursor = position + query.length;
    }
    nodes.push(<Fragment key="text-tail">{content.slice(cursor)}</Fragment>);
    return nodes;
  }, [activeMatch, content, matches.positions, query]);

  const moveMatch = (direction: -1 | 1) => {
    if (matches.positions.length === 0) return;
    setActiveMatch((current) => (current + direction + matches.positions.length) % matches.positions.length);
  };
  const copy = async () => {
    try { await navigator.clipboard.writeText(content); setCopied(true); } catch { setCopied(false); }
  };
  const download = async () => {
    setDownloadFailed(false); setDownloading(true);
    try { const target = await selectDownloadTarget(request.name); if (target) await startDownload(sessionId, target.grantId, request.path); }
    catch { setDownloadFailed(true); }
    finally { setDownloading(false); }
  };
  const formatSize = (size: number) => new Intl.NumberFormat(i18n.language, { style: "unit", unit: "kilobyte", maximumFractionDigits: 1 }).format(size / 1024);

  return <section role="dialog" aria-modal="true" aria-label={t("textPreview.title")} className="absolute inset-0 z-30 flex min-h-0 flex-col overflow-hidden bg-[hsl(var(--background))]">
    <header className="flex min-h-14 shrink-0 items-center gap-3 border-b border-[hsl(var(--border))] bg-[hsl(var(--surface))] px-3 py-2 sm:px-4">
      <Button variant="ghost" size="sm" className="shrink-0 px-2 sm:px-3" onClick={onClose}><ArrowLeft size={16} /><span className="hidden sm:inline">{t("imagePreview.backToFiles")}</span></Button>
      <div className="h-6 w-px shrink-0 bg-[hsl(var(--border))]" />
      <span className="grid h-8 w-8 shrink-0 place-items-center rounded-lg bg-blue-500/10 text-blue-500"><FileCode2 size={17} /></span>
      <div className="min-w-0 flex-1"><h2 className="truncate text-sm font-semibold">{request.name}</h2><p className="truncate text-[11px] text-[hsl(var(--muted))]">{request.path}</p></div>
      {file && <div className="hidden items-center gap-1.5 text-[11px] text-[hsl(var(--secondary))] lg:flex"><span className="rounded-full border border-[hsl(var(--border))] px-2 py-1 uppercase">{file.language}</span><span className="rounded-full border border-[hsl(var(--border))] px-2 py-1">{file.encoding}</span><span className="rounded-full border border-[hsl(var(--border))] px-2 py-1">{t("textPreview.lines", { count: lineCount })}</span><span className="rounded-full border border-[hsl(var(--border))] px-2 py-1">{formatSize(file.size)}</span></div>}
      <Button variant="secondary" size="sm" disabled={downloading} onClick={() => void download()}>{downloading ? <LoaderCircle size={15} className="animate-spin" /> : <Download size={15} />}<span className="hidden sm:inline">{t("files.download")}</span></Button>
      <Button variant="ghost" size="icon" className="h-8 w-8 shrink-0" aria-label={t("imagePreview.close")} onClick={onClose}><X size={17} /></Button>
    </header>
    {file && <div className="flex min-h-11 shrink-0 flex-wrap items-center gap-1.5 border-b border-[hsl(var(--border))] bg-[hsl(var(--surface))] px-3 py-1.5">
      <div className="relative min-w-44 flex-1 sm:max-w-sm"><Search size={14} className="pointer-events-none absolute left-2.5 top-1/2 -translate-y-1/2 text-[hsl(var(--muted))]" /><Input ref={searchInput} value={query} className="h-8 pl-8 pr-16 text-xs" placeholder={t("textPreview.search")} onChange={(event) => setQuery(event.target.value)} /><span className="absolute right-2 top-1/2 -translate-y-1/2 text-[10px] text-[hsl(var(--muted))]">{matches.total ? `${activeMatch + 1}/${matches.total > MAX_RENDERED_MATCHES ? `${MAX_RENDERED_MATCHES}+` : matches.total}` : "0/0"}</span></div>
      <Button variant="ghost" size="icon" className="h-8 w-8" disabled={!matches.total} aria-label={t("textPreview.previousMatch")} onClick={() => moveMatch(-1)}><ChevronUp size={15} /></Button><Button variant="ghost" size="icon" className="h-8 w-8" disabled={!matches.total} aria-label={t("textPreview.nextMatch")} onClick={() => moveMatch(1)}><ChevronDown size={15} /></Button>
      <span className="mx-1 hidden h-5 w-px bg-[hsl(var(--border))] sm:block" />
      <Button variant={wrap ? "secondary" : "ghost"} size="sm" onClick={() => setWrap((value) => !value)}><WrapText size={15} /><span className="hidden md:inline">{t("textPreview.wrap")}</span></Button>
      {formattedJson !== null && <Button variant={formatted ? "secondary" : "ghost"} size="sm" onClick={() => setFormatted((value) => !value)}><Braces size={15} /><span className="hidden md:inline">{t("textPreview.formatJson")}</span></Button>}
      <Button variant="ghost" size="sm" onClick={() => void copy()}>{copied ? <Check size={15} className="text-emerald-500" /> : <Copy size={15} />}<span className="hidden md:inline">{copied ? t("textPreview.copied") : t("textPreview.copy")}</span></Button>
    </div>}
    {downloadFailed && <p className="border-b border-red-500/20 bg-red-500/10 px-4 py-2 text-xs text-red-500">{t("files.operationFailed")}</p>}
    <div ref={viewer} className="relative min-h-0 flex-1 overflow-auto bg-[hsl(var(--background))]">
      {!file && !failure && <div className="grid h-full place-items-center"><div className="flex flex-col items-center gap-3 text-sm text-[hsl(var(--muted))]"><span className="grid h-12 w-12 place-items-center rounded-xl border border-[hsl(var(--border))] bg-[hsl(var(--surface))]"><LoaderCircle size={21} className="animate-spin text-blue-500" /></span>{t("textPreview.loading")}</div></div>}
      {failure && <div className="grid h-full place-items-center p-4"><div className="max-w-md rounded-xl border border-red-500/25 bg-[hsl(var(--surface))] p-6 text-center"><span className="mx-auto grid h-11 w-11 place-items-center rounded-xl bg-red-500/10 text-red-500"><FileCode2 size={21} /></span><p className="mt-3 text-sm font-semibold">{t("textPreview.failed")}</p><p className="mt-1.5 text-xs leading-5 text-[hsl(var(--secondary))]">{t(`connection.errors.${failure}`, { defaultValue: t("connection.defaultError") })}</p></div></div>}
      {file && <div className={`flex min-h-full min-w-full items-start font-mono text-[13px] leading-6 ${wrap ? "w-full" : "w-max"}`}><pre aria-hidden className="sticky left-0 z-10 min-h-full select-none border-r border-[hsl(var(--border))] bg-[hsl(var(--elevated))] px-3 py-4 text-right text-[hsl(var(--muted))]">{lineNumbers}</pre><pre className={`min-h-full flex-1 px-4 py-4 text-[hsl(var(--foreground))] ${wrap ? "min-w-0 whitespace-pre-wrap break-words" : "whitespace-pre"}`}>{highlighted}</pre></div>}
    </div>
  </section>;
}
