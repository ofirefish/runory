import { ArrowLeft, RefreshCw } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { HistoryDetails } from "./IncidentHistoryDetails";
import { loadHistoryDetail, type HistoryDetail, type HistoryItem } from "./incident-history";

export function IncidentHistoryPanel({ item, onBack }: { item: HistoryItem; onBack: () => void }) {
  const { t } = useTranslation();
  const [detail, setDetail] = useState<HistoryDetail | null>(null);
  const [failed, setFailed] = useState(false);
  const [revision, setRevision] = useState(0);
  const root = useRef<HTMLElement>(null);

  useEffect(() => { root.current?.focus(); }, []);
  useEffect(() => {
    let cancelled = false;
    setDetail(null);
    setFailed(false);
    void loadHistoryDetail(item).then((value) => {
      if (!cancelled) setDetail(value);
    }).catch(() => { if (!cancelled) setFailed(true); });
    return () => { cancelled = true; };
  }, [item, revision]);

  return <section className="agent-history-panel" ref={root} tabIndex={-1} aria-label={t("contextPanel.history")}>
    <div className="agent-history-toolbar">
      <button type="button" className="select-server-btn" onClick={onBack}><ArrowLeft size={14} />{t("contextPanel.historyReturnCurrent")}</button>
      <button type="button" className="agent-icon-button" disabled={!detail && !failed} aria-label={t("contextPanel.historyRefresh")} onClick={() => setRevision((value) => value + 1)}><RefreshCw size={14} /></button>
    </div>
    {failed ? <div className="history-error" role="alert"><p>{t("contextPanel.historyError")}</p><button type="button" onClick={() => setRevision((value) => value + 1)}>{t("contextPanel.historyRetry")}</button></div>
      : detail ? <HistoryDetails item={item} result={detail} />
        : <p className="history-empty" role="status">{t("common.loading")}</p>}
  </section>;
}
