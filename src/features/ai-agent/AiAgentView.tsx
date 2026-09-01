import { Bot, CheckCircle2, ClipboardCheck, History, Play, Plus, ShieldAlert, Trash2 } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "../../components/ui/button";
import { DialogShell } from "../../components/ui/dialog-shell";
import { Input } from "../../components/ui/input";
import { approveAgentStep, createAgentPlan, discardAgentPlan, executeAgentStep, getAgentPlan, listAgentAudit } from "../../lib/tauri/ai-agent";
import type { AiAgentPlan, AiAuditRecord, AiPlanStep, AiTerminalPreset, AiToolInput, AiToolName, AiToolOutput } from "../../types/ai-agent";
import { describeToolTarget, toolNeedsContainer, toolNeedsContent, toolNeedsPath } from "./agent-tools";

export type AgentSessionOption = { sessionId: string; profileId: string; label: string };
const toolNames: AiToolName[] = ["system-metrics", "process-list", "docker-list", "docker-restart", "nginx-test", "nginx-reload", "terminal-exec", "file-read", "file-write"];
const presets: AiTerminalPreset[] = ["disk-usage", "memory-usage", "listening-ports", "recent-errors"];

function makeTool(name: AiToolName, path: string, content: string, container: string, preset: AiTerminalPreset): AiToolInput | null {
  if (name === "file-read") return path.trim() ? { tool: name, path: path.trim() } : null;
  if (name === "file-write") return path.trim() ? { tool: name, path: path.trim(), content } : null;
  if (name === "docker-restart") return container.trim() ? { tool: name, container: container.trim() } : null;
  if (name === "terminal-exec") return { tool: name, preset };
  return { tool: name };
}

function OutputView({ output }: { output: AiToolOutput }) {
  const { t } = useTranslation();
  if (output.kind === "file-written") return <p className="text-sm">{t("agent.fileWritten", { path: output.path, bytes: output.bytes })}</p>;
  if (output.kind === "metrics") return <pre className="overflow-x-auto whitespace-pre-wrap rounded bg-slate-950 p-3 text-xs text-slate-100">{JSON.stringify(output, null, 2)}</pre>;
  if (output.kind === "processes" || output.kind === "containers") return <pre className="max-h-72 overflow-auto whitespace-pre-wrap rounded bg-slate-950 p-3 text-xs text-slate-100">{JSON.stringify(output, null, 2)}</pre>;
  const value = output.kind === "file" ? output.content : output.value;
  return <pre className="max-h-72 overflow-auto whitespace-pre-wrap rounded bg-slate-950 p-3 text-xs text-slate-100">{value || t("agent.emptyOutput")}</pre>;
}

