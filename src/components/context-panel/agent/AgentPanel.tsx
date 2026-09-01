import { CircleAlert } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { appErrorCode } from "../../../lib/app-error";
import { approveChangeSet, approveChangeSetStep, cancelAgentRun, executeChangeSet, getChangeSet, rejectChangeSet, rollbackChangeSet, runDoctor } from "../../../lib/tauri/agentic";
import type { ChangeSet, Incident } from "../../../types/agentic";
import type { ServerProfile } from "../../../types/domain";
import type { SessionState } from "../../../types/session";
import { AgentComposer } from "./AgentComposer";
import { AgentConversation } from "./AgentConversation";
import { AgentEmptyState } from "./AgentEmptyState";
import { AgentHeader } from "./AgentHeader";
import { IncidentHistoryPopover } from "./IncidentHistoryPopover";
import type { InlineChangeSetAction } from "./InlineChangeSetCard";
import type { AgentConversationItem } from "./agent-state";
import { inferDiagnosticInputs } from "./agent-routing";

type ServerSessionModel = {
  items: AgentConversationItem[];
  running: boolean;
  failed: boolean;
  failureCode: string | null;
  changeBusyId: string | null;
  changeErrorId: string | null;
  changeErrorCode: string | null;
  awaiting: boolean;
  pendingText: string;
  activeRunId: string | null;
};

const EMPTY_SESSION: ServerSessionModel = { items: [], running: false, failed: false, failureCode: null, changeBusyId: null, changeErrorId: null, changeErrorCode: null, awaiting: false, pendingText: "", activeRunId: null };

const uid = () => `run-${crypto.randomUUID()}`;

/**
 * Agent Tab — Conversation-first UI bound to the current server.
 *
 * - Composer sends natural language to the Rust Agent Runtime and the active
 *   model provider. No provider secret or tool orchestration lives in React.
 * - Missing parameters surface as an in-conversation question block, never
 *   a huge form.
 * - Tool calls render as an ActivityTimeline (no raw JSON in the panel).
 * - Diagnosis and evidence come from a typed Doctor run. Existing incident
 *   history and ChangeSet review remain available as separate workflows.
 * - Per-server conversation state is memory-only. Model context and remote
 *   evidence are never persisted to browser storage.
 */
