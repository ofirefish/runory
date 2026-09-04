import { RefreshCw, X } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { historyDate, historyStatusKey, loadRecentHistory, type HistoryItem } from "./incident-history";

export function IncidentHistoryPopover({ onClose, onSelect, targetId, sessionId = null }: {
  onClose: () => void;
  onSelect: (item: HistoryItem) => void;
  targetId: string | null;
  sessionId?: string | null;
}) {
  const { t, i18n } = useTranslation();
  const [items, setItems] = useState<HistoryItem[]>([]);
  const [loading, setLoading] = useState(true);
  const [failed, setFailed] = useState(false);
  const [partial, setPartial] = useState(false);
  const [revision, setRevision] = useState(0);
  const root = useRef<HTMLDivElement>(null);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    setFailed(false);
    setPartial(false);
    const request = (targetId ? loadRecentHistory(targetId, sessionId) : Promise.resolve({ items: [], partial: false })).then((value) => {
        if (!cancelled) { setItems(value.items); setPartial(value.partial); }
      });
    void request.catch(() => { if (!cancelled) setFailed(true); })
      .finally(() => { if (!cancelled) setLoading(false); });
    return () => { cancelled = true; };
  }, [targetId, sessionId, revision]);

  useEffect(() => {
    const previous = document.activeElement;
    root.current?.focus();
    const onDown = (event: MouseEvent) => {
      if (event.target instanceof Element && event.target.closest("[data-agent-history-trigger]")) return;
      if (root.current && !root.current.contains(event.target as Node)) onClose();
    };
    window.addEventListener("mousedown", onDown);
    return () => {
      window.removeEventListener("mousedown", onDown);
      if (previous instanceof HTMLElement && previous.isConnected) previous.focus();
    };
  }, [onClose]);

  return <div className="incident-history-popover" ref={root} role="dialog" tabIndex={-1}
    aria-label={t("contextPanel.history")} onKeyDown={(event) => {
      if (event.key === "Escape") { event.preventDefault(); event.stopPropagation(); onClose(); }
      if (event.key === "Tab") {
        const nodes = root.current?.querySelectorAll<HTMLElement>("button:not(:disabled), input:not(:disabled)");
        if (!nodes?.length) return;
        const first = nodes[0]; const last = nodes[nodes.length - 1];
        if (event.shiftKey && (document.activeElement === first || document.activeElement === root.current)) { event.preventDefault(); last.focus(); }
        else if (!event.shiftKey && document.activeElement === last) { event.preventDefault(); first.focus(); }
      }
    }}>
    <header>
      <span>{t("contextPanel.history")}</span>
      <button type="button" className="icon-x" disabled={loading} aria-label={t("contextPanel.historyRefresh")} onClick={() => setRevision((value) => value + 1)}><RefreshCw size={14} /></button>
      <button type="button" className="icon-x" aria-label={t("common.close")} onClick={onClose}><X size={14} /></button>
    </header>
    <div className="history-body" aria-busy={loading}>
      {loading ? <p className="history-empty" role="status">{t("common.loading")}</p>
        : failed ? <div className="history-error" role="alert"><p>{t("contextPanel.historyError")}</p><button type="button" onClick={() => setRevision((value) => value + 1)}>{t("contextPanel.historyRetry")}</button></div>
          : <>
              {partial && <p className="history-error" role="status">{t("contextPanel.historyPartial")}</p>}
              {(!partial || items.length > 0) && <HistoryList items={items} language={i18n.language} onSelect={(item) => { onSelect(item); onClose(); }} />}
            </>}
    </div>
  </div>;
}

export function HistoryList({ items, language, onSelect }: { items: HistoryItem[]; language: string; onSelect: (item: HistoryItem) => void }) {
  const { t } = useTranslation();
  if (items.length === 0) return <p className="history-empty">{t("contextPanel.historyEmpty")}</p>;
  return <ul className="history-list">{items.map((item) => <li key={`${item.kind}-${item.id}`}>
    <button type="button" onClick={() => onSelect(item)}>
      <span className="history-title">{item.title || t("contextPanel.historyUntitled")}</span>
      <span className="history-meta"><span>{t(`contextPanel.historyKind.${item.kind}`)}</span><span className={`history-state ${item.status}`}>{t(historyStatusKey(item), { defaultValue: t("contextPanel.historyUnknown") })}</span></span>
      <span className="history-meta">{historyDate(item.updatedAt, language) ?? t("contextPanel.historyTimeUnknown")}</span>
    </button>
  </li>)}</ul>;
}
