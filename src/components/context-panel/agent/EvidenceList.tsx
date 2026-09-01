import { useTranslation } from "react-i18next";
import type { Incident } from "../../../types/agentic";

/** Compact bullet list of evidence supporting the diagnosis. */
export function EvidenceList({ evidence }: { evidence: Incident["evidence"] }) {
  const { t } = useTranslation();
  if (evidence.length === 0) return null;
  return <ul className="evidence-list" aria-label={t("incident.evidence")}>
    {evidence.slice(0, 6).map((item) => <li key={item.id}>
      <span className={item.result.success ? "ok" : "err"} aria-hidden>{item.result.success ? "✓" : "✕"}</span>
      <span className="evidence-text">{item.summary || `${item.source} · ${item.targetId}`}</span>
    </li>)}
  </ul>;
}