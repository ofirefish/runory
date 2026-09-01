import { Bot } from "lucide-react";
import { lazy, Suspense, useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "../../components/ui/button";

const DoctorWorkspace = lazy(() => import("./DoctorWorkspace"));
const ChangeSetWorkspace = lazy(() => import("./ChangeSetWorkspace"));
const IntegrationsWorkspace = lazy(() => import("./IntegrationsWorkspace"));
const LegacyWorkspace = lazy(() => import("./LegacyWorkspace"));
const IncidentWorkspace = lazy(() => import("./IncidentWorkspace"));

export type AgentSessionOption = { sessionId: string; profileId: string; label: string };
type View = "incidents" | "doctor" | "changes" | "integrations" | "legacy";

export function AgenticWorkspaceView({ sessions, activeSessionId }: { sessions: AgentSessionOption[]; activeSessionId: string | null }) {
  const { t } = useTranslation();
  const [view, setView] = useState<View>("incidents");
  const [lastAgentRunId, setLastAgentRunId] = useState<string | null>(null);

  if (sessions.length === 0) {
    return <div className="grid h-full place-items-center p-6 text-sm text-[hsl(var(--muted))]">{t("agent.connectRequired")}</div>;
  }

  return <div className="h-full overflow-y-auto p-4 md:p-6"><div className="mx-auto grid max-w-6xl gap-4">
    <header className="flex flex-wrap items-center justify-between gap-3">
      <div className="flex items-center gap-2"><Bot size={20} /><div><h2 className="font-semibold">{t("agentic.title")}</h2><p className="text-sm text-[hsl(var(--secondary))]">{t("agentic.description")}</p></div></div>
      <nav className="flex flex-wrap gap-1">{(["incidents", "doctor", "changes", "integrations", "legacy"] as View[]).map((item) => <Button key={item} size="sm" variant={view === item ? "default" : "ghost"} onClick={() => setView(item)}>{t(`agentic.tab.${item}`)}</Button>)}</nav>
    </header>
    <Suspense fallback={<p className="p-4 text-sm text-[hsl(var(--muted))]">{t("common.loading")}</p>}>
      {view === "incidents" && <IncidentWorkspace sessions={sessions} activeSessionId={activeSessionId} onOpenChanges={() => setView("changes")} />}
      {view === "doctor" && <DoctorWorkspace sessions={sessions} activeSessionId={activeSessionId} onRunCompleted={setLastAgentRunId} />}
      {view === "changes" && <ChangeSetWorkspace sessions={sessions} activeSessionId={activeSessionId} agentRunId={lastAgentRunId} />}
      {view === "integrations" && <IntegrationsWorkspace />}
      {view === "legacy" && <LegacyWorkspace sessions={sessions} activeSessionId={activeSessionId} />}
    </Suspense>
  </div></div>;
}
