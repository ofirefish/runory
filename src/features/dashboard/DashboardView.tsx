import { Activity, Cpu, HardDrive, MemoryStick, Network, RefreshCw, Timer } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "../../components/ui/button";
import { Input } from "../../components/ui/input";
import { loadDashboard, loadProcesses, loadServiceHealth } from "../../lib/tauri/infrastructure";
import type { ProcessInfo, ServerDashboard, ServiceHealth } from "../../types/infrastructure";
import { formatFileSize } from "../files/sftp-format";

const percent = (value: number) => `${Math.round(value)}%`;

export function DashboardView({ sessionId, active }: { sessionId: string | null; active: boolean }) {
  const { t, i18n } = useTranslation();
  const [dashboard, setDashboard] = useState<ServerDashboard | null>(null);
  const [processes, setProcesses] = useState<ProcessInfo[]>([]);
  const [services, setServices] = useState<ServiceHealth[]>([]);
  const [serviceNames, setServiceNames] = useState("ssh,nginx,docker");
  const serviceNamesRef = useRef(serviceNames);
  serviceNamesRef.current = serviceNames;
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState(false);
  const refresh = useCallback(async () => {
    if (!sessionId) return;
    setLoading(true); setError(false);
    try {
      const names = serviceNamesRef.current.split(",").map((value) => value.trim()).filter(Boolean);
      const [overview, processList, health] = await Promise.all([loadDashboard(sessionId), loadProcesses(sessionId), names.length ? loadServiceHealth(sessionId, names) : Promise.resolve([])]);
      setDashboard(overview); setProcesses(processList); setServices(health);
    } catch { setError(true); } finally { setLoading(false); }
  }, [sessionId]);
  useEffect(() => { if (active && sessionId) void refresh(); }, [active, sessionId, refresh]);
  useEffect(() => { if (!active || !sessionId) return; const timer = window.setInterval(() => void refresh(), 10_000); return () => window.clearInterval(timer); }, [active, sessionId, refresh]);
  if (!sessionId) return <div className="grid h-full place-items-center text-sm text-[hsl(var(--muted))]">{t("dashboard.connectRequired")}</div>;
  const cards = dashboard ? [
    { icon: Cpu, label: t("dashboard.cpu"), value: percent(dashboard.cpuUsagePercent) },
    { icon: MemoryStick, label: t("dashboard.memory"), value: `${formatFileSize(dashboard.memoryUsedBytes, i18n.language)} / ${formatFileSize(dashboard.memoryTotalBytes, i18n.language)}` },
    { icon: Timer, label: t("dashboard.uptime"), value: t("dashboard.uptimeValue", { days: Math.floor(dashboard.uptimeSeconds / 86400), hours: Math.floor(dashboard.uptimeSeconds % 86400 / 3600) }) },
    { icon: Network, label: t("dashboard.network"), value: `↓ ${formatFileSize(dashboard.networkReceivedBytes, i18n.language)}  ↑ ${formatFileSize(dashboard.networkTransmittedBytes, i18n.language)}` },
  ] : [];
  return <section className="h-full overflow-auto bg-[hsl(var(--background))] p-4" aria-label={t("dashboard.title")}>
    <div className="mb-4 flex items-center"><div><h2 className="text-lg font-semibold">{t("dashboard.title")}</h2><p className="text-xs text-[hsl(var(--muted))]">{t("dashboard.agentless")}</p></div><Button variant="secondary" size="sm" className="ml-auto" disabled={loading} onClick={() => void refresh()}><RefreshCw size={15} className={loading ? "animate-spin" : undefined} />{t("common.refresh")}</Button></div>
    {error && <div className="mb-4 rounded-lg border border-red-500/30 bg-red-500/10 p-3 text-sm text-red-500">{t("dashboard.error")}</div>}
    <div className="grid gap-3 sm:grid-cols-2 xl:grid-cols-4">{cards.map((card) => <article key={card.label} className="rounded-lg border bg-[hsl(var(--surface))] p-4"><div className="flex items-center gap-2 text-xs text-[hsl(var(--secondary))]"><card.icon size={16} />{card.label}</div><p className="mt-3 text-xl font-semibold">{card.value}</p></article>)}</div>
    <div className="mt-4 grid gap-4 xl:grid-cols-2">
      <article className="rounded-lg border bg-[hsl(var(--surface))]"><header className="flex items-center gap-2 border-b px-4 py-3"><HardDrive size={16} /><h3 className="font-medium">{t("dashboard.disk")}</h3></header><div className="divide-y">{dashboard?.disks.map((disk) => <div key={disk.mount} className="px-4 py-3"><div className="flex justify-between text-sm"><span className="truncate font-mono">{disk.mount}</span><span>{percent(disk.usagePercent)}</span></div><div className="mt-2 h-1.5 rounded bg-[hsl(var(--elevated))]"><div className="h-full rounded bg-blue-500" style={{ width: percent(disk.usagePercent) }} /></div><p className="mt-1 text-xs text-[hsl(var(--muted))]">{formatFileSize(disk.usedBytes, i18n.language)} / {formatFileSize(disk.totalBytes, i18n.language)}</p></div>)}</div></article>
      <article className="rounded-lg border bg-[hsl(var(--surface))]"><header className="flex items-center gap-2 border-b px-4 py-3"><Activity size={16} /><h3 className="font-medium">{t("dashboard.services")}</h3></header><div className="p-3"><div className="flex gap-2"><Input value={serviceNames} aria-label={t("dashboard.serviceNames")} onChange={(event) => setServiceNames(event.target.value)} /><Button variant="secondary" onClick={() => void refresh()}>{t("common.refresh")}</Button></div><div className="mt-3 divide-y">{services.map((service) => <div key={service.name} className="flex items-center py-2 text-sm"><span className={`mr-2 h-2 w-2 rounded-full ${service.status === "active" ? "bg-emerald-500" : service.status === "failed" ? "bg-red-500" : "bg-slate-400"}`} /><span>{service.name}</span><span className="ml-auto text-xs text-[hsl(var(--secondary))]">{t(`dashboard.service.${service.status}`)}</span></div>)}</div></div></article>
    </div>
    <article className="mt-4 overflow-hidden rounded-lg border bg-[hsl(var(--surface))]"><header className="border-b px-4 py-3"><h3 className="font-medium">{t("dashboard.processes")}</h3></header><div className="max-h-80 overflow-auto"><table className="w-full text-left text-sm"><thead className="sticky top-0 bg-[hsl(var(--elevated))] text-xs"><tr><th className="px-3 py-2">PID</th><th className="px-3 py-2">{t("dashboard.user")}</th><th className="px-3 py-2">CPU</th><th className="px-3 py-2">MEM</th><th className="px-3 py-2">{t("dashboard.command")}</th></tr></thead><tbody>{processes.map((process) => <tr key={process.pid} className="border-t border-[hsl(var(--border-soft))]"><td className="px-3 py-2 font-mono">{process.pid}</td><td className="px-3 py-2">{process.user}</td><td className="px-3 py-2">{process.cpuPercent}%</td><td className="px-3 py-2">{process.memoryPercent}%</td><td className="px-3 py-2 font-mono">{process.command}</td></tr>)}</tbody></table></div></article>
  </section>;
}
