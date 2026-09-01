import { Disc3, Globe, ServerCog } from "lucide-react";
import { useTranslation } from "react-i18next";

export type QuickActionId = "website" | "nginx" | "docker" | "disk" | "service";

const QUICK_PROMPTS: Record<QuickActionId, string> = {
  website: "contextPanel.quickPrompt.website",
  nginx: "contextPanel.quickPrompt.nginx",
  docker: "contextPanel.quickPrompt.docker",
  disk: "contextPanel.quickPrompt.disk",
  service: "contextPanel.quickPrompt.service",
};

/**
 * Idle-state quick prompts. Clicking submits the corresponding natural
 * language intent to the Composer — it never opens a parameter form.
 */
export function AgentQuickActions({ onPrompt, disabled }: { onPrompt: (prompt: string) => void; disabled?: boolean }) {
  const { t } = useTranslation();
  const actions: { id: QuickActionId; icon: typeof Globe; label: string }[] = [
    { id: "website", icon: Globe, label: t("contextPanel.quick.website") },
    { id: "nginx", icon: ServerCog, label: t("contextPanel.quick.nginx") },
    { id: "docker", icon: ServerCog, label: t("contextPanel.quick.docker") },
    { id: "disk", icon: Disc3, label: t("contextPanel.quick.disk") },
    { id: "service", icon: ServerCog, label: t("contextPanel.quick.service") },
  ];
  return <div className="agent-quick-actions" aria-label={t("contextPanel.quickActions")}>
    {actions.map((action) => <button key={action.id} type="button" disabled={disabled} onClick={() => onPrompt(t(QUICK_PROMPTS[action.id]))}>
      <action.icon size={13} /><span>{action.label}</span>
    </button>)}
  </div>;
}