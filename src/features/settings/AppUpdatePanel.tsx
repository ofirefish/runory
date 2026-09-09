import { Download, RefreshCw, RotateCw } from "lucide-react";
import { useEffect } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "../../components/ui/button";
import { useAppUpdateStore } from "../../stores/app-update-store";

export function AppUpdatePanel() {
  const { t } = useTranslation();
  const { phase, configured, currentVersion, update, downloadedBytes, totalBytes, errorCode, initialize, check, download, install } = useAppUpdateStore();
  useEffect(() => { void initialize(); }, [initialize]);
  if (!currentVersion) return null;
  const progress = totalBytes && totalBytes > 0 ? Math.min(100, Math.round((downloadedBytes / totalBytes) * 100)) : null;
  const busy = phase === "checking" || phase === "downloading" || phase === "installing";
  return <section className="settings-card">
    <h4>{t("settings.update.title")}</h4>
    <p>{t("settings.update.currentVersion", { version: currentVersion ?? "—" })}</p>
    {!configured && <p className="mt-2 text-xs text-[hsl(var(--muted))]">{t("settings.update.notConfigured")}</p>}
    {configured && phase === "up-to-date" && <p className="mt-2 text-xs text-[hsl(var(--muted))]">{t("settings.update.upToDate")}</p>}
    {update && <div className="mt-3 rounded-md border p-3">
      <p className="text-sm font-medium">{t("settings.update.available", { version: update.version })}</p>
      {update.notes && <p className="mt-2 whitespace-pre-wrap text-xs text-[hsl(var(--muted))]">{update.notes}</p>}
    </div>}
    {phase === "downloading" && <div className="mt-3" role="status" aria-label={t("settings.update.downloading")}>
      <div className="h-1.5 overflow-hidden rounded-full bg-[hsl(var(--elevated))]"><div className="h-full bg-[hsl(var(--primary))] transition-[width]" style={{ width: `${progress ?? 12}%` }} /></div>
      <p className="mt-1 text-xs text-[hsl(var(--muted))]">{progress === null ? t("settings.update.downloading") : t("settings.update.progress", { progress })}</p>
    </div>}
    {phase === "busy" && <p className="mt-3 text-xs text-amber-600" role="alert">{t("settings.update.activeSessions")}</p>}
    {phase === "error" && <p className="mt-3 text-xs text-red-500" role="alert">{t(`errors.${errorCode ?? "UNKNOWN"}`, { defaultValue: t("errors.UNKNOWN") })}</p>}
    <div className="mt-3 flex flex-wrap gap-2">
      {configured && !update && <Button size="sm" variant="secondary" disabled={busy} onClick={() => void check(false)}><RefreshCw size={14} />{t(phase === "checking" ? "settings.update.checking" : "settings.update.check")}</Button>}
      {configured && update && phase === "available" && <Button size="sm" onClick={() => void download()}><Download size={14} />{t("settings.update.download")}</Button>}
      {update && phase === "ready" && <Button size="sm" onClick={() => void install(false)}><RotateCw size={14} />{t("settings.update.install")}</Button>}
      {update && phase === "busy" && <Button size="sm" variant="danger" onClick={() => void install(true)}><RotateCw size={14} />{t("settings.update.installAnyway")}</Button>}
      {configured && phase === "error" && <Button size="sm" variant="secondary" onClick={() => void check(false)}><RefreshCw size={14} />{t("settings.update.retry")}</Button>}
    </div>
  </section>;
}
