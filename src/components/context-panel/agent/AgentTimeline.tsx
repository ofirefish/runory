import { ChevronDown, ChevronRight, UserRound } from "lucide-react";
import { useTranslation } from "react-i18next";
import type { AgentEventEnvelope, AgentV2DisplayContext, TimelineApproval } from "../../../types/agent-v2";
import { toolLabelKey } from "./agent-timeline-utils";
import { AgentApprovalCard } from "./AgentApprovalCard";
import { AgentMarkdown } from "./AgentMarkdown";

const COMPACT_TOOL_EVENTS = new Set([
  "tool_completed",
  "tool_failed",
]);
export function AgentTimeline({ events, displayContext, userAvatarUrl, pendingApproval, busy, expanded, onToggle, onApprove, onReject, readOnly = false }: {
  events: AgentEventEnvelope[];
  displayContext?: AgentV2DisplayContext;
  userAvatarUrl?: string | null;
  pendingApproval?: TimelineApproval;
  busy?: boolean;
  readOnly?: boolean;
  expanded: Record<string, boolean>;
  onToggle: (key: string) => void;
  onApprove: () => void;
  onReject: () => void;
}) {
  const { t } = useTranslation();
  if (events.length === 0) return null;
  return <div className="agent-timeline" aria-label={t("contextPanel.timeline.label")}>
    {events.map((envelope) => {
      const key = `${envelope.runId}-${envelope.seq}`;
      const type = envelope.event.type;
      const payload = envelope.event.payload ?? {};
      if (type === "user_message_added") {
        return <div key={key} className="agent-user-turn">
          <div className="agent-user-message"><p>{String(payload.content ?? "")}</p></div>
          <span className="agent-user-avatar" aria-hidden>
            {userAvatarUrl ? <img src={userAvatarUrl} alt="" /> : <UserRound size={13} />}
          </span>
          {displayContext && <p className="agent-runtime-context">
            <span>OS: {displayContext.os}</span>
            <span>User: {displayContext.user}</span>
            <span>Dir: {displayContext.directory}</span>
          </p>}
        </div>;
      }
      if (type === "assistant_message_added") {
        const text = String(payload.content ?? "");
        if (isDuplicateCommandAnalysis(events, envelope.seq, text)) return null;
        return <div key={key} className="agent-answer"><AgentMarkdown content={text} /></div>;
      }
      if (type === "command_analysis_updated") {
        const text = String(payload.summary ?? "");
        return text ? <div key={key} className="agent-answer"><AgentMarkdown content={text} /></div> : null;
      }
      if (type === "command_proposed") {
        const commandId = String(payload.command_id ?? "");
        const result = commandResult(events, commandId);
        if (result) {
          return <TimelineCommandRow key={key} envelope={result} command={String(payload.command ?? "")} />;
        }
        if (readOnly || events.some((item) => item.seq > envelope.seq && ["run_completed", "run_failed", "run_cancelled"].includes(item.event.type))) {
          return <div key={key} className="agent-command-result-card"><p>{t("contextPanel.historyCommand")}</p><pre className="agent-command-result-command"><code>{String(payload.command ?? "")}</code></pre><p>{String(payload.reason ?? "")}</p></div>;
        }
        if (pendingApproval?.kind === "command" && pendingApproval.toolCallId === commandId) {
          return <AgentApprovalCard key={key} approval={pendingApproval} busy={busy} onApprove={onApprove} onReject={onReject} />;
        }
        return <TimelineCommandRunning key={key} command={String(payload.command ?? "")} />;
      }
      if (COMPACT_TOOL_EVENTS.has(type)) {
        return <TimelineToolRow
          key={key}
          envelope={envelope}
          toolName={requestedToolName(events, envelope)}
          expanded={expanded[key]}
          onToggle={() => onToggle(key)}
        />;
      }
      if (type === "observation_added") {
        return <div key={key} className="timeline-observation"><span>{String(payload.summary ?? "")}</span></div>;
      }
      if (type === "facts_updated") {
        const keys = Array.isArray(payload.fact_keys) ? payload.fact_keys.map(String) : [];
        return <TimelineChangeSetRow key={key} label={t("contextPanel.timeline.factsUpdated")} detail={keys.slice(0, 4).join(", ")} />;
      }
      if (type === "user_input_required") {
        return <div key={key} className="agent-question"><p className="question-text">{String(payload.question ?? "")}</p></div>;
      }
      if (type === "change_set_proposed") {
        return <TimelineChangeSetRow key={key} label={t("contextPanel.timeline.changeSetProposed")} detail={String(payload.change_set_id ?? "")} />;
      }
      if (type === "change_set_approved") {
        return <TimelineChangeSetRow key={key} label={t("contextPanel.timeline.changeSetApproved")} detail={`v${String(payload.version ?? "")}`} />;
      }
      if (type === "change_set_execution_started") {
        return <TimelineChangeSetRow key={key} label={t("contextPanel.timeline.changeSetExecuting")} />;
      }
      if (type === "change_set_execution_completed") {
        const success = payload.success !== false;
        return <TimelineChangeSetRow key={key} label={success ? t("contextPanel.timeline.changeSetExecuted") : t("contextPanel.timeline.changeSetExecutionFailed")} success={success} />;
      }
      if (type === "verification_started") {
        return <TimelineChangeSetRow key={key} label={t("contextPanel.timeline.verificationStarted")} />;
      }
      if (type === "verification_completed") {
        const success = payload.success !== false;
        return <TimelineChangeSetRow key={key} label={success ? t("contextPanel.timeline.verificationPassed") : t("contextPanel.timeline.verificationFailed")} success={success} />;
      }
      if (type === "rollback_started") {
        return <TimelineChangeSetRow key={key} label={t("contextPanel.timeline.rollbackStarted")} />;
      }
      if (type === "rollback_completed") {
        const success = payload.success !== false;
        return <TimelineChangeSetRow key={key} label={success ? t("contextPanel.timeline.rollbackCompleted") : t("contextPanel.timeline.rollbackFailed")} success={success} />;
      }
      if (type === "run_paused") {
        return <TimelineChangeSetRow key={key} label={t("contextPanel.timeline.paused")} />;
      }
      if (type === "run_resumed") {
        return null;
      }
      return null;
    })}
    {!readOnly && pendingApproval && pendingApproval.kind !== "command" && <AgentApprovalCard approval={pendingApproval} busy={busy} onApprove={onApprove} onReject={onReject} />}
  </div>;
}

