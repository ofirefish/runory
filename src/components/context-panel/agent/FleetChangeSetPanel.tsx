import { Play, RotateCcw, ShieldCheck } from "lucide-react";
import { useTranslation } from "react-i18next";
import type { FleetChangeSetReview } from "../../../types/agent-v2";
import { fleetChangeSetCanRollback } from "./fleet-changeset-view";

type FleetChangeSetPanelProps = {
  review: FleetChangeSetReview;
  profileNames: Record<string, string>;
  busy?: boolean;
  onApprove?: () => void;
  onExecute?: () => void;
  onRollback?: () => void;
};

/** Read-only projection of Rust-generated, exact-version ChangeSets. */
export function FleetChangeSetPanel({
  review,
  profileNames,
  busy = false,
  onApprove,
  onExecute,
  onRollback,
}: FleetChangeSetPanelProps) {
  const { t } = useTranslation();
  const execution = review.execution;
  const canRollback = fleetChangeSetCanRollback(review);

  return <section className="fleet-changeset-panel" aria-label={t("contextPanel.fleet.changeSet.title")}>
    <header className="fleet-changeset-header">
      <div>
        <strong>{t("contextPanel.fleet.changeSet.title")}</strong>
        <p>{t("contextPanel.fleet.changeSet.summary", { count: review.targets.length, version: execution.version })}</p>
      </div>
      <span className="fleet-changeset-risk">{execution.risk}</span>
    </header>

    <div className="fleet-changeset-targets">
      {review.targets.map((target) => <article key={target.changeSet.id} className="fleet-changeset-target">
        <div className="fleet-changeset-target-header">
          <strong>{profileNames[target.profileId] ?? target.profileId}</strong>
          {target.role && <span className="fleet-target-role">{target.role}</span>}
          <span>{t(`contextPanel.changeSet.approval.${target.changeSet.approvalState}`)}</span>
        </div>
        <ol>
          {target.changeSet.steps.map((step) => <li key={step.id}>
            <div><code>{step.toolName}</code><span>{step.risk}</span></div>
            <pre>{step.preview}</pre>
            <p>
              {t("contextPanel.changeSet.verification")}: {t(`contextPanel.changeSet.verificationCode.${step.verificationPlanCode}`)}
              {" · "}{t("contextPanel.changeSet.rollback")}: {t(`contextPanel.changeSet.rollbackCode.${step.rollbackCapability}`)}
            </p>
          </li>)}
        </ol>
      </article>)}
    </div>

    <div className="fleet-actions">
      {execution.approvalState === "draft" && onApprove && <button type="button" disabled={busy} onClick={onApprove}>
        <ShieldCheck size={14} />{t("contextPanel.fleet.changeSet.approve")}
      </button>}
      {execution.approvalState === "approved" && execution.executionState === "approved" && onExecute && <button type="button" disabled={busy} onClick={onExecute}>
        <Play size={14} />{t("contextPanel.fleet.changeSet.execute")}
      </button>}
      {canRollback && onRollback && <button type="button" className="danger" disabled={busy} onClick={onRollback}>
        <RotateCcw size={14} />{t("contextPanel.fleet.rollback")}
      </button>}
    </div>
  </section>;
}
