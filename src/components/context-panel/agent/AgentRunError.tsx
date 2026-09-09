import { CircleAlert } from "lucide-react";
import { useTranslation } from "react-i18next";
import type { AgentEventEnvelope } from "../../../types/agent-v2";

const RETRYABLE_ERROR_CODES = new Set([
  "MODEL_UNAVAILABLE",
  "MODEL_TIMEOUT",
  "MODEL_RATE_LIMITED",
  "MODEL_RESPONSE_INVALID",
  "MODEL_RESPONSE_EMPTY",
  "MODEL_JSON_INVALID",
  "MODEL_DECISION_INVALID",
  "MODEL_COMMAND_INVALID",
  "MODEL_USAGE_INVALID",
  "MODEL_PROVIDER_RESPONSE_INVALID",
  "AGENT_REASONER_FAILED",
  "UNKNOWN",
]);

export function isRetryableAgentError(errorCode: string | null | undefined): boolean {
  return typeof errorCode === "string" && RETRYABLE_ERROR_CODES.has(errorCode);
}

export function AgentRunError({ events, lastErrorCode, running, onRetry, retrying = false }: {
  events: AgentEventEnvelope[];
  lastErrorCode: string | null;
  running: boolean;
  onRetry?: () => void;
  retrying?: boolean;
}) {
  const { t } = useTranslation();
  const latest = [...events].reverse().find((item) => ["run_failed", "run_completed", "run_cancelled", "user_message_added"].includes(item.event.type));
  const failure = latest?.event.type === "run_failed" ? latest : undefined;
  const errorCode = lastErrorCode ?? (failure ? String(failure.event.payload?.error_code ?? "UNKNOWN") : null);
  if (!errorCode || (running && !lastErrorCode)) return null;
  const canRetry = !running && Boolean(onRetry) && isRetryableAgentError(errorCode);
  return <div className="agent-error" role="alert">
    <CircleAlert size={14} aria-hidden />
    <span>{t(`contextPanel.error.${errorCode}`, { defaultValue: t("contextPanel.runFailed") })}</span>
    {canRetry && <button type="button" className="agent-error-retry" disabled={retrying} title={t("contextPanel.retryHint")} onClick={onRetry}>{t("contextPanel.retry")}</button>}
  </div>;
}
