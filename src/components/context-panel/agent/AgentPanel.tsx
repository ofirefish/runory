import { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { appErrorCode } from "../../../lib/app-error";
import {
  approveAgentV2Run,
  bindResumableAgentV2Run,
  cancelAgentV2Run,
  getAgentHistory,
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
import { AgentRunError } from "./AgentRunError";
import { IncidentHistoryPopover } from "./IncidentHistoryPopover";
import { IncidentHistoryPanel } from "./IncidentHistoryPanel";
import type { HistoryItem } from "./incident-history";
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
  const [historySelection, setHistorySelection] = useState<{ serverKey: string; item: HistoryItem } | null>(null);
  const [historyLoading, setHistoryLoading] = useState(false);
  const historyRequest = useRef(0);
  const [conversationTargets, setConversationTargets] = useState<Record<string, string[]>>({});
  const closeHistory = useCallback(() => setHistoryOpen(false), []);
  const [actionBusy, setActionBusy] = useState(false);
  const [nowEpochMs, setNowEpochMs] = useState(Date.now());
  const expanded = useAgentViewStore((store) => store.expanded);
  const toggleExpanded = useAgentViewStore((store) => store.toggleExpanded);
  const lastSeqRef = useRef<Record<string, number>>({});

  const serverKey = profile?.id ?? "none";
  const selectedHistory = historySelection?.serverKey === serverKey ? historySelection.item : null;
  const model = byServer[serverKey] ?? EMPTY;
  const targetMismatch = conversationTargets[serverKey] !== undefined &&
    (conversationTargets[serverKey].length !== 1 || conversationTargets[serverKey][0] !== profile?.id);
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
      if (base.runId !== envelope.runId) return current;
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
    const historyVersion = historyRequest.current;
    void listResumableAgentV2Runs().then(async (items) => {
      if (cancelled || historyVersion !== historyRequest.current || items.length === 0) return;
      const matches = items.filter((item) => item.targetIds.length === 1 && item.targetIds[0] === profile.id);
      const match = matches[matches.length - 1];
      if (!match) return;
      if (["completed", "cancelled", "failed"].includes(match.run.state)) return;
      await bindResumableAgentV2Run(match.run.id, sessionId);
      if (cancelled || historyVersion !== historyRequest.current) return;
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
    if (!sessionId || model.running || actionBusy || historyLoading || targetMismatch) return;
    const trimmed = text.trim();
    if (!trimmed) return;
    if (model.runId && (model.pendingQuestion || ["completed", "failed", "cancelled"].includes(model.runState ?? ""))) {
      setActionBusy(true);
      window.dispatchEvent(new Event("runory:agent-run"));
      try {
        await bindResumableAgentV2Run(model.runId, sessionId);
        await bindSubscription(model.runId);
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
    if (!model.runId || !sessionId || targetMismatch || actionBusy) return;
    setActionBusy(true);
    window.dispatchEvent(new Event("runory:agent-run"));
    const runId = model.runId;
    void bindResumableAgentV2Run(runId, sessionId).then(() => resumeAgentV2Run(runId))
      .catch((error) => patch({ lastErrorCode: appErrorCode(error), running: false }))
      .finally(() => setActionBusy(false));
  };

  const approvalAction = async (approve: boolean) => {
    if (!model.runId || !sessionId || targetMismatch || !model.pendingApproval || actionBusy) return;
    setActionBusy(true);
    window.dispatchEvent(new Event("runory:agent-run"));
    try {
      await bindResumableAgentV2Run(model.runId, sessionId);
      if (approve) await approveAgentV2Run(model.runId);
      else await rejectAgentV2Run(model.runId);
    } catch (error) {
      patch({ lastErrorCode: appErrorCode(error) });
    } finally {
      setActionBusy(false);
    }
  };

  const newConversation = () => {
    historyRequest.current += 1;
    setHistoryLoading(false);
    setConversationTargets((current) => ({ ...current, [serverKey]: profile ? [profile.id] : [] }));
    setHistorySelection(null);
    setHistoryOpen(false);
    if (model.runId) void cancelAgentV2Run(model.runId).catch(() => undefined);
    setByServer((current) => ({ ...current, [serverKey]: emptyTimeline() }));
    lastSeqRef.current[serverKey] = 0;
    window.dispatchEvent(new Event("runory:agent-done"));
  };

  const disconnected = profile !== null && !connected;
  const openHistoryConversation = async (item: HistoryItem) => {
    setHistoryOpen(false);
    const request = ++historyRequest.current;
    if (item.kind !== "run") {
      setHistoryLoading(false);
      setHistorySelection({ serverKey, item });
      return;
    }
    setHistorySelection(null);
    setHistoryLoading(true);
    try {
      const detail = await getAgentHistory(item.id);
      if (request !== historyRequest.current) return;
      setConversationTargets((current) => ({ ...current, [serverKey]: item.targetIds }));
      let timeline = emptyTimeline();
      for (const event of detail.events) timeline = mergeAgentEvent(timeline, event);
      lastSeqRef.current[serverKey] = Math.max(0, ...detail.events.map((event) => event.seq));
      patch({ ...timeline, runId: item.id, runState: detail.run.state,
        running: isRunningState(detail.run.state, false),
        pendingApproval: detail.run.state === "awaiting_approval" ? pendingApprovalFromEvents(detail.events) : undefined });
      if (sessionId && item.targetIds.length === 1 && item.targetIds[0] === profile?.id) {
        await bindResumableAgentV2Run(item.id, sessionId);
        if (request === historyRequest.current) await bindSubscription(item.id);
      }
    } catch (error) {
      if (request === historyRequest.current) patch({ lastErrorCode: appErrorCode(error) });
    } finally {
      if (request === historyRequest.current) setHistoryLoading(false);
    }
  };
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
      runState={selectedHistory ? null : model.runState}
      running={!selectedHistory && running}
      onNewConversation={newConversation}
      onOpenHistory={() => setHistoryOpen((value) => !value)}
      historyOpen={historyOpen}
      onPause={!selectedHistory && !targetMismatch && running && !paused ? pauseRun : undefined}
      onResume={!selectedHistory && !targetMismatch && paused ? resumeRun : undefined}
    />
    {historyOpen && <IncidentHistoryPopover key={`${serverKey}-${sessionId}`} onClose={closeHistory} onSelect={(item) => void openHistoryConversation(item)} targetId={profile?.id ?? null} sessionId={sessionId} />}

    <div className="agent-panel-main">
      {historyLoading ? <p className="history-empty" role="status">{t("common.loading")}</p> : selectedHistory ? <IncidentHistoryPanel key={`${selectedHistory.kind}-${selectedHistory.id}`} item={selectedHistory} onBack={() => setHistorySelection(null)} /> : <>
      {targetMismatch && <p className="history-note">{t("contextPanel.historyTargetMismatch")}</p>}
      {!profile && !hasTimeline && <AgentNoServerState onSelect={onSelectServer} />}
      {disconnected && <AgentDisconnectedPanel state={state} onReconnect={onNewTerminal} />}
      {profile && connected && !hasTimeline && <AgentEmptyState onPrompt={beginRun} />}
      {hasTimeline && <>
        <AgentTimeline
          events={model.events}
          displayContext={displayContext}
          pendingApproval={targetMismatch || !connected ? undefined : model.pendingApproval}
          readOnly={targetMismatch || !connected}
          busy={actionBusy}
          expanded={expanded}
          onToggle={toggleExpanded}
          onApprove={() => void approvalAction(true)}
          onReject={() => void approvalAction(false)}
        />
        <AgentRunFooter events={model.events} nowEpochMs={nowEpochMs} />
      </>}
      <AgentRunError events={model.events} lastErrorCode={model.lastErrorCode} running={running} />
      </>}
    </div>

    {!selectedHistory && <AgentComposer
      onSubmit={beginRun}
      onCancel={cancelRun}
      running={(running || actionBusy) && !paused}
      disabled={!profile || !connected || paused || historyLoading || targetMismatch || model.runState === "awaiting_approval"}
      placeholder={model.pendingQuestion ? t("contextPanel.answerPlaceholder") : t("contextPanel.composerPlaceholder")}
    />}
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