export function AiAgentView({ sessions, activeSessionId }: { sessions: AgentSessionOption[]; activeSessionId: string | null }) {
  const { t } = useTranslation();
  const [selectedSessions, setSelectedSessions] = useState<string[]>(activeSessionId ? [activeSessionId] : []);
  const [goal, setGoal] = useState("");
  const [toolName, setToolName] = useState<AiToolName>("system-metrics");
  const [path, setPath] = useState("");
  const [content, setContent] = useState("");
  const [container, setContainer] = useState("");
  const [preset, setPreset] = useState<AiTerminalPreset>("disk-usage");
  const [tools, setTools] = useState<AiToolInput[]>([]);
  const [plan, setPlan] = useState<AiAgentPlan | null>(null);
  const [pending, setPending] = useState<AiPlanStep | null>(null);
  const [outputs, setOutputs] = useState<Record<string, AiToolOutput>>({});
  const [audit, setAudit] = useState<AiAuditRecord[] | null>(null);
  const [busy, setBusy] = useState(false);
  const [failed, setFailed] = useState(false);
  const sessionLabels = useMemo(() => new Map(sessions.map((session) => [session.sessionId, session.label])), [sessions]);
  const draft = makeTool(toolName, path, content, container, preset);

  useEffect(() => {
    const planId = plan?.id;
    return () => { if (planId) void discardAgentPlan(planId).catch(() => undefined); };
  }, [plan?.id]);

  const addTool = () => {
    if (!draft) return;
    setTools((current) => [...current, draft]);
    setContent("");
  };
  const create = async () => {
    if (!goal.trim() || selectedSessions.length === 0 || tools.length === 0) return;
    setBusy(true); setFailed(false);
    try {
      setPlan(await createAgentPlan(selectedSessions, goal, tools));
      setTools([]); setContent(""); setOutputs({}); setAudit(null);
    } catch { setFailed(true); } finally { setBusy(false); }
  };
  const approve = async () => {
    if (!plan || !pending) return;
    setBusy(true); setFailed(false);
    try { setPlan(await approveAgentStep(plan.id, pending.id)); setPending(null); } catch { setFailed(true); } finally { setBusy(false); }
  };
  const execute = async (step: AiPlanStep) => {
    if (!plan) return;
    setBusy(true); setFailed(false);
    try {
      const result = await executeAgentStep(plan.id, step.id);
      setPlan(result.plan); setOutputs((current) => ({ ...current, [step.id]: result.output }));
    } catch {
      setFailed(true);
      try { setPlan(await getAgentPlan(plan.id)); } catch { /* Preserve the last reviewable plan. */ }
    } finally { setBusy(false); }
  };
  const loadAudit = async () => {
    setBusy(true); setFailed(false);
    try { setAudit(await listAgentAudit()); } catch { setFailed(true); } finally { setBusy(false); }
  };

  if (sessions.length === 0) return <div className="grid h-full place-items-center p-6 text-sm text-[hsl(var(--muted))]">{t("agent.connectRequired")}</div>;

  return <div className="h-full overflow-y-auto bg-[hsl(var(--background))] p-4 md:p-6"><div className="mx-auto grid max-w-6xl gap-5">
    <header className="flex items-start justify-between gap-3"><div className="flex items-start gap-3"><div className="rounded-lg bg-violet-500/10 p-2 text-violet-500"><Bot size={20} /></div><div><h2 className="font-semibold">{t("agent.title")}</h2><p className="mt-1 text-sm text-[hsl(var(--secondary))]">{t("agent.description")}</p></div></div><Button variant="secondary" size="sm" disabled={busy} onClick={() => void loadAudit()}><History size={15} />{t("agent.audit")}</Button></header>
    {!plan && <div className="grid gap-4 lg:grid-cols-[1fr_1.25fr]">
      <section className="rounded-xl border bg-[hsl(var(--surface))] p-4"><h3 className="font-medium">{t("agent.targets")}</h3><div className="mt-3 grid gap-2">{sessions.map((session) => <label key={session.sessionId} className="flex items-center gap-2 rounded border p-2 text-sm"><input type="checkbox" checked={selectedSessions.includes(session.sessionId)} onChange={(event) => setSelectedSessions((current) => event.target.checked ? [...current, session.sessionId] : current.filter((id) => id !== session.sessionId))} />{session.label}</label>)}</div><label className="mt-4 grid gap-1.5 text-sm"><span>{t("agent.goal")}</span><Input value={goal} onChange={(event) => setGoal(event.target.value)} placeholder={t("agent.goalPlaceholder")} /></label></section>
      <section className="rounded-xl border bg-[hsl(var(--surface))] p-4"><h3 className="font-medium">{t("agent.typedTools")}</h3><div className="mt-3 grid gap-3"><label className="grid gap-1.5 text-sm"><span>{t("agent.tool")}</span><select className="h-9 rounded-md border bg-transparent px-3" value={toolName} onChange={(event) => setToolName(event.target.value as AiToolName)}>{toolNames.map((name) => <option key={name} value={name}>{t(`agent.tool.${name}`)}</option>)}</select></label>
        {toolName === "terminal-exec" && <label className="grid gap-1.5 text-sm"><span>{t("agent.preset")}</span><select className="h-9 rounded-md border bg-transparent px-3" value={preset} onChange={(event) => setPreset(event.target.value as AiTerminalPreset)}>{presets.map((value) => <option key={value} value={value}>{t(`agent.preset.${value}`)}</option>)}</select></label>}
        {toolNeedsPath(toolName) && <label className="grid gap-1.5 text-sm"><span>{t("agent.remotePath")}</span><Input value={path} onChange={(event) => setPath(event.target.value)} /></label>}
        {toolNeedsContent(toolName) && <label className="grid gap-1.5 text-sm"><span>{t("agent.fileContent")}</span><textarea className="min-h-28 rounded-md border bg-transparent p-3 font-mono text-sm" value={content} onChange={(event) => setContent(event.target.value)} /></label>}
        {toolNeedsContainer(toolName) && <label className="grid gap-1.5 text-sm"><span>{t("agent.container")}</span><Input value={container} onChange={(event) => setContainer(event.target.value)} /></label>}
        <Button variant="secondary" disabled={!draft || tools.length >= 12} onClick={addTool}><Plus size={15} />{t("agent.addTool")}</Button>
        <div className="grid gap-2">{tools.map((tool, index) => <div key={`${tool.tool}-${index}`} className="flex items-center gap-2 rounded border p-2 text-sm"><span className="flex-1">{t(`agent.tool.${tool.tool}`)}{tool.tool === "file-write" ? ` · ${tool.path} · ${new TextEncoder().encode(tool.content).length} B` : describeToolTarget(tool) ? ` · ${describeToolTarget(tool)}` : ""}</span><Button variant="ghost" size="icon" aria-label={t("agent.removeTool")} onClick={() => setTools((current) => current.filter((_, item) => item !== index))}><Trash2 size={15} /></Button></div>)}</div>
      </div></section>
      <div className="lg:col-span-2">{failed && <p className="mb-3 text-sm text-red-500">{t("agent.error")}</p>}<Button disabled={busy || !goal.trim() || selectedSessions.length === 0 || tools.length === 0} onClick={() => void create()}><ClipboardCheck size={16} />{t("agent.createPlan")}</Button></div>
    </div>}
    {plan && <section className="rounded-xl border bg-[hsl(var(--surface))] p-4"><div className="flex items-start justify-between gap-3"><div><h3 className="font-medium">{t("agent.plan")}</h3><p className="mt-1 text-sm text-[hsl(var(--secondary))]">{plan.goal}</p></div><Button variant="ghost" size="sm" onClick={() => { setPlan(null); setOutputs({}); }}>{t("agent.newPlan")}</Button></div><div className="mt-4 grid gap-3">{plan.steps.map((step, index) => <div key={step.id} className="rounded-lg border p-3"><div className="flex flex-wrap items-center gap-2 text-sm"><span className="font-medium">{index + 1}. {t(`agent.tool.${step.tool.tool}`)}</span><span className="text-[hsl(var(--muted))]">{sessionLabels.get(step.sessionId) ?? step.sessionId}</span><span className={`rounded-full px-2 py-0.5 text-xs ${step.risk === "critical" ? "bg-red-500/10 text-red-600" : step.risk === "high" ? "bg-orange-500/10 text-orange-600" : "bg-emerald-500/10 text-emerald-600"}`}>{t(`ai.risk.${step.risk}`)}</span><span className="rounded-full bg-[hsl(var(--elevated))] px-2 py-0.5 text-xs">{t(`agent.status.${step.status}`)}</span></div>{describeToolTarget(step.tool) && <p className="mt-2 font-mono text-xs">{describeToolTarget(step.tool)}{step.tool.tool === "file-write" ? ` · ${t("agent.fileBytes", { bytes: step.tool.bytes })}` : ""}</p>}<div className="mt-3 flex gap-2">{step.status === "pending-approval" && <Button size="sm" variant={step.risk === "critical" ? "danger" : "default"} onClick={() => setPending(step)}><ShieldAlert size={14} />{t("agent.reviewApprove")}</Button>}{step.status === "approved" && <Button size="sm" disabled={busy} onClick={() => void execute(step)}><Play size={14} />{t("agent.execute")}</Button>}{step.status === "succeeded" && <span className="flex items-center gap-1 text-xs text-emerald-600"><CheckCircle2 size={14} />{t("agent.completed")}</span>}</div>{outputs[step.id] && <div className="mt-3"><OutputView output={outputs[step.id]} /></div>}</div>)}</div>{failed && <p className="mt-3 text-sm text-red-500">{t("agent.error")}</p>}</section>}
    {audit && <section className="rounded-xl border bg-[hsl(var(--surface))] p-4"><h3 className="font-medium">{t("agent.audit")}</h3>{audit.length === 0 ? <p className="mt-3 text-sm text-[hsl(var(--muted))]">{t("agent.auditEmpty")}</p> : <div className="mt-3 grid gap-2">{audit.slice(0, 100).map((record) => <div key={record.id} className="grid gap-1 rounded border p-2 text-xs md:grid-cols-[10rem_1fr_8rem_8rem]"><span>{new Date(record.startedAtEpochSeconds * 1000).toLocaleString()}</span><span>{t(`agent.tool.${record.tool}`)}{record.target ? ` · ${record.target}` : ""}</span><span>{t(`ai.risk.${record.risk}`)}</span><span className={record.succeeded ? "text-emerald-600" : "text-red-500"}>{record.succeeded ? t("agent.auditSucceeded") : record.errorCode}</span></div>)}</div>}</section>}
  </div>{pending && <DialogShell title={t("agent.approvalTitle")} onClose={() => setPending(null)}><p className="text-sm text-[hsl(var(--secondary))]">{t("agent.approvalDescription")}</p><div className="mt-3 rounded border p-3"><p className="font-medium">{t(`agent.tool.${pending.tool.tool}`)}</p>{describeToolTarget(pending.tool) && <p className="mt-1 font-mono text-xs">{describeToolTarget(pending.tool)}{pending.tool.tool === "file-write" ? ` · ${t("agent.fileBytes", { bytes: pending.tool.bytes })}` : ""}</p>}<p className="mt-2 text-sm">{t("agent.riskLabel")}: {t(`ai.risk.${pending.risk}`)}</p></div><div className="mt-5 flex justify-end gap-2"><Button variant="ghost" onClick={() => setPending(null)}>{t("common.cancel")}</Button><Button variant={pending.risk === "critical" ? "danger" : "default"} disabled={busy} onClick={() => void approve()}>{t("agent.approve")}</Button></div></DialogShell>}</div>;
}
