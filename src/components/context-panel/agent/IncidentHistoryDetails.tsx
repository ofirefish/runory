import { useState } from "react";
import { useTranslation } from "react-i18next";
import { AgentTimeline } from "./AgentTimeline";
import { AgentRunError } from "./AgentRunError";
import { historyDate, historyStatusKey, type HistoryDetail, type HistoryItem } from "./incident-history";

export function HistoryDetails({ item, result }: { item: HistoryItem; result: HistoryDetail }) {
  const { t, i18n } = useTranslation();
  const [expanded, setExpanded] = useState<Record<string, boolean>>({});
  const status = result.kind === "run" ? result.detail.run.state : result.detail.status;
  const updatedAt = result.kind === "run" ? result.detail.run.updatedAtEpochMs : Math.max(0, ...result.detail.timeline.map((event) => event.occurredAtEpochMs));
  return <section className="history-details">
    <h3>{item.title || t("contextPanel.historyUntitled")}</h3>
    <p className="history-meta">{t(historyStatusKey({ kind: result.kind, status }), { defaultValue: t("contextPanel.historyUnknown") })} · {historyDate(updatedAt, i18n.language) ?? t("contextPanel.historyTimeUnknown")}</p>
    <p className="history-note">{t("contextPanel.historyReadOnly")}</p>
    <dl className="history-fields"><dt>{t("incident.targets")}</dt><dd>{item.targetIds.join(", ") || t("contextPanel.historyUnknown")}</dd></dl>
    {result.kind === "run" ? <>
      {result.detail.truncated && <p className="history-note">{t("contextPanel.historyTruncated")}</p>}
      {result.detail.events.length === 0 && <p>{t("contextPanel.historyNoDetails")}</p>}
      <AgentTimeline events={result.detail.events} readOnly expanded={expanded}
        onToggle={(key) => setExpanded((value) => ({ ...value, [key]: !value[key] }))}
        onApprove={() => undefined} onReject={() => undefined} />
      <AgentRunError events={result.detail.events} lastErrorCode={null} running={false} />
    </> : <>
      {result.detail.recoveryState === "metadata-only" && <p className="history-note">{t("incident.metadataOnly")}</p>}
      <dl className="history-fields">
        <dt>{t("incident.pack")}</dt><dd>{t(`incident.pack.${result.detail.pack}`, { defaultValue: t("contextPanel.historyUnknown") })}</dd>
        <dt>{t("incident.rootCause")}</dt><dd>{t(`incident.cause.${result.detail.rootCause.code}`, { defaultValue: t("incident.cause.incident-inconclusive") })}</dd>
        <dt>{t("incident.verification")}</dt><dd>{t(`contextPanel.historyVerification.${result.detail.verification.statusCode}`, { defaultValue: t("contextPanel.historyUnknown") })}</dd>
      </dl>
      {result.detail.symptoms.map((symptom, index) => <p key={index}>{symptom}</p>)}
      <h4>{t("incident.evidence")}</h4>
      {result.detail.recoveryState === "live"
        ? result.detail.evidence.map((evidence) => <p key={evidence.id}>{evidence.source}: {evidence.summary}</p>)
        : result.detail.evidenceReferences.map((evidence) => <p key={evidence.id}>{evidence.source} · {evidence.targetId}</p>)}
      <h4>{t("incident.timeline")}</h4>
      <ol className="history-lifecycle">{result.detail.timeline.map((event, index) => <li key={index}>
        <span>{t(`incident.timeline.${event.code}`, { defaultValue: t(`incident.status.${event.status}`, { defaultValue: t("contextPanel.historyUnknown") }) })}</span>
        <time>{historyDate(event.occurredAtEpochMs, i18n.language) ?? t("contextPanel.historyTimeUnknown")}</time>
      </li>)}</ol>
    </>}
  </section>;
}
