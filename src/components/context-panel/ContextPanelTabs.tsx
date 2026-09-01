import { Bot, Info } from "lucide-react";
import { useTranslation } from "react-i18next";

export type ContextTab = "inspector" | "agent";

/**
 * Inspector / Agent segmented tabs for the right Context Panel.
 * Styling matches Runory Desktop v3: quiet text-secondary inactive,
 * accent-soft + accent text active, subtle border. No large pills.
 */
export function ContextPanelTabs({ active, onChange, running }: {
  active: ContextTab;
  onChange: (tab: ContextTab) => void;
  /** True while an Agent run is active for the current context. */
  running?: boolean;
}) {
  const { t } = useTranslation();
  const items: { id: ContextTab; icon: typeof Info; label: string; dot?: boolean }[] = [
    { id: "agent", icon: Bot, label: t("agent.title"), dot: running },
    { id: "inspector", icon: Info, label: t("inspector.title") },
  ];
  const onKeyDown = (event: React.KeyboardEvent<HTMLDivElement>) => {
    if (event.key !== "ArrowLeft" && event.key !== "ArrowRight") return;
    const current = items.findIndex((item) => item.id === active);
    if (current < 0) return;
    event.preventDefault();
    const next = (current + (event.key === "ArrowRight" ? 1 : items.length - 1)) % items.length;
    onChange(items[next].id);
  };
  return <div className="context-panel-tabs" role="tablist" aria-label={t("contextPanel.tablist")} onKeyDown={onKeyDown}>
    {items.map((item) => <button key={item.id} type="button" role="tab" id={`context-tab-${item.id}`} aria-selected={active === item.id} aria-controls="context-panel-content" tabIndex={active === item.id ? 0 : -1} className={active === item.id ? "active" : ""} onClick={() => onChange(item.id)} title={item.label}>
      <item.icon size={14} />
      <span>{item.label}</span>
      {item.dot && <span className="context-panel-dot" aria-label={t("contextPanel.agentRunning")} />}
    </button>)}
  </div>;
}
