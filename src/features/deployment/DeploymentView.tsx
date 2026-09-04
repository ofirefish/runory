import { Archive, CalendarClock, GitBranch, History, KeyRound, LockKeyhole, Play, RefreshCw } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "../../components/ui/button";
import { DialogShell } from "../../components/ui/dialog-shell";
import { DeploymentInputField as Field, DeploymentSelectField as SelectField, DeploymentTextareaField as TextareaField } from "./DeploymentFields";
import { addCron, createBackup, inspectSsl, issueSsl, listCron, listDeploymentHistory, removeCron, runDeployment, setupGit, writeEnvironment } from "../../lib/tauri/infrastructure";
import type { BuildPreset, CronEntry, CronSchedule, CronTask, DeploymentRecord, RestartTarget } from "../../types/infrastructure";

type Section = "git" | "deploy" | "environment" | "ssl" | "backup" | "cron" | "history";
type Pending = { description: string; run: () => Promise<void> };

function ConfirmOperation({ pending, onClose }: { pending: Pending; onClose: () => void }) {
  const { t } = useTranslation(); const [busy, setBusy] = useState(false); const [failed, setFailed] = useState(false);
  const confirm = async () => { setBusy(true); setFailed(false); try { await pending.run(); onClose(); } catch { setFailed(true); setBusy(false); } };
  return <DialogShell title={t("deployment.confirmTitle")} onClose={onClose}><p className="text-sm text-[hsl(var(--secondary))]">{pending.description}</p><p className="mt-3 text-xs text-amber-600">{t("deployment.confirmHint")}</p>{failed && <p className="mt-3 text-sm text-red-500">{t("deployment.error")}</p>}<div className="mt-5 flex justify-end gap-2"><Button variant="ghost" onClick={onClose}>{t("common.cancel")}</Button><Button disabled={busy} onClick={() => void confirm()}>{t("deployment.confirm")}</Button></div></DialogShell>;
}

