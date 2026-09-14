import { Pause, Play, RotateCcw, ShieldCheck, Square, X } from "lucide-react";
import { useTranslation } from "react-i18next";
import type {
  FleetApprovalV2,
  FleetEventEnvelopeV2,
  FleetExecutionStrategyV2,
  FleetRunV2,
} from "../../../types/agent-v2";
import { fleetStageProgress } from "./fleet-run-view";

type FleetRunPanelProps = {
  run: FleetRunV2;
  approval?: FleetApprovalV2 | null;
  events?: FleetEventEnvelopeV2[];
  profileNames: Record<string, string>;
  busy?: boolean;
  selectedStrategy?: FleetExecutionStrategyV2;
  onStrategyChange?: (strategy: FleetExecutionStrategyV2) => void;
  onRequestApproval?: () => void;
  onApprove?: () => void;
  onReject?: () => void;
  onStart?: () => void;
  onPause?: () => void;
  onContinue?: () => void;
  onStop?: () => void;
  onRollback?: () => void;
};

const TERMINAL_STATES = new Set([
  "succeeded",
  "failed",
  "rolled_back",
  "rollback_failed",
  "interrupted",
  "cancelled",
]);

/**
 * Pure Fleet projection. It receives durable Rust state/events and emits only
 * explicit user intents; scheduling, policy and target execution stay in Rust.
 */
export function FleetRunPanel({
  run,
  approval,
  events = [],
  profileNames,
  busy = false,
  selectedStrategy,
  onStrategyChange,
  onRequestApproval,
  onApprove,
  onReject,
  onStart,
  onPause,
  onContinue,
  onStop,
  onRollback,
}: FleetRunPanelProps) {
  const { t } = useTranslation();
  const canEditStrategy = run.state === "draft" && Boolean(onStrategyChange);
  const latestEvent = events.at(-1);

  return <section className="fleet-run-panel" aria-label={t("contextPanel.fleet.title")}>
    <header className="fleet-run-summary">
      <div>
        <strong>{t("contextPanel.fleet.title")}</strong>
        <p>{t("contextPanel.fleet.summary", { targets: run.targets.length, stages: run.stages.length })}</p>
      </div>
      <span className={`fleet-state fleet-state-${run.state}`}>{t(`contextPanel.fleet.state.${run.state}`)}</span>
    </header>

    <div className="fleet-policy-row">
      <label htmlFor={`fleet-strategy-${run.id}`}>{t("contextPanel.fleet.strategy")}</label>
      <select
        id={`fleet-strategy-${run.id}`}
        value={selectedStrategy ?? run.stages[0]?.executionStrategy ?? "sequential"}
        disabled={!canEditStrategy || busy}
        onChange={(event) => onStrategyChange?.(event.target.value as FleetExecutionStrategyV2)}
      >
        <option value="sequential">{t("contextPanel.fleet.strategy.sequential")}</option>
        <option value="canary">{t("contextPanel.fleet.strategy.canary")}</option>
        <option value="rolling_batch">{t("contextPanel.fleet.strategy.rolling_batch")}</option>
        {!run.production && <option value="parallel">{t("contextPanel.fleet.strategy.parallel")}</option>}
      </select>
      {run.production && <span className="fleet-policy-note"><ShieldCheck size={12} />{t("contextPanel.fleet.productionPolicy")}</span>}
    </div>

    <ol className="fleet-stage-list">
      {run.stages.map((stage, index) => <li key={stage.id} className="fleet-stage-card">
        <div className="fleet-stage-header">
          <span className="fleet-stage-index">{index + 1}</span>
          <div><strong>{stage.summary}</strong><p>{t(`contextPanel.fleet.stageState.${stage.state}`)}</p></div>
        </div>
        {stage.dependsOn.length > 0 && <p className="fleet-stage-dependency">
          {t("contextPanel.fleet.dependsOn", { count: stage.dependsOn.length })}
        </p>}
        <div className="fleet-target-list">
          {stage.targetIds.map((targetId) => {
            const target = run.targets.find((item) => item.profileId === targetId);
            const child = run.children.find((item) => item.stageId === stage.id && item.targetId === targetId);
            const state = child?.state ?? stage.state;
            return <div className="fleet-target-row" key={targetId}>
              <span className={`fleet-target-dot fleet-target-${fleetStageProgress(state)}`} aria-hidden />
              <span className="fleet-target-name">{profileNames[targetId] ?? targetId}</span>
              {target?.role && <span className="fleet-target-role">{target.role}</span>}
              <span className="fleet-target-state">{t(`contextPanel.fleet.stageState.${state}`)}</span>
              {child?.errorCode && <code>{child.errorCode}</code>}
            </div>;
          })}
        </div>
      </li>)}
    </ol>

    {latestEvent?.code && <p className="fleet-event-code" role="status">{latestEvent.code}</p>}
    <FleetActions
      run={run}
      approval={approval}
      busy={busy}
      onRequestApproval={onRequestApproval}
      onApprove={onApprove}
      onReject={onReject}
      onStart={onStart}
      onPause={onPause}
      onContinue={onContinue}
      onStop={onStop}
      onRollback={onRollback}
    />
  </section>;
}

function FleetActions({ run, approval, busy, onRequestApproval, onApprove, onReject, onStart, onPause, onContinue, onStop, onRollback }: Pick<
  FleetRunPanelProps,
  "run" | "approval" | "busy" | "onRequestApproval" | "onApprove" | "onReject" | "onStart" | "onPause" | "onContinue" | "onStop" | "onRollback"
>) {
  const { t } = useTranslation();
  const pendingApproval = run.state === "awaiting_approval" && approval?.state === "pending";
  const terminal = TERMINAL_STATES.has(run.state);
  return <div className="fleet-actions">
    {run.state === "draft" && onRequestApproval && <button type="button" disabled={busy} onClick={onRequestApproval}>
      <ShieldCheck size={14} />{t("contextPanel.fleet.requestApproval")}
    </button>}
    {pendingApproval && onApprove && <button type="button" disabled={busy} onClick={onApprove}>
      <ShieldCheck size={14} />{t("contextPanel.fleet.approve")}
    </button>}
    {pendingApproval && onReject && <button type="button" className="secondary" disabled={busy} onClick={onReject}>
      <X size={14} />{t("contextPanel.fleet.reject")}
    </button>}
    {run.state === "approved" && onStart && <button type="button" disabled={busy} onClick={onStart}>
      <Play size={14} />{t("contextPanel.fleet.start")}
    </button>}
    {["executing", "verifying"].includes(run.state) && onPause && <button type="button" className="secondary" disabled={busy} onClick={onPause}>
      <Pause size={14} />{t("contextPanel.fleet.pause")}
    </button>}
    {run.state === "paused_for_review" && onContinue && <button type="button" disabled={busy} onClick={onContinue}>
      <Play size={14} />{t("contextPanel.fleet.continue")}
    </button>}
    {!terminal && run.state !== "draft" && onStop && <button type="button" className="danger" disabled={busy} onClick={onStop}>
      <Square size={13} />{t("contextPanel.fleet.stop")}
    </button>}
    {["paused_for_review", "failed"].includes(run.state) && onRollback && <button type="button" className="danger" disabled={busy} onClick={onRollback}>
      <RotateCcw size={14} />{t("contextPanel.fleet.rollback")}
    </button>}
  </div>;
}
