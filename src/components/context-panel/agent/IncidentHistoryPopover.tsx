import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { listIncidents } from "../../../lib/tauri/agentic";
import type { Incident } from "../../../types/agentic";

function timeAgo(epochMs: number): string {
  const diff = Date.now() - epochMs;
  if (diff < 60_000) return "just now";
  if (diff < 3_600_000) return `${Math.floor(diff / 60_000)}m ago`;
  if (diff < 86_400_000) return `${Math.floor(diff / 3_600_000)}h ago`;
  if (diff < 7 * 86_400_000) return `${Math.floor(diff / 86_400_000)}d ago`;
  return new Date(epochMs).toLocaleDateString();
}

/**
 * Recent incidents popover replacing the old large “Local incident
 * history / select an incident” dropdown. Clicking resumes an incident.
 */
export function IncidentHistoryPopover({ open, onClose, onSelect }: {
  open: boolean;
  onClose: () => void;
  onSelect: (incident: Incident) => void;
}) {
  const { t } = useTranslation();
  const [history, setHistory] = useState<Incident[]>([]);
  const [failed, setFailed] = useState(false);
  const root = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    void listIncidents().then(setHistory).catch(() => setFailed(true));
  }, [open]);

  useEffect(() => {
    if (!open) return;
    const onDown = (event: MouseEvent) => {
      if (root.current && !root.current.contains(event.target as Node)) onClose();
    };
    window.addEventListener("mousedown", onDown);
    return () => window.removeEventListener("mousedown", onDown);
  }, [open, onClose]);

  if (!open) return null;
  return <div className="incident-history-popover" ref={root} role="dialog" aria-label={t("contextPanel.history")}>
    <header>{t("contextPanel.history")}<button type="button" className="icon-x" aria-label={t("common.close")} onClick={onClose}>×</button></header>
    {failed ? <p className="history-error">{t("agent.error")}</p>
      : history.length === 0 ? <p className="history-empty">{t("contextPanel.historyEmpty")}</p>
        : <ul>{history.slice(0, 12).map((item) => <li key={item.id}>
          <button type="button" onClick={() => { onSelect(item); onClose(); }}>
            <span className="history-title">{item.symptoms[0] ?? item.id}</span>
            <span className="history-meta">
              <span className={`history-state ${item.status}`}>{t(`incident.status.${item.status}`)}</span>
              <span>{timeAgo(item.durationMs > 0 ? Date.now() - item.durationMs : Date.now())}</span>
            </span>
          </button>
        </li>)}</ul>}
  </div>;
}