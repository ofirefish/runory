import { Box, FileText, Play, RefreshCw, RotateCcw, ServerCog, Square, Workflow } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "../../components/ui/button";
import { DialogShell } from "../../components/ui/dialog-shell";
import { Input } from "../../components/ui/input";
import { actOnDocker, actOnNginx, actOnPm2, listDocker, listPm2, readLogs } from "../../lib/tauri/infrastructure";
import type { DockerContainer, LogSource, Pm2Process, ResourceAction } from "../../types/infrastructure";
import { formatFileSize } from "../files/sftp-format";
import { LogSourceSelect } from "./LogSourceSelect";
import { cn } from "../../lib/utils";

type PendingAction = { kind: "docker" | "pm2"; target: string; action: ResourceAction } | { kind: "nginx"; target: "nginx"; action: "test" | "reload" };
type OperationsSection = PendingAction["kind"] | "logs";

function ActionDialog({ action, onConfirm, onClose }: { action: PendingAction; onConfirm: () => Promise<void>; onClose: () => void }) {
  const { t } = useTranslation(); const [busy, setBusy] = useState(false); const [failed, setFailed] = useState(false);
  const confirm = async () => { setBusy(true); setFailed(false); try { await onConfirm(); onClose(); } catch { setFailed(true); setBusy(false); } };
  return <DialogShell title={t("operations.confirmTitle")} onClose={onClose}><p className="text-sm text-[hsl(var(--secondary))]">{t("operations.confirmDescription", { action: t(`operations.action.${action.action}`), target: action.target })}</p>{failed && <p className="mt-3 text-sm text-red-500">{t("operations.error")}</p>}<div className="mt-5 flex justify-end gap-2"><Button variant="ghost" onClick={onClose}>{t("common.cancel")}</Button><Button variant={action.action === "stop" ? "danger" : "default"} disabled={busy} onClick={() => void confirm()}>{t(`operations.action.${action.action}`)}</Button></div></DialogShell>;
}

