import { LoaderCircle } from "lucide-react";
import { useTranslation } from "react-i18next";
import type { AgentStatusState } from "./agent-state";

/**
 * Quiet status line shown while the Agent routes / investigates / diagnoses.
 * Small spinner only — no pulsing or neon animation.
 */
export function AgentStatus({ state, detail }: { state: AgentStatusState; detail?: string }) {
  const { t } = useTranslation();
  const running = state === "routing" || state === "investigating" || state === "diagnosing";
  return <div className="agent-status" role="status" aria-live="polite">
    {running && <LoaderCircle size={13} className="agent-status-spinner" aria-hidden />}
    <span>{detail || t(`contextPanel.status.${state}`)}</span>
  </div>;
}
