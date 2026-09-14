import { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { appErrorCode } from "../../../lib/app-error";
import {
  approveAgentV2FleetPlan,
  approveAgentV2FleetChangeSet,
  approveAgentV2Run,
  bindResumableAgentV2Run,
  cancelAgentV2FleetPlan,
  cancelAgentV2Run,
  continueAgentV2FleetPlan,
  draftAgentV2FleetPrompt,
  getAgentHistory,
  getAgentV2FleetEvents,
  getAgentV2FleetPlan,
  getLatestAgentV2FleetChangeSet,
  listResumableAgentV2Runs,
  pauseAgentV2FleetPlan,
  pauseAgentV2Run,
  rejectAgentV2FleetPlan,
  executeAgentV2FleetChangeSet,
  rollbackAgentV2FleetChangeSet,
  rejectAgentV2Run,
  replyAgentV2Run,
  requestAgentV2FleetApproval,
  resumeAgentV2Run,
  retryAgentV2Run,
  startAgentV2FleetPlan,
  startAgentV2Run,
  subscribeAgentV2Run,
} from "../../../lib/tauri/agent-v2";
import type { AgentTimelineView, FleetApprovalV2, FleetChangeSetReview, FleetEventEnvelopeV2, FleetRunV2 } from "../../../types/agent-v2";
import type { ServerProfile } from "../../../types/domain";
import type { SessionState } from "../../../types/session";
import { osLogoDictionary } from "../../../features/profiles/os-logo-data";
import { AgentComposer, type AgentComposerSubmitOptions } from "./AgentComposer";
import { AgentEmptyState } from "./AgentEmptyState";
import { AgentHeader } from "./AgentHeader";
import { AgentTimeline } from "./AgentTimeline";
import { AgentRunError, isRetryableAgentError } from "./AgentRunError";
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
import { useCloudIdentityStore } from "../../../stores/cloud-identity-store";
import { useCatalogStore } from "../../../stores/catalog-store";
import { useSessionStore } from "../../../stores/session-store";
import { buildAgentMentionOptions } from "./agent-mention-options";
import { parseFleetMentions, resolveFleetMentions } from "../../../lib/agent/fleet-mentions";
import { FleetRunPanel } from "./FleetRunPanel";
import { FleetChangeSetPanel } from "./FleetChangeSetPanel";

type ServerTimeline = AgentTimelineView;

const EMPTY: ServerTimeline = emptyTimeline();
const TERMINAL_FLEET_STATES = new Set(["succeeded", "failed", "rolled_back", "rollback_failed", "interrupted", "cancelled"]);

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
  const [fleetRun, setFleetRun] = useState<FleetRunV2 | null>(null);
  const [fleetApproval, setFleetApproval] = useState<FleetApprovalV2 | null>(null);
  const [fleetChangeSet, setFleetChangeSet] = useState<FleetChangeSetReview | null>(null);
  const [fleetEvents, setFleetEvents] = useState<FleetEventEnvelopeV2[]>([]);
  const [fleetErrorCode, setFleetErrorCode] = useState<string | null>(null);
  const [nowEpochMs, setNowEpochMs] = useState(Date.now());
  const expanded = useAgentViewStore((store) => store.expanded);
  const toggleExpanded = useAgentViewStore((store) => store.toggleExpanded);
  const userAvatarUrl = useCloudIdentityStore((store) => store.avatarUrl);
  const userDisplayName = useCloudIdentityStore((store) => store.displayName);
  const catalogProfiles = useCatalogStore((store) => store.profiles);
  const catalogGroups = useCatalogStore((store) => store.groups);
  const sessionTabs = useSessionStore((store) => store.tabs);
  const mentionOptions = buildAgentMentionOptions(catalogProfiles, catalogGroups, sessionTabs);
  const lastSeqRef = useRef<Record<string, number>>({});
  const fleetSeqRef = useRef(0);

  const serverKey = profile?.id ?? "none";
  const selectedHistory = historySelection?.serverKey === serverKey ? historySelection.item : null;
  const model = byServer[serverKey] ?? EMPTY;
  const targetMismatch = !fleetRun && conversationTargets[serverKey] !== undefined &&
    (conversationTargets[serverKey].length !== 1 || conversationTargets[serverKey][0] !== profile?.id);
  const phase = timelinePhase(model.events);
  const fleetProfileNames = Object.fromEntries(catalogProfiles.map((item) => [item.id, item.name]));
  const fleetRunId = fleetRun?.id;

  useEffect(() => {
    if (!fleetRunId) return;
    let cancelled = false;
    void getLatestAgentV2FleetChangeSet(fleetRunId)
      .then((review) => { if (!cancelled) setFleetChangeSet(review); })
      .catch((error) => { if (!cancelled) setFleetErrorCode(appErrorCode(error)); });
    return () => { cancelled = true; };
  }, [fleetRunId]);

  useEffect(() => {
    if (model.events.length === 0 || ["completed", "failed", "cancelled"].includes(phase)) return;
    setNowEpochMs(Date.now());
    const timer = window.setInterval(() => setNowEpochMs(Date.now()), 1000);
    return () => window.clearInterval(timer);
  }, [model.events.length, phase]);

  useEffect(() => {
    if (!fleetRun || fleetRun.recoveryState !== "live" || TERMINAL_FLEET_STATES.has(fleetRun.state)) return;
    let cancelled = false;
    let polling = false;
    const poll = async () => {
      if (polling) return;
      polling = true;
      try {
        const [events, review] = await Promise.all([
          getAgentV2FleetEvents(fleetRun.id, fleetSeqRef.current),
          getLatestAgentV2FleetChangeSet(fleetRun.id),
        ]);
        if (cancelled) return;
        setFleetChangeSet(review);
        if (events.length > 0) {
          fleetSeqRef.current = Math.max(fleetSeqRef.current, ...events.map((event) => event.seq));
          setFleetEvents((current) => [...current, ...events.filter((event) => !current.some((item) => item.seq === event.seq))]);
          setFleetRun(await getAgentV2FleetPlan(fleetRun.id));
        }
      } catch (error) {
        if (!cancelled) setFleetErrorCode(appErrorCode(error));
      } finally {
        polling = false;
      }
    };
    void poll();
    const timer = window.setInterval(() => void poll(), 1_000);
    return () => { cancelled = true; window.clearInterval(timer); };
  }, [fleetRun]);

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
          os: profile.osDistribution ? osLogoDictionary[profile.osDistribution].label : "Linux",
          user: profile.username,
          directory: profile.username === "root" ? "/root" : `/home/${profile.username}`,
        },
      });
      void bindSubscription(match.run.id);
    }).catch(() => undefined);
    return () => { cancelled = true; };
  }, [bindSubscription, patch, profile, sessionId]);

  const beginRun = async (text: string, options: AgentComposerSubmitOptions) => {
    if (!sessionId || model.running || actionBusy || historyLoading || targetMismatch) return;
    const trimmed = text.trim();
    if (!trimmed) return;
    const parsedMentions = parseFleetMentions(trimmed);
    if (parsedMentions.mentions.length > 0 || parsedMentions.errors.length > 0) {
      const resolved = resolveFleetMentions(parsedMentions.mentions, catalogProfiles, catalogGroups, sessionTabs);
      const inputError = parsedMentions.errors[0] ?? resolved.errors[0];
      if (inputError) {
        setFleetErrorCode(inputError.code);
        return;
      }
      setActionBusy(true);
      setFleetErrorCode(null);
      window.dispatchEvent(new Event("runory:agent-run"));
      try {
        const run = await draftAgentV2FleetPrompt({
          targets: resolved.targets.map((target) => ({
            profileId: target.profileId,
            sessionId: target.sessionId,
            role: target.role,
          })),
          goal: trimmed,
          production: true,
          executionStrategy: options.fleetStrategy,
          failurePolicy: "pause_for_review",
        });
        fleetSeqRef.current = 0;
        setFleetEvents([]);
        setFleetApproval(null);
        setFleetChangeSet(null);
        setFleetRun(run);
        setConversationTargets((current) => ({ ...current, [serverKey]: run.targets.map((target) => target.profileId) }));
        setByServer((current) => ({ ...current, [serverKey]: emptyTimeline() }));
      } catch (error) {
        setFleetErrorCode(appErrorCode(error));
        window.dispatchEvent(new Event("runory:agent-done"));
      } finally {
        setActionBusy(false);
      }
      return;
    }
    setFleetErrorCode(null);
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

  const retryFailedRun = async () => {
    if (!sessionId || !model.runId || actionBusy || running || targetMismatch) return;
    if (!isRetryableAgentError(model.lastErrorCode)) return;
    setActionBusy(true);
    patch({ lastErrorCode: null, running: true });
    window.dispatchEvent(new Event("runory:agent-run"));
    try {
      await bindResumableAgentV2Run(model.runId, sessionId);
      await bindSubscription(model.runId);
      await retryAgentV2Run(model.runId);
    } catch (error) {
      patch({ lastErrorCode: appErrorCode(error), running: false });
    } finally {
      setActionBusy(false);
    }
  };

  const cancelRun = () => {
    if (!model.runId) return;
    patch({ running: false });
    window.dispatchEvent(new Event("runory:agent-done"));
    void cancelAgentV2Run(model.runId).catch((error) => {
      patch({ lastErrorCode: appErrorCode(error) });
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
    const runId = model.runId;
    const approvalId = model.pendingApproval.approvalId;
    setActionBusy(true);
    patch({ lastErrorCode: null });
    window.dispatchEvent(new Event("runory:agent-run"));
    try {
      await bindResumableAgentV2Run(runId, sessionId);
      if (approve) await approveAgentV2Run(runId, approvalId);
      else await rejectAgentV2Run(runId, approvalId);
    } catch (error) {
      patch({ lastErrorCode: appErrorCode(error) });
    } finally {
      setActionBusy(false);
    }
  };

  const refreshFleet = async (fleetRunId: string) => {
    setFleetRun(await getAgentV2FleetPlan(fleetRunId));
  };

  const fleetRequestApproval = async () => {
    if (!fleetRun || actionBusy) return;
    setActionBusy(true);
    setFleetErrorCode(null);
    try {
      setFleetApproval(await requestAgentV2FleetApproval(fleetRun.id, fleetRun.version));
      await refreshFleet(fleetRun.id);
    } catch (error) {
      setFleetErrorCode(appErrorCode(error));
    } finally {
      setActionBusy(false);
    }
  };

  const fleetApprovalAction = async (approve: boolean) => {
    if (!fleetRun || !fleetApproval || actionBusy) return;
    setActionBusy(true);
    setFleetErrorCode(null);
    try {
      const decided = approve
        ? await approveAgentV2FleetPlan(fleetRun.id, fleetApproval.id)
        : await rejectAgentV2FleetPlan(fleetRun.id, fleetApproval.id);
      setFleetApproval(decided);
      await refreshFleet(fleetRun.id);
    } catch (error) {
      setFleetErrorCode(appErrorCode(error));
    } finally {
      setActionBusy(false);
    }
  };

  const fleetStart = async () => {
    if (!fleetRun || !fleetApproval || actionBusy) return;
    setActionBusy(true);
    setFleetErrorCode(null);
    try {
      await startAgentV2FleetPlan(fleetRun.id, fleetApproval.id, fleetRun.targets.length);
      await refreshFleet(fleetRun.id);
    } catch (error) {
      setFleetErrorCode(appErrorCode(error));
    } finally {
      setActionBusy(false);
    }
  };

  const fleetPause = async () => {
    if (!fleetRun || actionBusy) return;
    setActionBusy(true);
    try {
      await pauseAgentV2FleetPlan(fleetRun.id);
      await refreshFleet(fleetRun.id);
    } catch (error) {
      setFleetErrorCode(appErrorCode(error));
    } finally {
      setActionBusy(false);
    }
  };

  const fleetContinue = async () => {
    if (!fleetRun || !fleetApproval || actionBusy) return;
    setActionBusy(true);
    setFleetErrorCode(null);
    try {
      await continueAgentV2FleetPlan(fleetRun.id, fleetApproval.id);
      await refreshFleet(fleetRun.id);
    } catch (error) {
      setFleetErrorCode(appErrorCode(error));
    } finally {
      setActionBusy(false);
    }
  };

  const fleetStop = async () => {
    if (!fleetRun || actionBusy) return;
    setActionBusy(true);
    try {
      await cancelAgentV2FleetPlan(fleetRun.id);
      await refreshFleet(fleetRun.id);
      window.dispatchEvent(new Event("runory:agent-done"));
    } catch (error) {
      setFleetErrorCode(appErrorCode(error));
    } finally {
      setActionBusy(false);
    }
  };

  const refreshFleetChangeSet = async () => {
    if (!fleetRun) return;
    setFleetChangeSet(await getLatestAgentV2FleetChangeSet(fleetRun.id));
  };

  const fleetChangeAction = async (action: "approve" | "execute" | "rollback") => {
    if (!fleetRun || !fleetChangeSet || actionBusy) return;
    setActionBusy(true);
    setFleetErrorCode(null);
    const execution = fleetChangeSet.execution;
    try {
      if (action === "approve") {
        await approveAgentV2FleetChangeSet(fleetRun.id, fleetRun.version, execution.id, execution.version);
      } else if (action === "execute") {
        await executeAgentV2FleetChangeSet(fleetRun.id, fleetRun.version, execution.id, execution.version);
      } else {
        await rollbackAgentV2FleetChangeSet(fleetRun.id, fleetRun.version, execution.id, execution.version);
      }
      await refreshFleetChangeSet();
    } catch (error) {
      setFleetErrorCode(appErrorCode(error));
      await refreshFleetChangeSet().catch(() => undefined);
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
    if (fleetRun && !TERMINAL_FLEET_STATES.has(fleetRun.state)) void cancelAgentV2FleetPlan(fleetRun.id).catch(() => undefined);
    setFleetRun(null);
    setFleetApproval(null);
    setFleetChangeSet(null);
    setFleetEvents([]);
    setFleetErrorCode(null);
    fleetSeqRef.current = 0;
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
    os: profile.osDistribution ? osLogoDictionary[profile.osDistribution].label : "Linux",
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
      {historyLoading ? <p className="history-empty" role="status">{t("common.loading")}</p> : selectedHistory ? <IncidentHistoryPanel key={`${selectedHistory.kind}-${selectedHistory.id}`} item={selectedHistory} onBack={() => setHistorySelection(null)} /> : fleetRun ? <>
        <FleetRunPanel
          run={fleetRun}
          approval={fleetApproval}
          events={fleetEvents}
          profileNames={fleetProfileNames}
          busy={actionBusy}
          onRequestApproval={() => void fleetRequestApproval()}
          onApprove={() => void fleetApprovalAction(true)}
          onReject={() => void fleetApprovalAction(false)}
          onStart={() => void fleetStart()}
          onPause={() => void fleetPause()}
          onContinue={() => void fleetContinue()}
          onStop={() => void fleetStop()}
        />
        {fleetChangeSet && <FleetChangeSetPanel
          review={fleetChangeSet}
          profileNames={fleetProfileNames}
          busy={actionBusy}
          onApprove={() => void fleetChangeAction("approve")}
          onExecute={() => void fleetChangeAction("execute")}
          onRollback={() => void fleetChangeAction("rollback")}
        />}
        {fleetErrorCode && <p className="agent-error-card" role="alert">{t("contextPanel.fleet.targetError", { code: fleetErrorCode })}</p>}
      </> : <>
      {fleetErrorCode && <p className="agent-error-card" role="alert">{t("contextPanel.fleet.targetError", { code: fleetErrorCode })}</p>}
      {targetMismatch && <p className="history-note">{t("contextPanel.historyTargetMismatch")}</p>}
      {!profile && !hasTimeline && <AgentNoServerState onSelect={onSelectServer} />}
      {disconnected && <AgentDisconnectedPanel state={state} onReconnect={onNewTerminal} />}
      {profile && connected && !hasTimeline && <AgentEmptyState onPrompt={(text) => void beginRun(text, { fleetStrategy: "sequential" })} />}
      {hasTimeline && <>
        <AgentTimeline
          events={model.events}
          displayContext={displayContext}
          userAvatarUrl={userAvatarUrl}
          userDisplayName={userDisplayName}
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
      <AgentRunError events={model.events} lastErrorCode={model.lastErrorCode} running={running} retrying={actionBusy} onRetry={!selectedHistory && !targetMismatch && connected ? () => void retryFailedRun() : undefined} />
      </>}
    </div>

    {!selectedHistory && !fleetRun && <AgentComposer
      onSubmit={beginRun}
      onCancel={cancelRun}
      running={(running || actionBusy) && !paused}
      disabled={!profile || !connected || paused || historyLoading || targetMismatch || model.runState === "awaiting_approval"}
      placeholder={model.pendingQuestion ? t("contextPanel.answerPlaceholder") : t("contextPanel.composerPlaceholder")}
      mentionOptions={mentionOptions}
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
