import { Play } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "../../components/ui/button";
import { Input } from "../../components/ui/input";
import { cancelAgentRun, listMcp, refreshSkills, runDoctor, runMultiDoctor } from "../../lib/tauri/agentic";
import type { AgentRun, McpServerConfig, MultiServerRun, Skill } from "../../types/agentic";
import type { AgentSessionOption } from "./AgenticWorkspaceView";
import { RunResult } from "./RunResult";

export default function DoctorWorkspace({ sessions, activeSessionId, onRunCompleted }: { sessions: AgentSessionOption[]; activeSessionId: string | null; onRunCompleted: (runId: string) => void }) {
  const { t } = useTranslation();
  const [goal, setGoal] = useState("");
  const [service, setService] = useState("");
  const [url, setUrl] = useState("");
  const [selected, setSelected] = useState<string[]>(activeSessionId ? [activeSessionId] : []);
  const [run, setRun] = useState<AgentRun | null>(null);
  const [multiRun, setMultiRun] = useState<MultiServerRun | null>(null);
  const [runningId, setRunningId] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [failed, setFailed] = useState(false);
  const [skills, setSkills] = useState<Skill[]>([]);
  const [skillId, setSkillId] = useState("");
  const [mcp, setMcp] = useState<McpServerConfig[]>([]);
  const [mcpSelection, setMcpSelection] = useState("");
  const [mcpArguments, setMcpArguments] = useState("{}");
  const enabledSkills = useMemo(() => skills.filter((item) => item.enabled), [skills]);
  const enabledMcpTools = useMemo(() => mcp.flatMap((server) => server.enabled ? server.tools.filter((tool) => tool.enabled && tool.readOnly).map((tool) => ({ serverId: server.id, serverLabel: server.label, ...tool })) : []), [mcp]);

  useEffect(() => {
    void refreshSkills().then(setSkills).catch(() => undefined);
    void listMcp().then(setMcp).catch(() => undefined);
  }, []);

  useEffect(() => {
    if (activeSessionId) {
      setSelected((current) => current.includes(activeSessionId) ? current : [activeSessionId]);
    }
  }, [activeSessionId]);

  const diagnose = async () => {
    if (!goal.trim() || selected.length === 0) return;
    let mcpContext = null;
    if (mcpSelection) {
      try {
        const argumentsValue: unknown = JSON.parse(mcpArguments);
        if (!argumentsValue || Array.isArray(argumentsValue) || typeof argumentsValue !== "object") throw new Error("invalid");
        const selectedTool = enabledMcpTools.find((item) => `${item.serverId}\t${item.name}` === mcpSelection);
        if (!selectedTool) throw new Error("missing");
        mcpContext = { serverId: selectedTool.serverId, toolName: selectedTool.name, arguments: argumentsValue as Record<string, unknown> };
      } catch {
        setFailed(true);
        return;
      }
    }
    setBusy(true); setFailed(false); setRun(null); setMultiRun(null);
    try {
      if (selected.length === 1) {
        const runId = crypto.randomUUID(); setRunningId(runId);
        const result = await runDoctor({ runId, sessionId: selected[0], userRequest: goal.trim(), service: service.trim() || null, httpUrl: url.trim() || null, portHost: null, port: null, includeNginxTest: true, skillId: skillId || null, mcpContext }, () => undefined);
        setRun(result); onRunCompleted(result.id);
      } else {
        const targets = selected.map((sessionId) => ({ sessionId, runId: crypto.randomUUID() }));
        setMultiRun(await runMultiDoctor(crypto.randomUUID(), goal.trim(), targets, service.trim() || null, skillId || null));
      }
    } catch { setFailed(true); } finally { setBusy(false); setRunningId(null); }
  };

  return <div className="grid gap-4">
    {failed && <p className="rounded border border-red-500/30 bg-red-500/5 p-3 text-sm text-red-500">{t("agent.error")}</p>}
    <section className="grid gap-4 rounded-xl border bg-[hsl(var(--surface))] p-4 lg:grid-cols-2">
      <div><h3 className="font-medium">{t("agentic.targets")}</h3><div className="mt-2 grid gap-2">{sessions.map((item) => <label key={item.sessionId} className="flex items-center gap-2 rounded border p-2 text-sm"><input type="checkbox" checked={selected.includes(item.sessionId)} onChange={(event) => setSelected((current) => event.target.checked ? [...new Set([...current, item.sessionId])].slice(0, 10) : current.filter((id) => id !== item.sessionId))} />{item.label}</label>)}</div></div>
      <div className="grid gap-3">
        <label className="grid gap-1 text-sm"><span>{t("agent.goal")}</span><Input value={goal} onChange={(event) => setGoal(event.target.value)} /></label>
        <label className="grid gap-1 text-sm"><span>{t("agentic.serviceOptional")}</span><Input value={service} onChange={(event) => setService(event.target.value)} /></label>
        <label className="grid gap-1 text-sm"><span>{t("agentic.urlOptional")}</span><Input value={url} onChange={(event) => setUrl(event.target.value)} /></label>
        <label className="grid gap-1 text-sm"><span>{t("agentic.skillOptional")}</span><select className="h-9 rounded border bg-transparent px-2" value={skillId} onChange={(event) => setSkillId(event.target.value)}><option value="">{t("agentic.noSkill")}</option>{enabledSkills.map((item) => <option key={item.manifest.id} value={item.manifest.id}>{item.manifest.id}</option>)}</select></label>
        <label className="grid gap-1 text-sm"><span>{t("agentic.mcpToolOptional")}</span><select className="h-9 rounded border bg-transparent px-2" value={mcpSelection} onChange={(event) => setMcpSelection(event.target.value)}><option value="">{t("agentic.noMcpTool")}</option>{enabledMcpTools.map((item) => <option key={`${item.serverId}:${item.name}`} value={`${item.serverId}\t${item.name}`}>{item.serverLabel} · {item.name}</option>)}</select></label>
        {mcpSelection && <label className="grid gap-1 text-sm"><span>{t("agentic.mcpArguments")}</span><textarea className="min-h-20 rounded border bg-transparent p-3 font-mono text-xs" value={mcpArguments} onChange={(event) => setMcpArguments(event.target.value)} /></label>}
        <div className="flex gap-2"><Button disabled={busy || !goal.trim() || selected.length === 0} onClick={() => void diagnose()}><Play size={15} />{selected.length > 1 ? t("agentic.compare") : t("agentic.diagnose")}</Button>{busy && runningId && <Button variant="danger" onClick={() => void cancelAgentRun(runningId).catch(() => undefined)}>{t("agentic.cancel")}</Button>}</div>
      </div>
    </section>
    {run && <RunResult run={run} />}
    {multiRun && <section className="rounded-xl border p-4"><h3 className="font-medium">{t("agentic.drift")}</h3><p className="mt-1 text-sm">{t("agentic.targetCount", { count: multiRun.targets.length })}</p>{multiRun.drift.length === 0 ? <p className="mt-3 text-sm text-[hsl(var(--muted))]">{t("agentic.noDrift")}</p> : <pre className="mt-3 overflow-auto rounded bg-slate-950 p-3 text-xs text-slate-100">{JSON.stringify(multiRun.drift, null, 2)}</pre>}</section>}
  </div>;
}
