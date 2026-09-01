import { Bot } from "lucide-react";
import { useTranslation } from "react-i18next";
import { AgentQuickActions } from "./AgentQuickActions";

/** Empty state before any Agent run: a quiet prompt + quick actions. */
export function AgentEmptyState({ onPrompt }: { onPrompt: (prompt: string) => void }) {
  const { t } = useTranslation();
  return <div className="agent-empty">
    <div className="agent-empty-icon"><Bot size={18} /></div>
    <strong>{t("contextPanel.agentName")}</strong>
    <p>{t("contextPanel.emptyHint")}</p>
    <AgentQuickActions onPrompt={onPrompt} />
  </div>;
}