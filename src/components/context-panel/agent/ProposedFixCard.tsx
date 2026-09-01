import { ExternalLink, ShieldAlert } from "lucide-react";
import { useTranslation } from "react-i18next";
import type { Incident } from "../../../types/agentic";
import { Button } from "../../ui/button";

/**
 * Proposed fix: risk + step count + [Review Plan] that opens a full
 * ChangeSet review in the Main Workspace — never inline shell execution.
 */
export function ProposedFixCard({ incident, onReviewPlan }: {
  incident: Incident;
  onReviewPlan: () => void;
}) {
  const { t } = useTranslation();
  const fix = incident.proposedFix;
  return <div className="proposed-fix-card" aria-label={t("contextPanel.proposedFix")}>
    <div className="proposed-fix-head">
      <h4>{t("contextPanel.proposedFix")}</h4>
      <span className="proposed-fix-count">{fix.actionCodes.length} {t("contextPanel.changes")}</span>
    </div>
    <p className="proposed-fix-text">{t(`incident.fix.${fix.code}`)}</p>
    <ol className="proposed-fix-steps">
      {fix.actionCodes.map((code, index) => <li key={code}>{index + 1}. {t(`incident.action.${code}`)}</li>)}
    </ol>
    <div className="proposed-fix-meta">
      <span className="risk-chip"><ShieldAlert size={12} />R{fix.risk} · {t("contextPanel.approvalRequired")}</span>
    </div>
    <Button size="sm" onClick={onReviewPlan}>{t("contextPanel.reviewPlan")}<ExternalLink size={13} /></Button>
  </div>;
}