import { useState } from "react";
import { useTranslation } from "react-i18next";
import type { AgentRun } from "../../../types/agentic";
import { ActivityTimeline } from "./ActivityTimeline";
import { AgentStatus } from "./AgentStatus";
import { DiagnosisCard } from "./DiagnosisCard";
import { ProposedFixCard } from "./ProposedFixCard";
import { InlineChangeSetCard, type InlineChangeSetAction } from "./InlineChangeSetCard";
import type { AgentConversationItem } from "./agent-state";

/**
 * Structured conversation: UserMessage → AgentStatus → ActivityTimeline →
 * Diagnosis / Evidence → ProposedFix. Not a chat-bubble feed.
 */
export function AgentConversation({ items, expanded, onToggle, onReviewPlan, onAnswerQuestion, changeBusyId, changeErrorId, changeErrorCode, onChangeSetAction }: {
  items: AgentConversationItem[];
  expanded: Record<string, boolean>;
  onToggle: (id: string) => void;
  onReviewPlan: (runId: string) => void;
  onAnswerQuestion: (text: string) => void;
  changeBusyId: string | null;
  changeErrorId: string | null;
  changeErrorCode: string | null;
  onChangeSetAction: (runId: string, action: InlineChangeSetAction, stepId?: string) => Promise<void>;
}) {
  const { t } = useTranslation();
  if (items.length === 0) return null;
  return <div className="agent-conversation">
    {items.map((item) => {
      switch (item.kind) {
        case "user":
          return <div key={item.id} className="agent-user-message"><span className="agent-user-label">{t("contextPanel.you")}</span><p>{item.text}</p></div>;
        case "status":
          return <AgentStatus key={item.id} state={item.state} detail={item.text || undefined} />;
        case "run":
          return <div key={item.id} className="agent-block">
            <ActivityTimeline run={item.run} expanded={expanded} onToggle={onToggle} />
            {item.run.answer ? <RunAnswer run={item.run} /> : item.run.diagnosis && <RunDiagnosis run={item.run} />}
            {item.run.changeSet && <InlineChangeSetCard changeSet={item.run.changeSet} busy={changeBusyId === item.run.changeSet.id} errorCode={changeErrorId === item.run.changeSet.id ? changeErrorCode : null} onAction={(action, stepId) => onChangeSetAction(item.run.id, action, stepId)} onReview={() => onReviewPlan(item.run.id)} />}
          </div>;
        case "incident":
          return <div key={item.id} className="agent-block">
            <DiagnosisCard incident={item.incident} />
            <ProposedFixCard incident={item.incident} onReviewPlan={() => onReviewPlan(item.incident.agentRunId)} />
          </div>;
        case "question":
          return <div key={item.id} className="agent-question">
            <AgentStatus state="question" />
            <p className="question-text">{item.text}</p>
            {item.answered ? <p className="question-answered">{t("contextPanel.questionAnswered")}</p> : <QuestionAnswer onAnswer={onAnswerQuestion} />}
          </div>;
        case "repair":
          return <div key={item.id} className="agent-block">
            <div className="repair-card">
              <p className="repair-title">{t("contextPanel.proposedFix")} · {item.changeCount} {t("contextPanel.changes")}</p>
              <p className="repair-risk">{t("contextPanel.riskLabel")}: {item.changeRisk} · {t("contextPanel.approvalRequired")}</p>
              <button type="button" className="review-plan" onClick={() => onReviewPlan(item.runId)}>{t("contextPanel.reviewPlan")}</button>
            </div>
          </div>;
        default:
          return null;
      }
    })}
  </div>;
}

function RunAnswer({ run }: { run: AgentRun }) {
  return <div className="agent-answer">
    <p>{run.answer}</p>
  </div>;
}

/** Minimal diagnosis text from a raw Doctor run (pre-incident engines). */
function RunDiagnosis({ run }: { run: AgentRun }) {
  const { t } = useTranslation();
  const { diagnosis } = run;
  if (!diagnosis) return null;
  return <div className="diagnosis-card">
    <div className="diagnosis-heading"><h4>{t("contextPanel.rootCause")}</h4><span>{run.model}</span></div>
    <p className="diagnosis-cause">{diagnosis.rootCauseCode}</p>
    <p className="diagnosis-confidence">{t("incident.confidence", { value: Math.round(diagnosis.confidence * 100), count: diagnosis.evidenceIds.length })}</p>
  </div>;
}

/** In-conversation answer box for the engine's clarifying question. */
function QuestionAnswer({ onAnswer }: { onAnswer: (text: string) => void }) {
  const { t } = useTranslation();
  const [value, setValue] = useState("");
  const submit = () => {
    const text = value.trim();
    if (!text) return;
    onAnswer(text);
    setValue("");
  };
  return <div className="agent-composer inline">
    <textarea
      rows={1}
      value={value}
      onChange={(event) => setValue(event.target.value)}
      onKeyDown={(event) => {
        if (event.key === "Enter" && !event.shiftKey) { event.preventDefault(); submit(); }
      }}
      placeholder={t("contextPanel.answerPlaceholder")}
      aria-label={t("contextPanel.answerPlaceholder")}
    />
    <button type="button" className="agent-send" disabled={!value.trim()} onClick={submit} aria-label={t("contextPanel.send")}>{t("contextPanel.send")}</button>
  </div>;
}