export function OperationsView({ sessionId, active }: { sessionId: string | null; active: boolean }) {
  const { t, i18n } = useTranslation();
  const [section, setSection] = useState<OperationsSection>("docker");
  const [containers, setContainers] = useState<DockerContainer[]>([]); const [processes, setProcesses] = useState<Pm2Process[]>([]);
  const [pending, setPending] = useState<PendingAction | null>(null); const [loading, setLoading] = useState(false);
  const [errors, setErrors] = useState<Partial<Record<OperationsSection, boolean>>>({});
  const [outputs, setOutputs] = useState<Partial<Record<OperationsSection, string>>>({});
  const error = errors[section] ?? false;
  const output = outputs[section] ?? "";
  const [logSource, setLogSource] = useState<LogSource>("system"); const [logTarget, setLogTarget] = useState(""); const [logLines, setLogLines] = useState(200);
  const refresh = useCallback(async () => { if (!sessionId) return; setLoading(true); setErrors((previous) => ({ ...previous, [section]: false })); try { if (section === "docker") setContainers(await listDocker(sessionId)); else if (section === "pm2") setProcesses(await listPm2(sessionId)); } catch { setErrors((previous) => ({ ...previous, [section]: true })); } finally { setLoading(false); } }, [section, sessionId]);
  useEffect(() => { if (active) void refresh(); }, [active, refresh]);
  if (!sessionId) return <div className="grid h-full place-items-center text-sm text-[hsl(var(--muted))]">{t("operations.connectRequired")}</div>;
  const run = async (action: PendingAction) => { const result = action.kind === "docker" ? await actOnDocker(sessionId, action.target, action.action as ResourceAction) : action.kind === "pm2" ? await actOnPm2(sessionId, action.target, action.action as ResourceAction) : await actOnNginx(sessionId, action.action as "test" | "reload"); setOutputs((previous) => ({ ...previous, [action.kind]: result.output })); setErrors((previous) => ({ ...previous, [action.kind]: !result.success })); await refresh(); };
  const actionButtons = (kind: "docker" | "pm2", target: string) => <div className="flex gap-1">{(["start", "stop", "restart"] as const).map((action) => <Button key={action} variant="ghost" size="icon" className="h-7 w-7" title={t(`operations.action.${action}`)} aria-label={t("operations.resourceAction", { action: t(`operations.action.${action}`), target })} onClick={() => setPending({ kind, target, action })}>{action === "start" ? <Play size={13} /> : action === "stop" ? <Square size={13} /> : <RotateCcw size={13} />}</Button>)}</div>;
  const sections = [{ id: "docker" as const, icon: Box }, { id: "pm2" as const, icon: Workflow }, { id: "nginx" as const, icon: ServerCog }, { id: "logs" as const, icon: FileText }];
  return <section className="flex h-full min-h-0 flex-col bg-[hsl(var(--surface))]" aria-label={t("operations.title")}><nav className="flex h-11 shrink-0 items-center gap-1 border-b px-3">{sections.map((item) => <Button key={item.id} variant={section === item.id ? "secondary" : "ghost"} size="sm" onClick={() => setSection(item.id)}><item.icon size={15} />{t(`operations.${item.id}`)}</Button>)}<Button variant="ghost" size="icon" className="ml-auto" disabled={loading} aria-label={t("common.refresh")} onClick={() => void refresh()}><RefreshCw size={16} className={loading ? "animate-spin" : undefined} /></Button></nav>
    {error && <div className="m-4 rounded-lg border border-red-500/30 bg-red-500/10 p-3 text-sm text-red-500">{t("operations.unsupported")}</div>}
    <div className={cn("min-h-0 flex-1 p-4", section === "logs" ? "flex flex-col overflow-hidden" : "overflow-auto")}>
      {section === "docker" && <div className="overflow-hidden rounded-lg border"><table className="w-full text-left text-sm"><thead className="bg-[hsl(var(--elevated))] text-xs"><tr><th className="px-3 py-2">{t("operations.name")}</th><th className="px-3 py-2">{t("operations.image")}</th><th className="px-3 py-2">{t("operations.status")}</th><th className="w-28 px-3 py-2">{t("operations.actions")}</th></tr></thead><tbody>{containers.map((item) => <tr key={item.id} className="border-t"><td className="px-3 py-2">{item.name}</td><td className="px-3 py-2 font-mono text-xs">{item.image}</td><td className="px-3 py-2">{item.status}</td><td className="px-3 py-2">{actionButtons("docker", item.id)}</td></tr>)}</tbody></table></div>}
      {section === "pm2" && <div className="overflow-hidden rounded-lg border"><table className="w-full text-left text-sm"><thead className="bg-[hsl(var(--elevated))] text-xs"><tr><th className="px-3 py-2">{t("operations.name")}</th><th className="px-3 py-2">{t("operations.status")}</th><th className="px-3 py-2">CPU</th><th className="px-3 py-2">MEM</th><th className="w-28 px-3 py-2">{t("operations.actions")}</th></tr></thead><tbody>{processes.map((item) => <tr key={item.id} className="border-t"><td className="px-3 py-2">{item.name}</td><td className="px-3 py-2">{item.status}</td><td className="px-3 py-2">{item.cpuPercent}%</td><td className="px-3 py-2">{formatFileSize(item.memoryBytes, i18n.language)}</td><td className="px-3 py-2">{actionButtons("pm2", String(item.id))}</td></tr>)}</tbody></table></div>}
      {section === "nginx" && <div className="rounded-lg border p-4"><h3 className="font-medium">{t("operations.nginxTools")}</h3><p className="mt-1 text-sm text-[hsl(var(--secondary))]">{t("operations.nginxHint")}</p><div className="mt-4 flex gap-2"><Button variant="secondary" onClick={() => setPending({ kind: "nginx", target: "nginx", action: "test" })}>{t("operations.action.test")}</Button><Button onClick={() => setPending({ kind: "nginx", target: "nginx", action: "reload" })}>{t("operations.action.reload")}</Button></div></div>}
      {section === "logs" && <div className="shrink-0 space-y-3"><div className="grid gap-2 rounded-lg border p-3 md:grid-cols-[180px_1fr_120px_auto]"><LogSourceSelect value={logSource} onValueChange={setLogSource} /><Input value={logTarget} disabled={!(["docker","pm2","service"] as LogSource[]).includes(logSource)} placeholder={t("operations.logTarget")} onChange={(event) => setLogTarget(event.target.value)} /><Input type="number" min={20} max={5000} value={logLines} onChange={(event) => setLogLines(Number(event.target.value))} /><Button onClick={async () => { setErrors((previous) => ({ ...previous, logs: false })); try { const result = await readLogs(sessionId, logSource, logTarget || null, logLines); setOutputs((previous) => ({ ...previous, logs: result.output })); } catch { setErrors((previous) => ({ ...previous, logs: true })); } }}>{t("operations.readLogs")}</Button></div></div>}
      {(section === "logs" || output) && <pre className={cn("mt-4 overflow-auto whitespace-pre-wrap rounded-lg bg-slate-950 p-4 font-mono text-xs text-slate-100", section === "logs" ? "min-h-0 flex-1" : "max-h-96")}>{output}</pre>}
    </div>{pending && <ActionDialog action={pending} onClose={() => setPending(null)} onConfirm={() => run(pending)} />}</section>;
}