function isDuplicateCommandAnalysis(events: AgentEventEnvelope[], assistantSeq: number, text: string): boolean {
  for (let index = events.length - 1; index >= 0; index -= 1) {
    const candidate = events[index];
    if (candidate.seq >= assistantSeq) continue;
    if (candidate.event.type === "command_analysis_updated") {
      return String(candidate.event.payload?.summary ?? "") === text;
    }
    if (candidate.event.type === "assistant_message_added" || candidate.event.type === "user_message_added") {
      return false;
    }
  }
  return false;
}

function commandResult(events: AgentEventEnvelope[], commandId: string): AgentEventEnvelope | undefined {
  return events.find((candidate) => (candidate.event.type === "command_completed" || candidate.event.type === "command_failed")
    && String(candidate.event.payload?.command_id ?? "") === commandId);
}

function requestedToolName(events: AgentEventEnvelope[], current: AgentEventEnvelope): string {
  const toolCallId = String(current.event.payload?.tool_call_id ?? "");
  for (let index = events.length - 1; index >= 0; index -= 1) {
    const candidate = events[index];
    if (candidate.seq >= current.seq || candidate.event.type !== "tool_requested") continue;
    const payload = candidate.event.payload ?? {};
    if (String(payload.tool_call_id ?? "") === toolCallId && typeof payload.tool_name === "string") {
      return payload.tool_name;
    }
  }
  return toolCallId || "tool";
}

