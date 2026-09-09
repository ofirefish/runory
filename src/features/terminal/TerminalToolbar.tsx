import { ChevronDown, ChevronUp, ClipboardPaste, Copy, Search, X } from "lucide-react";
import { useEffect, useRef } from "react";
import { createPortal } from "react-dom";
import { useTranslation } from "react-i18next";
import { Button } from "../../components/ui/button";
import { Input } from "../../components/ui/input";

type SearchResult = { resultIndex: number; resultCount: number };

export function TerminalToolbar({ toolbarHost, searchOpen, query, result, clipboardError, onOpenSearch, onCloseSearch, onQueryChange, onFindNext, onFindPrevious, onCopy, onPaste }: {
  toolbarHost?: HTMLDivElement | null;
  searchOpen: boolean;
  query: string;
  result: SearchResult;
  clipboardError: boolean;
  onOpenSearch: () => void;
  onCloseSearch: () => void;
  onQueryChange: (query: string) => void;
  onFindNext: () => void;
  onFindPrevious: () => void;
  onCopy: () => void;
  onPaste: () => void;
}) {
  const { t } = useTranslation();
  const searchInput = useRef<HTMLInputElement>(null);
  useEffect(() => { if (searchOpen) searchInput.current?.focus(); }, [searchOpen]);

  const actions = <div className={toolbarHost ? "flex items-center gap-1" : "pointer-events-auto flex items-center gap-1 rounded-md border bg-[hsl(var(--surface))]/95 p-1 shadow-lg"}>
    <Button variant="ghost" size="icon" className="h-8 w-8" aria-label={t("terminal.search")} aria-expanded={searchOpen} title={t("terminal.searchShortcut")} onClick={searchOpen ? onCloseSearch : onOpenSearch}><Search size={15} /></Button>
    <Button variant="ghost" size="icon" className="h-8 w-8" aria-label={t("terminal.copySelection")} title={t("terminal.copyShortcut")} onClick={onCopy}><Copy size={15} /></Button>
    <Button variant="ghost" size="icon" className="h-8 w-8" aria-label={t("terminal.paste")} title={t("terminal.pasteShortcut")} onClick={onPaste}><ClipboardPaste size={15} /></Button>
  </div>;

  return <div className="pointer-events-none absolute right-2 top-2 z-10 flex max-w-[calc(100%-1rem)] items-start gap-1">
    {searchOpen && <form className="pointer-events-auto flex items-center gap-1 rounded-md border bg-[hsl(var(--surface))] p-1 shadow-lg" onSubmit={(event) => { event.preventDefault(); onFindNext(); }}>
      <Input ref={searchInput} className="h-8 w-52" value={query} onChange={(event) => onQueryChange(event.target.value)} onKeyDown={(event) => { if (event.key === "Escape") onCloseSearch(); else if (event.key === "Enter" && event.shiftKey) { event.preventDefault(); onFindPrevious(); } }} aria-label={t("terminal.searchInput")} placeholder={t("terminal.searchPlaceholder")} />
      <span className="min-w-14 text-center text-xs text-[hsl(var(--muted))]">{result.resultCount > 0 ? t("terminal.searchCount", { current: result.resultIndex + 1, total: result.resultCount }) : t("terminal.noMatches")}</span>
      <Button type="button" variant="ghost" size="icon" className="h-8 w-8" aria-label={t("terminal.previousMatch")} onClick={onFindPrevious}><ChevronUp size={15} /></Button>
      <Button type="submit" variant="ghost" size="icon" className="h-8 w-8" aria-label={t("terminal.nextMatch")}><ChevronDown size={15} /></Button>
      <Button type="button" variant="ghost" size="icon" className="h-8 w-8" aria-label={t("terminal.closeSearch")} onClick={onCloseSearch}><X size={15} /></Button>
    </form>}
    {toolbarHost ? createPortal(actions, toolbarHost) : actions}
    {clipboardError && <span role="alert" className="rounded-md border bg-[hsl(var(--surface))] p-2 text-xs text-red-500">{t("terminal.clipboardError")}</span>}
  </div>;
}
