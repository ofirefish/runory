import { Lightbulb, Play, X } from "lucide-react";
import { useTranslation } from "react-i18next";
import type { TimelineApproval } from "../../../types/agent-v2";

export function AgentApprovalCard({ approval, busy, onApprove, onReject }: {
  approval: TimelineApproval;
  busy?: boolean;
  onApprove: () => void;
  onReject: () => void;
}) {
  const { t } = useTranslation();
  if (approval.kind === "command") {
    return <div className="agent-approval-card agent-command-approval" role="region" aria-label={t("contextPanel.timeline.commandApprovalRequired")}>
      <div className="agent-approval-header">
        <strong>{t("contextPanel.timeline.commandApprovalRequired")}</strong>
        <span className="agent-approval-badges">
          {approval.risk && <span className={`agent-approval-risk risk-${approval.risk}`}>{t(`contextPanel.commandRisk.${approval.risk}`, { defaultValue: approval.risk })}</span>}
          {approval.mutability && <span className="agent-command-mutability">{t(`contextPanel.commandMutability.${approval.mutability}`, { defaultValue: approval.mutability })}</span>}
        </span>
      </div>
      <pre className="agent-command-preview"><code>$ {approval.command ?? ""}</code></pre>
      {approval.reason && <div className="agent-command-why">
        <div className="agent-command-why-title"><Lightbulb size={14} aria-hidden />{t("contextPanel.timeline.whyThisCommand")}</div>
        <p>{approval.reason}</p>
      </div>}
      <div className="agent-approval-actions">
        <button type="button" className="plan-btn plan-btn-primary" disabled={busy} onClick={onApprove}><Play size={13} aria-hidden />{t("contextPanel.timeline.runCommand")}</button>
        <button type="button" className="plan-btn" disabled={busy} onClick={onReject}><X size={13} aria-hidden />{t("contextPanel.timeline.cancelCommand")}</button>
      </div>
    </div>;
  }
  return <div className="agent-approval-card" role="region" aria-label={t("contextPanel.timeline.approvalRequired")}>
    <div className="agent-approval-header">
      <strong>{t("contextPanel.timeline.approvalRequired")}</strong>
      {approval.risk && <span className="agent-approval-risk">{approval.risk}</span>}
    </div>
    <p className="agent-approval-action">
      {approval.kind === "change_set"
        ? t("contextPanel.timeline.changeSetApproval", { title: approval.toolName })
        : approval.toolName}
    </p>
    {approval.reason && <p className="agent-approval-reason">{approval.reason}</p>}
    <div className="agent-approval-actions">
      <button type="button" className="plan-btn" disabled={busy} onClick={onReject}>{t("contextPanel.timeline.reject")}</button>
      <button type="button" className="plan-btn plan-btn-primary" disabled={busy} onClick={onApprove}>{t("contextPanel.timeline.approve")}</button>
    </div>
  </div>;
}

export function AgentCurrentApproach({ summary, expanded, onToggle }: {
  summary?: string;
  expanded: boolean;
  onToggle: () => void;
}) {
  const { t } = useTranslation();
  if (!summary) return null;
  return <div className="agent-current-approach">
    <button type="button" className="agent-current-approach-toggle" onClick={onToggle} aria-expanded={expanded}>
      {t("contextPanel.timeline.currentApproach")}
    </button>
    {expanded && <p>{summary}</p>}
  </div>;
}
