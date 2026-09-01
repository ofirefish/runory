import { ExternalLink, Play, RotateCcw, ShieldCheck, ShieldX, Wrench } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import type { ChangeSet } from "../../../types/agentic";
import { Button } from "../../ui/button";
import { DialogShell } from "../../ui/dialog-shell";
import { getInlineChangeSetCapabilities } from "./change-set-actions";

export type InlineChangeSetAction = "approve" | "approve-step" | "reject" | "execute" | "rollback";

export function InlineChangeSetCard({ changeSet, busy, errorCode, onAction, onReview }: {
  changeSet: ChangeSet;
  busy: boolean;
  errorCode: string | null;
  onAction: (action: InlineChangeSetAction, stepId?: string) => Promise<void>;
  onReview: () => void;
}) {
  const { t } = useTranslation();
  const [confirm, setConfirm] = useState<"approve" | "execute" | "rollback" | null>(null);
  const policy = changeSet.policyEvaluation;
  const denied = policy?.decision === "DENY";
  const live = changeSet.recoveryState === "live";
  const capabilities = getInlineChangeSetCapabilities(changeSet);

  const apply = async (action: "approve" | "execute" | "rollback") => {
    await onAction(action);
    setConfirm(null);
  };

  return <section className="inline-changeset" aria-label={t("contextPanel.changeSet.title")}>
    <header className="inline-changeset-head">
      <span className="inline-changeset-icon"><Wrench size={14} /></span>
      <div><h4>{t("contextPanel.changeSet.title")}</h4><p>{changeSet.title}</p></div>
      <span className={`risk-chip risk-${changeSet.risk.toLowerCase()}`}>{changeSet.risk}</span>
    </header>

    <div className="inline-changeset-state">
      <span>v{changeSet.version}</span>
      <span>{t(`contextPanel.changeSet.approval.${changeSet.approvalState}`)}</span>
      <span>{t(`contextPanel.changeSet.execution.${changeSet.executionState}`)}</span>
    </div>

    {policy && <div className={`inline-policy ${denied ? "denied" : "allowed"}`}>
      <strong>{t("contextPanel.changeSet.policy")}</strong>
      <span>{t(`agentic.policyDecision.${policy.decision}`)}</span>
      <small>{t(`agentic.policyReason.${policy.reason}`)}</small>
    </div>}

    <div className="inline-change-steps">
      {changeSet.steps.map((step) => {
        const approved = changeSet.approvedStepIds.includes(step.id);
        return <details key={step.id} open={changeSet.steps.length === 1}>
          <summary><span>{step.order}. {step.toolName}</span><span>{t(`contextPanel.changeSet.step.${step.state}`)}</span></summary>
          <pre>{step.preview}</pre>
          <p>{t("contextPanel.changeSet.verification")}: {step.verificationPlanCode}</p>
          <p>{t("contextPanel.changeSet.rollback")}: {step.rollbackCapability}</p>
          {step.errorCode && <p className="inline-change-error">{step.errorCode}</p>}
          {capabilities.canApproveSteps && !approved && <Button size="sm" disabled={busy || denied} onClick={() => void onAction("approve-step", step.id)}><ShieldCheck size={13} />{t("agentic.approvePolicyStep")}</Button>}
          {approved && <p className="inline-change-approved"><ShieldCheck size={12} />{t("contextPanel.changeSet.stepApproved")}</p>}
        </details>;
      })}
    </div>

    {changeSet.preconditions.length > 0 && <div className="inline-preconditions">
      <strong>{t("contextPanel.changeSet.preconditions")}</strong>
      {changeSet.preconditions.map((item) => <span key={item.stepId}>{item.checkTool} · {t(`contextPanel.changeSet.precondition.${item.state}`)}</span>)}
    </div>}

    {!live && <p className="inline-change-warning">{t("agentic.recoveredMetadataOnly")}</p>}
    {errorCode && <p className="inline-change-error">{t(`contextPanel.changeSet.error.${errorCode}`, { defaultValue: errorCode })}</p>}

    <footer className="inline-change-actions">
      {capabilities.canApprove && <Button size="sm" disabled={busy} onClick={() => setConfirm("approve")}><ShieldCheck size={14} />{t("agent.approve")}</Button>}
      {capabilities.canExecute && <Button size="sm" variant="danger" disabled={busy} onClick={() => setConfirm("execute")}><Play size={14} />{t("agent.execute")}</Button>}
      {capabilities.canRollback && <Button size="sm" variant="danger" disabled={busy} onClick={() => setConfirm("rollback")}><RotateCcw size={14} />{t("contextPanel.changeSet.rollbackAction")}</Button>}
      {capabilities.canReject && <Button size="sm" variant="ghost" disabled={busy} onClick={() => void onAction("reject")}><ShieldX size={14} />{t("contextPanel.changeSet.reject")}</Button>}
      <Button size="sm" variant="ghost" onClick={onReview}>{t("contextPanel.reviewPlan")}<ExternalLink size={13} /></Button>
    </footer>

    {confirm && <DialogShell title={t(`contextPanel.changeSet.confirm.${confirm}.title`)} onClose={() => setConfirm(null)}>
      <p className="text-sm">{t(`contextPanel.changeSet.confirm.${confirm}.body`, { id: changeSet.id, version: changeSet.version })}</p>
      <div className="mt-5 flex justify-end gap-2"><Button variant="ghost" onClick={() => setConfirm(null)}>{t("common.cancel")}</Button><Button variant="danger" disabled={busy} onClick={() => void apply(confirm)}>{t(`contextPanel.changeSet.confirm.${confirm}.action`)}</Button></div>
    </DialogShell>}
  </section>;
}
