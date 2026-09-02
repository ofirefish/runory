import { Boxes, Play, RotateCcw, ShieldCheck, Wrench } from "lucide-react";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "../../components/ui/button";
import { DialogShell } from "../../components/ui/dialog-shell";
import { Input } from "../../components/ui/input";
import { approveChangeSet, approveChangeSetStep, approveFleetChangeSet, draftChangeSet, draftFleetChangeSet, executeChangeSet, executeFleetChangeSet, listChangeSets, listFleetChangeSets, rollbackChangeSet } from "../../lib/tauri/agentic";
import type { ChangeSet, ChangeStepDraft, ExecutionStrategy, FailurePolicy, FleetChangeSet, PolicyEvaluation } from "../../types/agentic";
import type { AgentSessionOption } from "./types";

export default function ChangeSetWorkspace({ sessions, activeSessionId, agentRunId }: { sessions: AgentSessionOption[]; activeSessionId: string | null; agentRunId: string | null }) {
  const { t } = useTranslation();
  const [items, setItems] = useState<ChangeSet[]>([]);
  const [changeSet, setChangeSet] = useState<ChangeSet | null>(null);
  const [title, setTitle] = useState("");
  const [changeTool, setChangeTool] = useState<ChangeStepDraft["tool"]>("file.patch");
  const [path, setPath] = useState("");
  const [expected, setExpected] = useState("");
  const [replacement, setReplacement] = useState("");
  const [service, setService] = useState("");
  const [approvalOpen, setApprovalOpen] = useState(false);
  const [fleetApprovalOpen, setFleetApprovalOpen] = useState(false);
  const [fleetItems, setFleetItems] = useState<FleetChangeSet[]>([]);
  const [fleet, setFleet] = useState<FleetChangeSet | null>(null);
  const [fleetTargetIds, setFleetTargetIds] = useState<string[]>([]);
  const [executionStrategy, setExecutionStrategy] = useState<ExecutionStrategy>("sequential");
  const [failurePolicy, setFailurePolicy] = useState<FailurePolicy>("pause-for-review");
  const [production, setProduction] = useState(true);
  const [serviceVerification, setServiceVerification] = useState("");
  const [busy, setBusy] = useState(false);
  const [failed, setFailed] = useState(false);
  const activeSession = activeSessionId ?? sessions[0]?.sessionId ?? null;

  const reload = async (selectedId?: string) => {
    const [next, fleets] = await Promise.all([listChangeSets(), listFleetChangeSets()]);
    setItems(next);
    setFleetItems(fleets);
    if (selectedId) setChangeSet(next.find((item) => item.id === selectedId) ?? null);
  };
  useEffect(() => { void reload().catch(() => setFailed(true)); }, []);

  const makeStep = (): ChangeStepDraft | null => {
    if (changeTool === "file.patch") return path.trim() && expected && expected !== replacement ? { tool: changeTool, path: path.trim(), expected, replacement } : null;
    if (changeTool === "nginx.reload") return { tool: changeTool };
    if (changeTool === "docker.restart") return service.trim() ? { tool: changeTool, container: service.trim() } : null;
    return service.trim() ? { tool: changeTool, service: service.trim() } : null;
  };
  const draft = async () => {
    if (!activeSession || !title.trim()) return;
    const step = makeStep(); if (!step) return;
    setBusy(true); setFailed(false);
    try {
      const result = await draftChangeSet(agentRunId ?? crypto.randomUUID(), activeSession, title.trim(), [step]);
      setChangeSet(result); setExpected(""); setReplacement(""); await reload(result.id);
    } catch { setFailed(true); } finally { setBusy(false); }
  };
  const approve = async () => {
    if (!changeSet) return; setBusy(true); setFailed(false);
    try { const result = await approveChangeSet(changeSet.id, changeSet.version); setChangeSet(result); setApprovalOpen(false); await reload(result.id); } catch { setFailed(true); } finally { setBusy(false); }
  };
  const execute = async () => {
    if (!changeSet) return; setBusy(true); setFailed(false);
    try { const result = await executeChangeSet(changeSet.id, changeSet.version); setChangeSet(result); await reload(result.id); } catch { setFailed(true); } finally { setBusy(false); }
  };
  const approveStep = async (stepId: string) => {
    if (!changeSet) return; setBusy(true); setFailed(false);
    try { const result = await approveChangeSetStep(changeSet.id, changeSet.version, stepId); setChangeSet(result); await reload(result.id); } catch { setFailed(true); } finally { setBusy(false); }
  };
  const rollback = async () => {
    if (!changeSet) return; setBusy(true); setFailed(false);
    try { const result = await rollbackChangeSet(changeSet.id, changeSet.version); setChangeSet(result); await reload(result.id); } catch { setFailed(true); } finally { setBusy(false); }
  };
  const draftFleet = async () => {
    if (!title.trim() || fleetTargetIds.length < 2) return;
    const step = makeStep(); if (!step) return;
    setBusy(true); setFailed(false);
    try {
      const runId = agentRunId ?? crypto.randomUUID();
      const result = await draftFleetChangeSet({
        agentRunId: runId,
        title: title.trim(),
        targets: fleetTargetIds.map((sessionId) => ({ agentRunId: runId, sessionId, title: title.trim(), steps: [step] })),
        executionStrategy,
        failurePolicy,
        batchSize: 2,
        canaryCount: 1,
        production,
        serviceVerification: serviceVerification.trim() || null,
        crossTargetVerification: true,
      });
      setFleet(result); await reload();
    } catch { setFailed(true); } finally { setBusy(false); }
  };
  const approveFleet = async () => {
    if (!fleet) return; setBusy(true); setFailed(false);
    try { const result = await approveFleetChangeSet(fleet.id, fleet.version, fleet.targets.map((target) => target.targetId)); setFleet(result); setFleetApprovalOpen(false); await reload(); } catch { setFailed(true); } finally { setBusy(false); }
  };
  const executeFleet = async () => {
    if (!fleet) return; setBusy(true); setFailed(false);
    try { const result = await executeFleetChangeSet(fleet.id, fleet.version); setFleet(result); await reload(); } catch { setFailed(true); } finally { setBusy(false); }
  };

  return <div className="grid gap-4">
    {failed && <p className="rounded border border-red-500/30 bg-red-500/5 p-3 text-sm text-red-500">{t("agent.error")}</p>}
    {items.length > 0 && <label className="grid gap-1 rounded-xl border p-3 text-sm"><span>{t("agentic.changeHistory")}</span><select className="h-9 rounded border bg-transparent px-2" value={changeSet?.id ?? ""} onChange={(event) => setChangeSet(items.find((item) => item.id === event.target.value) ?? null)}><option value="">{t("agentic.newChangeSet")}</option>{items.map((item) => <option key={item.id} value={item.id}>{item.title} · v{item.version} · {item.executionState}</option>)}</select></label>}
    <section className="grid gap-4 rounded-xl border bg-[hsl(var(--surface))] p-4">
      <div className="flex items-center gap-2"><Wrench size={18} /><h3 className="font-medium">{t("agentic.changeTitle")}</h3></div>
      <p className="text-sm text-[hsl(var(--secondary))]">{t("agentic.changeBoundary")}</p>
      {!changeSet && <div className="grid gap-3">
        <Input value={title} onChange={(event) => setTitle(event.target.value)} placeholder={t("agent.goal")} />
        <select className="h-9 rounded border bg-transparent px-2" value={changeTool} onChange={(event) => setChangeTool(event.target.value as ChangeStepDraft["tool"])}><option value="file.patch">file.patch</option><option value="service.restart">service.restart</option><option value="service.reload">service.reload</option><option value="nginx.reload">nginx.reload</option><option value="docker.restart">docker.restart</option></select>
        {changeTool === "file.patch" && <><Input value={path} onChange={(event) => setPath(event.target.value)} placeholder={t("agent.remotePath")} /><textarea className="min-h-24 rounded border bg-transparent p-3 font-mono text-sm" value={expected} onChange={(event) => setExpected(event.target.value)} placeholder={t("agentic.expected")} /><textarea className="min-h-24 rounded border bg-transparent p-3 font-mono text-sm" value={replacement} onChange={(event) => setReplacement(event.target.value)} placeholder={t("agentic.replacement")} /></>}
        {(changeTool === "service.restart" || changeTool === "service.reload" || changeTool === "docker.restart") && <Input value={service} onChange={(event) => setService(event.target.value)} placeholder={changeTool === "docker.restart" ? t("agent.container") : t("agentic.serviceOptional")} />}
        <Button disabled={busy || !title.trim() || !makeStep()} onClick={() => void draft()}>{t("agentic.createDraft")}</Button>
      </div>}
      {changeSet && <div className="grid gap-3">
        <div className="flex flex-wrap gap-2 text-sm"><span>v{changeSet.version}</span><span>{changeSet.risk}</span><span>{changeSet.approvalState}</span><span>{changeSet.executionState}</span></div>
        {changeSet.recoveryState === "metadata-only" && <p className="rounded border border-amber-500/30 bg-amber-500/5 p-3 text-sm text-amber-700">{t("agentic.recoveredMetadataOnly")}</p>}
        {changeSet.policyEvaluation && <PolicyCheck evaluation={changeSet.policyEvaluation} />}
        {changeSet.steps.map((step) => <div key={step.id} className="rounded border p-3"><p className="font-medium">{step.toolName}</p>{changeSet.recoveryState === "live" && <pre className="mt-2 overflow-auto whitespace-pre-wrap rounded bg-slate-950 p-3 text-xs text-slate-100">{step.preview}</pre>}<p className="mt-2 text-xs">{t("agentic.verify")}: {step.verificationPlanCode} · {t("agentic.rollback")}: {step.rollbackCapability}</p>{changeSet.policyEvaluation?.decision === "REQUIRE_STEP_APPROVAL" && !changeSet.approvedStepIds.includes(step.id) && <Button className="mt-2" size="sm" disabled={busy} onClick={() => void approveStep(step.id)}><ShieldCheck size={14} />{t("agentic.approvePolicyStep")}</Button>}</div>)}
        <div className="flex gap-2">{changeSet.recoveryState === "live" && changeSet.approvalState !== "approved" && changeSet.executionState === "not-started" && changeSet.policyEvaluation?.decision !== "DENY" && changeSet.policyEvaluation?.decision !== "REQUIRE_STEP_APPROVAL" && <Button onClick={() => setApprovalOpen(true)}><ShieldCheck size={15} />{t("agent.approve")}</Button>}{changeSet.recoveryState === "live" && changeSet.approvalState === "approved" && changeSet.executionState === "not-started" && <Button variant="danger" disabled={busy} onClick={() => void execute()}><Play size={15} />{t("agent.execute")}</Button>}{changeSet.recoveryState === "live" && changeSet.steps.some((step) => step.rollbackCapability !== "not-supported") && (changeSet.executionState === "committed" || changeSet.executionState === "failed") && <Button variant="danger" disabled={busy} onClick={() => void rollback()}><RotateCcw size={15} />{t("contextPanel.changeSet.rollbackAction")}</Button>}<Button variant="ghost" onClick={() => setChangeSet(null)}>{t("agent.newPlan")}</Button></div>
      </div>}
    </section>
    <section className="grid gap-4 rounded-xl border bg-[hsl(var(--surface))] p-4">
      <div className="flex items-center gap-2"><Boxes size={18} /><h3 className="font-medium">{t("agentic.fleetTitle")}</h3></div>
      <p className="text-sm text-[hsl(var(--secondary))]">{t("agentic.fleetBoundary")}</p>
      {fleetItems.length > 0 && <select className="h-9 rounded border bg-transparent px-2" value={fleet?.id ?? ""} onChange={(event) => setFleet(fleetItems.find((item) => item.id === event.target.value) ?? null)}><option value="">{t("agentic.newFleet")}</option>{fleetItems.map((item) => <option key={item.id} value={item.id}>{item.title} · v{item.version} · {item.executionState}</option>)}</select>}
      {!fleet && <div className="grid gap-3">
        <div className="grid gap-2 sm:grid-cols-2">{sessions.map((session) => <label key={session.sessionId} className="flex items-center gap-2 rounded border p-2 text-sm"><input type="checkbox" checked={fleetTargetIds.includes(session.sessionId)} onChange={(event) => setFleetTargetIds((current) => event.target.checked ? [...current, session.sessionId] : current.filter((id) => id !== session.sessionId))} />{session.label}</label>)}</div>
        <div className="grid gap-3 sm:grid-cols-2"><label className="grid gap-1 text-sm"><span>{t("agentic.executionStrategy")}</span><select className="h-9 rounded border bg-transparent px-2" value={executionStrategy} onChange={(event) => setExecutionStrategy(event.target.value as ExecutionStrategy)}><option value="sequential">{t("agentic.strategy.sequential")}</option><option value="parallel" disabled={production}>{t("agentic.strategy.parallel")}</option><option value="canary">{t("agentic.strategy.canary")}</option><option value="rolling-batch">{t("agentic.strategy.rolling")}</option></select></label><label className="grid gap-1 text-sm"><span>{t("agentic.failurePolicy")}</span><select className="h-9 rounded border bg-transparent px-2" value={failurePolicy} onChange={(event) => setFailurePolicy(event.target.value as FailurePolicy)}><option value="stop">{t("agentic.failure.stop")}</option><option value="pause-for-review">{t("agentic.failure.pause")}</option><option value="continue">{t("agentic.failure.continue")}</option><option value="rollback">{t("agentic.failure.rollback")}</option></select></label></div>
        <label className="flex items-center gap-2 text-sm"><input type="checkbox" checked={production} onChange={(event) => { setProduction(event.target.checked); if (event.target.checked && executionStrategy === "parallel") setExecutionStrategy("sequential"); }} />{t("agentic.productionTargets")}</label>
        <Input value={serviceVerification} onChange={(event) => setServiceVerification(event.target.value)} placeholder={t("agentic.serviceVerificationOptional")} />
        <Button disabled={busy || fleetTargetIds.length < 2 || !title.trim() || !makeStep()} onClick={() => void draftFleet()}>{t("agentic.createFleetDraft")}</Button>
      </div>}
      {fleet && <div className="grid gap-3"><div className="flex flex-wrap gap-2 text-sm"><span>v{fleet.version}</span><span>{fleet.risk}</span><span>{fleet.executionStrategy}</span><span>{fleet.failurePolicy}</span><span>{fleet.executionState}</span></div>{fleet.recoveryState === "metadata-only" && <p className="rounded border border-amber-500/30 bg-amber-500/5 p-3 text-sm text-amber-700">{t("agentic.recoveredFleetMetadataOnly")}</p>}{fleet.policyEvaluation && <PolicyCheck evaluation={fleet.policyEvaluation} />}<div className="grid gap-2">{fleet.targets.map((target) => <div key={target.targetId} className="grid gap-1 rounded border p-3 text-sm"><span className="font-mono text-xs">{target.targetId}</span><span>{target.state} · {t("agentic.verify")}: {target.localVerification} / {target.serviceVerification} · {t("agentic.rollback")}: {target.rollbackState}</span>{target.errorCode && <span className="text-red-500">{target.errorCode}</span>}</div>)}</div><p className="text-xs text-[hsl(var(--secondary))]">{t("agentic.fleetVerification", { cross: fleet.verification.crossTarget, service: fleet.verification.serviceLevel, calls: fleet.toolCallCount })}</p><div className="flex gap-2">{fleet.recoveryState === "live" && fleet.approvalState !== "approved" && fleet.executionState === "draft" && <Button disabled={fleet.policyEvaluation?.decision === "DENY"} onClick={() => setFleetApprovalOpen(true)}><ShieldCheck size={15} />{t("agent.approve")}</Button>}{fleet.recoveryState === "live" && fleet.approvalState === "approved" && (fleet.executionState === "approved" || fleet.executionState === "paused-for-review") && <Button variant="danger" disabled={busy} onClick={() => void executeFleet()}><Play size={15} />{fleet.executionState === "paused-for-review" ? t("agentic.resumeAfterReview") : t("agent.execute")}</Button>}<Button variant="ghost" onClick={() => setFleet(null)}>{t("agentic.newFleet")}</Button></div></div>}
    </section>
    {approvalOpen && changeSet && <DialogShell title={t("agent.approvalTitle")} onClose={() => setApprovalOpen(false)}><p className="text-sm">{t("agentic.approvalExact", { id: changeSet.id, version: changeSet.version })}</p><div className="mt-5 flex justify-end gap-2"><Button variant="ghost" onClick={() => setApprovalOpen(false)}>{t("common.cancel")}</Button><Button variant="danger" disabled={busy} onClick={() => void approve()}>{t("agent.approve")}</Button></div></DialogShell>}
    {fleetApprovalOpen && fleet && <DialogShell title={t("agent.approvalTitle")} onClose={() => setFleetApprovalOpen(false)}><p className="text-sm">{t("agentic.fleetApprovalExact", { id: fleet.id, version: fleet.version, count: fleet.targets.length })}</p><ul className="mt-3 grid gap-1 font-mono text-xs">{fleet.targets.map((target) => <li key={target.targetId}>{target.targetId} · {target.changeSetId} v{target.changeSetVersion}</li>)}</ul><div className="mt-5 flex justify-end gap-2"><Button variant="ghost" onClick={() => setFleetApprovalOpen(false)}>{t("common.cancel")}</Button><Button variant="danger" disabled={busy} onClick={() => void approveFleet()}>{t("agent.approve")}</Button></div></DialogShell>}
  </div>;
}

function PolicyCheck({ evaluation }: { evaluation: PolicyEvaluation }) {
  const { t } = useTranslation();
  const denied = evaluation.decision === "DENY";
  return <section className={`grid gap-2 rounded border p-3 text-sm ${denied ? "border-red-500/30 bg-red-500/5" : "border-emerald-500/30 bg-emerald-500/5"}`}>
    <div className="flex flex-wrap items-center justify-between gap-2"><strong>{t("agentic.policyCheck")}</strong><span>{t(`agentic.policyDecision.${evaluation.decision}`)}</span></div>
    <p>{t(`agentic.policyReason.${evaluation.reason}`)}</p>
    <p className="text-xs text-[hsl(var(--secondary))]">{t("agentic.policyVersion", { version: evaluation.policyVersion, hash: evaluation.policyHash.slice(0, 12) })}</p>
    <p className="text-xs text-[hsl(var(--secondary))]">{t("agentic.policyMatchedRules", { rules: evaluation.matchedRules.map((rule) => rule.ruleId).join(", ") })}</p>
  </section>;
}
