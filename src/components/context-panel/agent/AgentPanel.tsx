import { CircleAlert } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { appErrorCode } from "../../../lib/app-error";
import {
  approveAgentV2Run,
  bindResumableAgentV2Run,
  cancelAgentV2Run,
  listResumableAgentV2Runs,
  pauseAgentV2Run,
  rejectAgentV2Run,
  replyAgentV2Run,
  resumeAgentV2Run,
  startAgentV2Run,
  subscribeAgentV2Run,
} from "../../../lib/tauri/agent-v2";
import type { AgentTimelineView } from "../../../types/agent-v2";
import type { ServerProfile } from "../../../types/domain";
import type { SessionState } from "../../../types/session";
import { AgentComposer } from "./AgentComposer";
import { AgentEmptyState } from "./AgentEmptyState";
import { AgentHeader } from "./AgentHeader";
import { AgentTimeline } from "./AgentTimeline";
import { IncidentHistoryPopover } from "./IncidentHistoryPopover";
import {
  deriveRunState,
  AGENT_MAX_ROUNDS,
  emptyTimeline,
  isRunningState,
  mergeAgentEvent,
  pendingApprovalFromEvents,
  reasoningRound,
  runElapsedSeconds,
  timelinePhase,
} from "./agent-timeline-utils";
import { useAgentViewStore } from "./agent-view-store";

type ServerTimeline = AgentTimelineView;

const EMPTY: ServerTimeline = emptyTimeline();

/**
 * Agent Tab — event-driven Timeline bound to Runtime V2 IPC.
 * React renders AgentEvent streams only; orchestration stays in Rust.
 */