function TimelineChangeSetRow({ label, detail, success }: { label: string; detail?: string; success?: boolean }) {
  const mark = success === undefined ? "●" : success ? "✓" : "✕";
  const markClass = success === undefined ? "neutral" : success ? "success" : "error";
  return <div className="timeline-changeset">
    <span className={`activity-mark ${markClass}`} aria-hidden>{mark}</span>
    <span>{label}</span>
    {detail && <span className="timeline-changeset-detail font-mono">{detail}</span>}
  </div>;
}

function TimelineToolRow({ envelope, toolName, expanded, onToggle }: {
  envelope: AgentEventEnvelope;
  toolName: string;
  expanded?: boolean;
  onToggle: () => void;
}) {
  const { t } = useTranslation();
  const type = envelope.event.type;
  const payload = envelope.event.payload ?? {};
  const labelKey = toolLabelKey(toolName);
  const label = labelKey.startsWith("contextPanel.") ? t(labelKey, { defaultValue: toolName }) : toolName;
  const success = type !== "tool_failed";
  const mark = success ? "✓" : "✕";
  const durationMs = typeof payload.duration_ms === "number" ? payload.duration_ms : undefined;
  return <div className={`activity-item compact${expanded ? " expanded" : ""}`}>
    <button type="button" className="activity-item-row" onClick={onToggle} aria-expanded={Boolean(expanded)}>
      <span className={`activity-mark ${success ? "success" : "error"}`} aria-hidden>{mark}</span>
      <span className="activity-label">{label}</span>
      <span className="activity-chevron" aria-hidden>{expanded ? <ChevronDown size={12} /> : <ChevronRight size={12} />}</span>
    </button>
    {expanded && <div className="activity-detail"><dl>
      <div><dt>{t("contextPanel.activity.tool")}</dt><dd className="font-mono">{toolName}</dd></div>
      {durationMs !== undefined && <div><dt>{t("contextPanel.activity.duration")}</dt><dd>{durationMs} ms</dd></div>}
      {!success && typeof payload.error_code === "string" && <div><dt>{t("contextPanel.activity.result")}</dt><dd>{payload.error_code}</dd></div>}
      {typeof payload.chunk === "string" && <div><dt>{t("contextPanel.timeline.result")}</dt><dd>{payload.chunk}</dd></div>}
    </dl></div>}
  </div>;
}

function TimelineCommandRunning({ command }: { command: string }) {
  const { t } = useTranslation();
  return <div className="agent-command-result-card running">
    <div className="agent-command-result-header"><span className="timeline-dot" aria-hidden /><span>{t("contextPanel.timeline.commandRunning")}</span></div>
    <pre className="agent-command-result-command"><code>$ {command}</code></pre>
  </div>;
}

function TimelineCommandRow({ envelope, command }: {
  envelope: AgentEventEnvelope;
  command: string;
}) {
  const { t } = useTranslation();
  const payload = envelope.event.payload ?? {};
  const success = envelope.event.type === "command_completed";
  const exitCode = payload.exit_code;
  const output = typeof payload.output_preview === "string" ? payload.output_preview : "";
  const durationMs = typeof payload.duration_ms === "number" ? payload.duration_ms : undefined;
  return <div className={`agent-command-result-card ${success ? "success" : "failed"}`}>
    <div className="agent-command-result-header">
      <span className={`activity-mark ${success ? "success" : "error"}`} aria-hidden>{success ? "✓" : "✕"}</span>
      <span className="activity-label">{success ? t("contextPanel.timeline.commandCompleted") : t("contextPanel.timeline.commandFailed")}</span>
      {exitCode !== undefined && exitCode !== null && <span className="timeline-command-exit">exit {String(exitCode)}</span>}
      {durationMs !== undefined && <span className="timeline-command-duration">{durationMs} ms</span>}
    </div>
    <pre className="agent-command-result-command"><code>$ {command}</code></pre>
    {output && <pre className="agent-command-output-preview"><code>{output}</code></pre>}
  </div>;
}