export function DeploymentView({ sessionId, profileId }: { sessionId: string | null; profileId: string }) {
  const { t, i18n } = useTranslation(); const [section, setSection] = useState<Section>("deploy"); const [pending, setPending] = useState<Pending | null>(null); const [output, setOutput] = useState(""); const [error, setError] = useState(false);
  const [repositoryPath, setRepositoryPath] = useState("/srv/app"); const [remoteUrl, setRemoteUrl] = useState(""); const [branch, setBranch] = useState("main"); const [build, setBuild] = useState<BuildPreset>("none"); const [restartKind, setRestartKind] = useState<"none" | "systemd" | "pm2" | "dockerCompose">("none"); const [restartName, setRestartName] = useState("");
  const [environmentPath, setEnvironmentPath] = useState("/srv/app/.env"); const [environmentText, setEnvironmentText] = useState("");
  const [domain, setDomain] = useState(""); const [email, setEmail] = useState(""); const [webroot, setWebroot] = useState("/var/www/html");
  const [backupSource, setBackupSource] = useState("/srv/app"); const [backupDestination, setBackupDestination] = useState("/srv/backups");
  const [cronEntries, setCronEntries] = useState<CronEntry[]>([]); const [cronSchedule, setCronSchedule] = useState<CronSchedule>("daily"); const [cronKind, setCronKind] = useState<"backup" | "serviceRestart" | "gitPull">("backup"); const [cronFirst, setCronFirst] = useState("/srv/app"); const [cronSecond, setCronSecond] = useState("/srv/backups");
  const [historyItems, setHistoryItems] = useState<DeploymentRecord[]>([]);
  const refreshLists = useCallback(async () => { if (!sessionId) return; const [cron, history] = await Promise.allSettled([listCron(sessionId), listDeploymentHistory(profileId)]); if (cron.status === "fulfilled") setCronEntries(cron.value); if (history.status === "fulfilled") setHistoryItems(history.value); }, [profileId, sessionId]);
  useEffect(() => { void refreshLists(); }, [refreshLists]);
  if (!sessionId) return <div className="grid h-full place-items-center text-sm text-[hsl(var(--muted))]">{t("deployment.connectRequired")}</div>;
  const showResult = (result: { output: string; success?: boolean }) => { setOutput(result.output || t("deployment.success")); setError(result.success === false); void refreshLists(); };
  const run = (description: string, operation: () => Promise<{ output: string }>) => setPending({ description, run: async () => { try { showResult(await operation()); } catch (cause) { setError(true); throw cause; } } });
  const restart: RestartTarget = restartKind === "none" ? { kind: "none" } : restartKind === "systemd" ? { kind: "systemd", service: restartName } : restartKind === "pm2" ? { kind: "pm2", process: restartName } : { kind: "dockerCompose", service: restartName };
  const environmentEntries = () => environmentText.split(/\r?\n/).filter(Boolean).map((line) => { const index = line.indexOf("="); return { key: index < 0 ? line : line.slice(0, index), value: index < 0 ? "" : line.slice(index + 1) }; });
  const cronTask = (): CronTask => cronKind === "backup" ? { kind: "backup", sourcePath: cronFirst, destinationDirectory: cronSecond } : cronKind === "serviceRestart" ? { kind: "serviceRestart", service: cronFirst } : { kind: "gitPull", repositoryPath: cronFirst, branch: cronSecond };
  const nav: { id: Section; icon: typeof GitBranch }[] = [{ id: "git", icon: GitBranch }, { id: "deploy", icon: Play }, { id: "environment", icon: KeyRound }, { id: "ssl", icon: LockKeyhole }, { id: "backup", icon: Archive }, { id: "cron", icon: CalendarClock }, { id: "history", icon: History }];
  return <section className="flex h-full min-h-0 bg-[hsl(var(--surface))]" aria-label={t("deployment.title")}><nav className="w-44 shrink-0 space-y-1 border-r p-2">{nav.map((item) => <Button key={item.id} variant={section === item.id ? "secondary" : "ghost"} className="w-full justify-start" onClick={() => setSection(item.id)}><item.icon size={15} />{t(`deployment.${item.id}`)}</Button>)}</nav><div className="min-w-0 flex-1 overflow-auto p-5">
    <div className="mb-5 flex items-center"><div><h2 className="text-lg font-semibold">{t(`deployment.${section}`)}</h2><p className="text-xs text-[hsl(var(--muted))]">{t(`deployment.${section}Hint`)}</p></div>{(section === "cron" || section === "history") && <Button variant="ghost" size="icon" className="ml-auto" aria-label={t("common.refresh")} onClick={() => void refreshLists()}><RefreshCw size={16} /></Button>}</div>
    {error && <div className="mb-4 rounded-lg border border-red-500/30 bg-red-500/10 p-3 text-sm text-red-500">{t("deployment.error")}</div>}
    {section === "git" && <div className="grid max-w-2xl gap-4"><Field label={t("deployment.repositoryPath")} value={repositoryPath} onChange={setRepositoryPath} /><Field label={t("deployment.remoteUrl")} value={remoteUrl} onChange={setRemoteUrl} /><Field label={t("deployment.branch")} value={branch} onChange={setBranch} /><Button className="w-fit" onClick={() => run(t("deployment.gitConfirm"), () => setupGit(sessionId, repositoryPath, remoteUrl, branch))}>{t("deployment.setupGit")}</Button></div>}
    {section === "deploy" && <div className="grid max-w-2xl gap-4">
      <Field label={t("deployment.repositoryPath")} value={repositoryPath} onChange={setRepositoryPath} />
      <Field label={t("deployment.branch")} value={branch} onChange={setBranch} />
      <SelectField label={t("deployment.buildPreset")} value={build} onChange={setBuild} options={(["none", "npm", "pnpm", "cargo"] as const).map((value) => ({ value, label: t(`deployment.build.${value}`) }))} />
      <SelectField label={t("deployment.restartTarget")} value={restartKind} onChange={setRestartKind} options={(["none", "systemd", "pm2", "dockerCompose"] as const).map((value) => ({ value, label: t(`deployment.restart.${value}`) }))} />
      {restartKind !== "none" && <Field label={t("deployment.restartName")} value={restartName} onChange={setRestartName} />}
      <Button className="w-fit" onClick={() => run(t("deployment.deployConfirm"), () => runDeployment(sessionId, repositoryPath, branch, build, restart))}>{t("deployment.run")}</Button>
    </div>}
    {section === "environment" && <div className="grid max-w-2xl gap-4">
      <Field label={t("deployment.environmentPath")} value={environmentPath} onChange={setEnvironmentPath} />
      <TextareaField label={t("deployment.environmentEntries")} value={environmentText} onChange={setEnvironmentText} description={t("deployment.environmentSecurity")} />
      <Button className="w-fit" onClick={() => run(t("deployment.environmentConfirm"), async () => { const result = await writeEnvironment(sessionId, environmentPath, environmentEntries()); if (result.success) setEnvironmentText(""); return result; })}>{t("deployment.writeEnvironment")}</Button>
    </div>}
    {section === "ssl" && <div className="grid max-w-2xl gap-4"><Field label={t("deployment.domain")} value={domain} onChange={setDomain} /><div className="flex gap-2"><Button variant="secondary" onClick={async () => { try { showResult(await inspectSsl(sessionId, domain)); } catch { setError(true); } }}>{t("deployment.inspectSsl")}</Button></div><Field label={t("deployment.email")} value={email} onChange={setEmail} type="email" /><Field label={t("deployment.webroot")} value={webroot} onChange={setWebroot} /><Button className="w-fit" onClick={() => run(t("deployment.sslConfirm"), () => issueSsl(sessionId, domain, email, webroot))}>{t("deployment.issueSsl")}</Button></div>}
    {section === "backup" && <div className="grid max-w-2xl gap-4"><Field label={t("deployment.sourcePath")} value={backupSource} onChange={setBackupSource} /><Field label={t("deployment.destinationDirectory")} value={backupDestination} onChange={setBackupDestination} /><Button className="w-fit" onClick={() => run(t("deployment.backupConfirm"), () => createBackup(sessionId, backupSource, backupDestination))}>{t("deployment.createBackup")}</Button></div>}
    {section === "cron" && <div className="grid gap-5 xl:grid-cols-2"><div className="grid content-start gap-4 rounded-lg border p-4">
      <SelectField label={t("deployment.schedule")} value={cronSchedule} onChange={setCronSchedule} options={(["hourly", "daily", "weekly"] as const).map((value) => ({ value, label: t(`deployment.schedule.${value}`) }))} />
      <SelectField label={t("deployment.cronTask")} value={cronKind} onChange={setCronKind} options={(["backup", "serviceRestart", "gitPull"] as const).map((value) => ({ value, label: t(`deployment.cron.${value}`) }))} />
      <Field label={t(cronKind === "serviceRestart" ? "deployment.serviceName" : cronKind === "backup" ? "deployment.sourcePath" : "deployment.repositoryPath")} value={cronFirst} onChange={setCronFirst} />
      {cronKind !== "serviceRestart" && <Field label={t(cronKind === "backup" ? "deployment.destinationDirectory" : "deployment.branch")} value={cronSecond} onChange={setCronSecond} />}
      <Button className="w-fit" onClick={() => run(t("deployment.cronConfirm"), async () => { await addCron(sessionId, cronSchedule, cronTask()); return { output: t("deployment.success") }; })}>{t("deployment.addCron")}</Button>
    </div><div className="overflow-hidden rounded-lg border"><div className="divide-y">{cronEntries.map((entry) => <div key={entry.id} className="flex items-center gap-3 p-3 text-sm"><span>{t(`deployment.schedule.${entry.schedule}`)}</span><span className="text-[hsl(var(--secondary))]">{t(`deployment.cron.${entry.taskKind}`)}</span><Button variant="ghost" size="sm" className="ml-auto text-red-500" onClick={() => run(t("deployment.cronRemoveConfirm"), async () => { const result = await removeCron(sessionId, entry.id); return result; })}>{t("common.delete")}</Button></div>)}</div></div></div>}
    {section === "history" && <div className="overflow-hidden rounded-lg border"><table className="w-full text-left text-sm"><thead className="bg-[hsl(var(--elevated))] text-xs"><tr><th className="px-3 py-2">{t("deployment.time")}</th><th className="px-3 py-2">{t("deployment.operation")}</th><th className="px-3 py-2">{t("deployment.target")}</th><th className="px-3 py-2">{t("deployment.result")}</th></tr></thead><tbody>{historyItems.map((record) => <tr key={record.id} className="border-t"><td className="px-3 py-2">{new Intl.DateTimeFormat(i18n.language, { dateStyle: "medium", timeStyle: "short" }).format(record.startedAtEpochSeconds * 1000)}</td><td className="px-3 py-2">{t(`deployment.operation.${record.operation}`)}</td><td className="px-3 py-2 font-mono text-xs">{record.target}</td><td className={`px-3 py-2 ${record.success ? "text-emerald-500" : "text-red-500"}`}>{t(record.success ? "deployment.succeeded" : "deployment.failed")}</td></tr>)}</tbody></table></div>}
    {output && <pre className="mt-5 max-h-80 overflow-auto whitespace-pre-wrap rounded-lg bg-slate-950 p-4 font-mono text-xs text-slate-100">{output}</pre>}
  </div>{pending && <ConfirmOperation pending={pending} onClose={() => setPending(null)} />}</section>;
}