export function AgentPanel({ profile, sessionId, connected, state, onNewTerminal, onSelectServer }: {
  profile: ServerProfile | null;
  sessionId: string | null;
  connected: boolean;
  state: SessionState;
  onNewTerminal: () => void;
  onSelectServer: () => void;
}) {
  const { t } = useTranslation();
  const [byServer, setByServer] = useState<Record<string, ServerTimeline>>({});
  const [historyOpen, setHistoryOpen] = useState(false);
  const [actionBusy, setActionBusy] = useState(false);
  const [nowEpochMs, setNowEpochMs] = useState(Date.now());
  const expanded = useAgentViewStore((store) => store.expanded);
  const toggleExpanded = useAgentViewStore((store) => store.toggleExpanded);
  const lastSeqRef = useRef<Record<string, number>>({});

  const serverKey = profile?.id ?? "none";
  const model = byServer[serverKey] ?? EMPTY;
  const phase = timelinePhase(model.events);

  useEffect(() => {
    if (model.events.length === 0 || ["completed", "failed", "cancelled"].includes(phase)) return;
    setNowEpochMs(Date.now());
    const timer = window.setInterval(() => setNowEpochMs(Date.now()), 1000);
    return () => window.clearInterval(timer);
  }, [model.events.length, phase]);

  const patch = useCallback((update: Partial<ServerTimeline>) => {
    setByServer((current) => ({
      ...current,
      [serverKey]: { ...(current[serverKey] ?? EMPTY), ...update },
    }));
  }, [serverKey]);

  const appendEvent = useCallback((envelope: Parameters<typeof mergeAgentEvent>[1]) => {
    setByServer((current) => {
      const base = current[serverKey] ?? EMPTY;
      const merged = mergeAgentEvent(base, envelope);
      lastSeqRef.current[serverKey] = Math.max(lastSeqRef.current[serverKey] ?? 0, envelope.seq);
      const next: ServerTimeline = {
        ...merged,
        runState: deriveRunState(merged.events),
        pendingApproval: pendingApprovalFromEvents(merged.events),
      };
      if (next.runState === "completed" || next.runState === "failed" || next.runState === "cancelled") {
        next.running = false;
        window.dispatchEvent(new Event("runory:agent-done"));
      }
      return { ...current, [serverKey]: next };
    });
  }, [serverKey]);

  const bindSubscription = useCallback(async (runId: string) => {
    const afterSeq = lastSeqRef.current[serverKey] ?? 0;
    await subscribeAgentV2Run(runId, afterSeq, appendEvent);
  }, [appendEvent, serverKey]);

  useEffect(() => {
    if (!sessionId || !profile) return;
    let cancelled = false;
    void listResumableAgentV2Runs().then(async (items) => {
      if (cancelled || items.length === 0) return;
      const matches = items.filter((item) => item.targetIds.length === 1 && item.targetIds[0] === profile.id);
      const match = matches[matches.length - 1];
      if (!match) return;
      if (["completed", "cancelled", "failed"].includes(match.run.state)) return;
      await bindResumableAgentV2Run(match.run.id, sessionId);
      patch({
        runId: match.run.id,
        running: ["running", "reasoning", "acting"].includes(match.run.state),
        runState: match.run.state,
        displayContext: {
          os: "Linux",
          user: profile.username,
          directory: profile.username === "root" ? "/root" : `/home/${profile.username}`,
        },
      });
      void bindSubscription(match.run.id);
    }).catch(() => undefined);
    return () => { cancelled = true; };
  }, [bindSubscription, patch, profile, sessionId]);

  const beginRun = async (text: string) => {
    if (!sessionId || model.running || actionBusy) return;
    const trimmed = text.trim();
    if (!trimmed) return;
    if (model.pendingQuestion && model.runId) {
      setActionBusy(true);
      window.dispatchEvent(new Event("runory:agent-run"));
      try {
        await replyAgentV2Run(model.runId, trimmed);
      } catch (error) {
        patch({ lastErrorCode: appErrorCode(error), running: false });
      } finally {
        setActionBusy(false);
      }
      return;
    }
    patch({ running: true, lastErrorCode: null, events: [], currentApproach: undefined, pendingApproval: undefined, pendingQuestion: undefined });
    window.dispatchEvent(new Event("runory:agent-run"));
    try {
      const response = await startAgentV2Run(sessionId, trimmed);
      lastSeqRef.current[serverKey] = 0;
      patch({ runId: response.runId, running: true, displayContext: response.context });
      await bindSubscription(response.runId);
    } catch (error) {
      patch({ running: false, lastErrorCode: appErrorCode(error) });
      window.dispatchEvent(new Event("runory:agent-done"));
    }
  };

  const cancelRun = () => {
    if (!model.runId) return;
    void cancelAgentV2Run(model.runId).finally(() => {
      patch({ running: false });
      window.dispatchEvent(new Event("runory:agent-done"));
    });
  };

  const pauseRun = () => {
    if (!model.runId || actionBusy) return;
    setActionBusy(true);
    void pauseAgentV2Run(model.runId)
      .catch((error) => patch({ lastErrorCode: appErrorCode(error) }))
      .finally(() => setActionBusy(false));
  };

  const resumeRun = () => {
    if (!model.runId || actionBusy) return;
    setActionBusy(true);
    window.dispatchEvent(new Event("runory:agent-run"));
    void resumeAgentV2Run(model.runId)
      .catch((error) => patch({ lastErrorCode: appErrorCode(error), running: false }))
      .finally(() => setActionBusy(false));
  };

  const approvalAction = async (approve: boolean) => {
    if (!model.runId || !model.pendingApproval || actionBusy) return;
    setActionBusy(true);
    window.dispatchEvent(new Event("runory:agent-run"));
    try {
      if (approve) await approveAgentV2Run(model.runId);
      else await rejectAgentV2Run(model.runId);
    } catch (error) {
      patch({ lastErrorCode: appErrorCode(error) });
    } finally {
      setActionBusy(false);
    }
  };

  const newConversation = () => {
    if (model.runId) void cancelAgentV2Run(model.runId).catch(() => undefined);
    setByServer((current) => ({ ...current, [serverKey]: emptyTimeline() }));
    lastSeqRef.current[serverKey] = 0;
    window.dispatchEvent(new Event("runory:agent-done"));
  };

  const disconnected = profile !== null && !connected;
  const running = isRunningState(model.runState, model.running);
  const paused = model.runState === "paused";
  const hasTimeline = model.events.length > 0;
  const displayContext = model.displayContext ?? (profile ? {
    os: "Linux",
    user: profile.username,
    directory: profile.username === "root" ? "/root" : `/home/${profile.username}`,
  } : undefined);

  return <div className="agent-panel">
    <AgentHeader
      profile={profile}
      connected={connected}
      state={state}
      runState={model.runState}
      running={running}
      onNewConversation={newConversation}
      onOpenHistory={() => setHistoryOpen(true)}
      onCancel={cancelRun}
      onPause={running && !paused ? pauseRun : undefined}
      onResume={paused ? resumeRun : undefined}
    />
    <IncidentHistoryPopover open={historyOpen} onClose={() => setHistoryOpen(false)} onSelect={() => undefined} />

    <div className="agent-panel-main">
      {!profile && <AgentNoServerState onSelect={onSelectServer} />}
      {disconnected && <AgentDisconnectedPanel state={state} onReconnect={onNewTerminal} />}
      {profile && connected && !hasTimeline && <AgentEmptyState onPrompt={beginRun} />}
      {profile && connected && hasTimeline && <>
        <AgentTimeline
          events={model.events}
          displayContext={displayContext}
          pendingApproval={model.pendingApproval}
          busy={actionBusy}
          expanded={expanded}
          onToggle={toggleExpanded}
          onApprove={() => void approvalAction(true)}
          onReject={() => void approvalAction(false)}
        />
        <AgentRunFooter events={model.events} nowEpochMs={nowEpochMs} />
      </>}
      {model.lastErrorCode && !running && <div className="agent-error" role="alert"><CircleAlert size={14} aria-hidden /><span>{t(`contextPanel.error.${model.lastErrorCode}`, { defaultValue: t("contextPanel.runFailed") })}</span></div>}
    </div>

    <AgentComposer
      onSubmit={beginRun}
      onCancel={cancelRun}
      running={(running || actionBusy) && !paused}
      disabled={!profile || !connected || paused}
      placeholder={model.pendingQuestion ? t("contextPanel.answerPlaceholder") : t("contextPanel.composerPlaceholder")}
    />
  </div>;
}

