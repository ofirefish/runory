import { useTranslation } from "react-i18next";
import type { AgentRun } from "../../../types/agentic";

const LABEL_KEYS: Record<string, string> = {
  "agent.exec": "contextPanel.activity.agentExec",
  "nginx.test": "contextPanel.activity.nginxTest",
  "nginx.reload": "contextPanel.activity.nginxReload",
  "http.check": "contextPanel.activity.httpCheck",
  "dns.check": "contextPanel.activity.dnsCheck",
  "service.status": "contextPanel.activity.serviceStatus",
  "docker.list": "contextPanel.activity.dockerList",
  "disk.usage": "contextPanel.activity.diskUsage",
  "log.read": "contextPanel.activity.logRead",
  "process.list": "contextPanel.activity.processList",
};

/**
 * Tool-call timeline: ✓ / ✕ / ○ rows with optional expansion to
 * Tool / Result / Duration. Raw JSON stays collapsed behind the row —
 * the panel never renders full tool payloads by default.
 */
export function ActivityTimeline({ run, expanded, onToggle }: {
  run: AgentRun;
  expanded: Record<string, boolean>;
  onToggle: (invocationId: string) => void;
}) {
  const { t } = useTranslation();
  return <div className="activity-timeline" aria-label={t("contextPanel.activity")}>
    <div className="activity-timeline-header"><span>{t("contextPanel.activity")}</span><span>{run.activities.length}</span></div>
    {run.activities.map((item) => {
      const isExpanded = Boolean(expanded[item.invocationId]);
      const label = LABEL_KEYS[item.toolName] ? t(LABEL_KEYS[item.toolName]) : item.toolName;
      return <div key={item.invocationId} className={`activity-item${isExpanded ? " expanded" : ""}`}>
        <button type="button" className="activity-item-row" onClick={() => onToggle(item.invocationId)} aria-expanded={isExpanded}>
          <span className={`activity-mark ${item.success ? "success" : "error"}`} aria-hidden>{item.success ? "✓" : "✕"}</span>
          <span className="activity-label">{label}</span>
          <span className="activity-chevron" aria-hidden>{isExpanded ? "▾" : "▸"}</span>
        </button>
        {isExpanded && <div className="activity-detail"><dl>
          <div><dt>{t("contextPanel.activity.tool")}</dt><dd className="font-mono">{item.toolName}</dd></div>
          <div><dt>{t("contextPanel.activity.result")}</dt><dd>{item.success ? t("contextPanel.activity.ok") : (item.errorCode ?? "—")}</dd></div>
          <div><dt>{t("contextPanel.activity.duration")}</dt><dd>{item.durationMs} ms</dd></div>
        </dl></div>}
      </div>;
    })}
  </div>;
}
