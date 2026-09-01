import { useTranslation } from "react-i18next";
import type { Incident } from "../../../types/agentic";
import { EvidenceList } from "./EvidenceList";

/**
 * Root-cause card backed by the existing Incident / Evidence engine.
 * Shows the root cause plus bullet evidence, not a vague “I think…”.
 */
export function DiagnosisCard({ incident }: { incident: Incident }) {
  const { t } = useTranslation();
  const { rootCause, evidence } = incident;
  return <div className="diagnosis-card" aria-label={t("contextPanel.rootCause")}>
    <h4>{t("contextPanel.rootCause")}</h4>
    <p className="diagnosis-cause">{t(`incident.cause.${rootCause.code}`)}</p>
    <p className="diagnosis-confidence">{t("incident.confidence", { value: Math.round(rootCause.confidence * 100), count: rootCause.evidenceIds.length })}</p>
    <EvidenceList evidence={evidence} />
  </div>;
}