function AgentRunFooter({ events, nowEpochMs }: { events: AgentTimelineView["events"]; nowEpochMs: number }) {
  const { t } = useTranslation();
  const phase = timelinePhase(events);
  if (["completed", "failed", "cancelled"].includes(phase)) return null;
  const label = phase === "awaiting_approval"
    ? t("contextPanel.timeline.phaseAwaitingApproval")
    : phase === "executing"
      ? t("contextPanel.timeline.phaseExecuting")
      : phase === "analyzing"
        ? t("contextPanel.timeline.phaseAnalyzing")
        : t("contextPanel.timeline.phaseThinking");
  return <div className="agent-run-footer" aria-live="polite">
    <span>{reasoningRound(events)}/{AGENT_MAX_ROUNDS}</span>
    <span>{runElapsedSeconds(events, nowEpochMs)}s</span>
    <span>{label}</span>
  </div>;
}

function AgentNoServerState({ onSelect }: { onSelect: () => void }) {
  const { t } = useTranslation();
  return <div className="agent-no-server">
    <strong>{t("contextPanel.agentName")}</strong>
    <p>{t("contextPanel.noServerHint")}</p>
    <button type="button" className="select-server-btn" onClick={onSelect}>{t("contextPanel.selectServer")}</button>
  </div>;
}

function AgentDisconnectedPanel({ state, onReconnect }: { state: SessionState; onReconnect: () => void }) {
  const { t } = useTranslation();
  return <div className="agent-disconnected">
    <p><i className={`status-dot status-${state}`} />{t("contextPanel.disconnectedHint")}</p>
    <button type="button" className="select-server-btn" onClick={onReconnect}>{t("contextPanel.reconnect")}</button>
  </div>;
}