export function AgentPanel({ profile, sessionId, connected, state, onNewTerminal, onSelectServer, onReviewPlan }: {
  profile: ServerProfile | null;
  sessionId: string | null;
  connected: boolean;
  state: SessionState;
  onNewTerminal: () => void;
  onSelectServer: () => void;
  onReviewPlan: (runId: string) => void;
}) {
  const { t } = useTranslation();
  const [sessions, setSessions] = useState<Record<string, ServerSessionModel>>({});
  const [expanded, setExpanded] = useState<Record<string, boolean>>({});
  const [historyOpen, setHistoryOpen] = useState(false);

  // The Agent binds to the current server profile id — switching servers
  // switches the conversation context explicitly (scenario 10).
  const serverKey = profile?.id ?? "none";
  const model = sessions[serverKey] ?? EMPTY_SESSION;

  const update = (patch: Partial<ServerSessionModel>) => {
    setSessions((current) => ({ ...current, [serverKey]: { ...(current[serverKey] ?? EMPTY_SESSION), ...patch } }));
  };
  const pushItem = (item: AgentConversationItem) => {
    setSessions((current) => {
      const base = current[serverKey] ?? EMPTY_SESSION;
      return { ...current, [serverKey]: { ...base, items: [...base.items, item] } };
    });
  };
  const finishStatus = (statusId: string, item?: AgentConversationItem) => {
    setSessions((current) => {
      const base = current[serverKey] ?? EMPTY_SESSION;
      const items = base.items.filter((candidate) => candidate.id !== statusId);
      return { ...current, [serverKey]: { ...base, items: item ? [...items, item] : items } };
    });
  };
  const updateStatus = (statusId: string, state: "routing" | "investigating" | "diagnosing", text: string) => {
    setSessions((current) => {
      const base = current[serverKey] ?? EMPTY_SESSION;
      return { ...current, [serverKey]: { ...base, items: base.items.map((item) => item.kind === "status" && item.id === statusId ? { ...item, state, text } : item) } };
    });
  };

  const runIntent = async (text: string) => {
    if (!sessionId) return;
    const activeRunId = crypto.randomUUID();
    update({ activeRunId });
    const statusId = uid();
    pushItem({ kind: "status", id: statusId, state: "investigating", text: "", at: Date.now() });
    window.dispatchEvent(new Event("runory:agent-run"));
    try {
      const inferred = inferDiagnosticInputs(text);
      const run = await runDoctor({
        runId: activeRunId,
        sessionId,
        userRequest: text,
        service: inferred.service,
        httpUrl: inferred.url,
        portHost: null,
        port: null,
        includeNginxTest: inferred.includeNginxTest,
        skillId: null,
        mcpContext: null,
      }, (event) => {
        const state = event.stage === "planning" ? "routing" : event.stage === "drafting-change-set" ? "diagnosing" : "investigating";
        const detail = event.stage === "running-tools"
          ? t("contextPanel.progress.runningTools", { tools: event.toolNames.join(", ") })
          : t(`contextPanel.progress.${event.stage}`);
        updateStatus(statusId, state, detail);
      });
      finishStatus(statusId, { kind: "run", id: uid(), run, at: Date.now() });
      if (run.state === "needs-input" && run.clarificationQuestion) {
        pushItem({ kind: "question", id: uid(), text: run.clarificationQuestion, answered: false, at: Date.now() });
        update({ running: false, awaiting: true, pendingText: text, failed: false, failureCode: null, activeRunId: null });
      } else {
        const runFailureCode = run.failureCode ?? failureCodeForState(run.state);
        update({ running: false, awaiting: false, pendingText: "", failed: runFailureCode !== null, failureCode: runFailureCode, activeRunId: null });
      }
    } catch (error) {
      finishStatus(statusId);
      update({ running: false, failed: true, failureCode: appErrorCode(error), activeRunId: null });
    } finally {
      window.dispatchEvent(new Event("runory:agent-done"));
    }
  };

  const beginRun = (text: string) => {
    const trimmed = text.trim();
    if (!trimmed || model.running) return;
    if (model.awaiting && model.pendingText) {
      // Answer to the engine's in-conversation question: merge and re-run.
      setSessions((current) => {
        const base = current[serverKey] ?? EMPTY_SESSION;
        let marked = false;
        const items = [...base.items].reverse().map((item) => {
          if (!marked && item.kind === "question" && !item.answered) { marked = true; return { ...item, answered: true }; }
          return item;
        }).reverse();
        return { ...current, [serverKey]: { ...base, items } };
      });
      pushItem({ kind: "user", id: uid(), text: trimmed, at: Date.now() });
      update({ awaiting: false, running: true, failed: false, failureCode: null });
      void runIntent(`${model.pendingText} — ${trimmed}`);
      return;
    }
    pushItem({ kind: "user", id: uid(), text: trimmed, at: Date.now() });
    update({ running: true, failed: false, failureCode: null, pendingText: trimmed });
    void runIntent(trimmed);
  };

  const newConversation = () => {
    setSessions((current) => ({ ...current, [serverKey]: { ...EMPTY_SESSION } }));
    setExpanded({});
  };

  const cancelRun = () => {
    if (!model.activeRunId) return;
    const runId = model.activeRunId;
    update({ activeRunId: null });
    void cancelAgentRun(runId).catch(() => update({ activeRunId: runId }));
  };
  const toggleItem = (id: string) => setExpanded((current) => ({ ...current, [id]: !current[id] }));

  const replaceRunChangeSet = (runId: string, changeSet: ChangeSet) => {
    setSessions((current) => {
      const base = current[serverKey] ?? EMPTY_SESSION;
      return { ...current, [serverKey]: { ...base, items: base.items.map((item) => item.kind === "run" && item.run.id === runId ? { ...item, run: { ...item.run, changeSet } } : item) } };
    });
  };

  const changeSetAction = async (runId: string, action: InlineChangeSetAction, stepId?: string) => {
    const runItem = model.items.find((item) => item.kind === "run" && item.run.id === runId);
    const changeSet = runItem?.kind === "run" ? runItem.run.changeSet : null;
    if (!changeSet || model.changeBusyId) return;
    update({ changeBusyId: changeSet.id, changeErrorId: null, changeErrorCode: null });
    window.dispatchEvent(new Event("runory:agent-run"));
    try {
      let next: ChangeSet;
      if (action === "approve") next = await approveChangeSet(changeSet.id, changeSet.version);
      else if (action === "approve-step" && stepId) next = await approveChangeSetStep(changeSet.id, changeSet.version, stepId);
      else if (action === "reject") next = await rejectChangeSet(changeSet.id, changeSet.version);
      else if (action === "execute") next = await executeChangeSet(changeSet.id, changeSet.version);
      else if (action === "rollback") next = await rollbackChangeSet(changeSet.id, changeSet.version);
      else return;
      replaceRunChangeSet(runId, next);
      update({ changeBusyId: null, changeErrorId: null, changeErrorCode: null });
    } catch (error) {
      try { replaceRunChangeSet(runId, await getChangeSet(changeSet.id)); } catch { /* Preserve the last known safe snapshot. */ }
      update({ changeBusyId: null, changeErrorId: changeSet.id, changeErrorCode: appErrorCode(error) });
    } finally {
      window.dispatchEvent(new Event("runory:agent-done"));
    }
  };

  const resumeIncident = (incident: Incident) => {
    pushItem({ kind: "incident", id: uid(), incident, at: Date.now() });
  };

  const disconnected = profile !== null && !connected;

  return <div className="agent-panel">
    <AgentHeader profile={profile} connected={connected} state={state} onNewConversation={newConversation} onOpenHistory={() => setHistoryOpen(true)} />
    <IncidentHistoryPopover open={historyOpen} onClose={() => setHistoryOpen(false)} onSelect={resumeIncident} />

    <div className="agent-panel-main">
      {!profile && <AgentNoServerState onSelect={onSelectServer} />}
      {disconnected && <AgentDisconnectedPanel state={state} onReconnect={onNewTerminal} />}
      {profile && connected && model.items.length === 0 && <AgentEmptyState onPrompt={beginRun} />}
      {profile && connected && model.items.length > 0 && (
        <>
          <AgentConversation
            items={model.items}
            expanded={expanded}
            onToggle={toggleItem}
            onReviewPlan={onReviewPlan}
            onAnswerQuestion={beginRun}
            changeBusyId={model.changeBusyId}
            changeErrorId={model.changeErrorId}
            changeErrorCode={model.changeErrorCode}
            onChangeSetAction={changeSetAction}
          />
        </>
      )}
      {model.failed && !model.running && <div className="agent-error" role="alert"><CircleAlert size={14} aria-hidden /><span>{t(`contextPanel.error.${model.failureCode ?? "UNKNOWN"}`, { defaultValue: t("contextPanel.runFailed") })}</span></div>}
    </div>

    <AgentComposer onSubmit={beginRun} onCancel={cancelRun} running={model.running} disabled={!profile || !connected} />
  </div>;
}

function failureCodeForState(state: string): string | null {
  if (state === "timed-out") return "AGENT_TIMEOUT";
  if (state === "budget-exceeded") return "BUDGET_EXCEEDED";
  if (state === "policy-blocked") return "POLICY_BLOCKED";
  return state === "failed" ? "UNKNOWN" : null;
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
