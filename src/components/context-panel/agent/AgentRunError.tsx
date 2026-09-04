import { CircleAlert } from "lucide-react";
import { useTranslation } from "react-i18next";
import type { AgentEventEnvelope } from "../../../types/agent-v2";

export function AgentRunError({ events, lastErrorCode, running }: {
  events: AgentEventEnvelope[];
  lastErrorCode: string | null;
  running: boolean;
}) {
  const { t } = useTranslation();
  const latest = [...events].reverse().find((item) => ["run_failed", "run_completed", "run_cancelled", "user_message_added"].includes(item.event.type));
  const failure = latest?.event.type === "run_failed" ? latest : undefined;
  const errorCode = lastErrorCode ?? (failure ? String(failure.event.payload?.error_code ?? "UNKNOWN") : null);
  if (running || !errorCode) return null;
  return <div className="agent-error" role="alert">
    <CircleAlert size={14} aria-hidden />
    <span>{t(`contextPanel.error.${errorCode}`, { defaultValue: t("contextPanel.runFailed") })}</span>
  </div>;
}
