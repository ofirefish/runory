import { Plus, Trash2 } from "lucide-react";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { Button } from "../../components/ui/button";
import { DialogShell } from "../../components/ui/dialog-shell";
import { Input } from "../../components/ui/input";
import { Label } from "../../components/ui/label";
import { SelectControl } from "../../components/ui/select-control";
import type { DockerDaemonConfig, DockerEngineSettingsView } from "../../types/infrastructure";
import { formatFileSize } from "../files/sftp-format";

type LogDriver = "" | "json-file" | "local" | "journald" | "syslog" | "none";

const emptyConfig = (): DockerDaemonConfig => ({
  registryMirrors: [],
  insecureRegistries: [],
  logDriver: "",
  logOpts: {},
  liveRestore: false,
});

function showsLogOpts(driver: string): boolean {
  return driver === "json-file" || driver === "local";
}

export function DockerSettingsPanel({
  settings,
  busy = false,
  onReload,
  onApply,
}: {
  settings: DockerEngineSettingsView | null;
  busy?: boolean;
  onReload: () => Promise<void>;
  onApply: (config: DockerDaemonConfig) => Promise<void>;
}) {
  const { t, i18n } = useTranslation();
  const [draft, setDraft] = useState<DockerDaemonConfig>(emptyConfig());
  const [confirmOpen, setConfirmOpen] = useState(false);
  const [actionBusy, setActionBusy] = useState(false);
  const [actionFailed, setActionFailed] = useState(false);

  useEffect(() => {
    if (settings) {
      setDraft({
        registryMirrors: [...settings.config.registryMirrors],
        insecureRegistries: [...settings.config.insecureRegistries],
        logDriver: settings.config.logDriver,
        logOpts: {
          maxSize: settings.config.logOpts.maxSize ?? "",
          maxFile: settings.config.logOpts.maxFile ?? "",
        },
        liveRestore: settings.config.liveRestore,
      });
    }
  }, [settings]);

  const logDriverOptions: { value: LogDriver; label: string }[] = [
    { value: "", label: t("operations.dockerSettings.logDriverDefault") },
    { value: "json-file", label: "json-file" },
    { value: "local", label: "local" },
    { value: "journald", label: "journald" },
    { value: "syslog", label: "syslog" },
    { value: "none", label: "none" },
  ];

  const updateList = (key: "registryMirrors" | "insecureRegistries", index: number, value: string) => {
    setDraft((previous) => {
      const next = [...previous[key]];
      next[index] = value;
      return { ...previous, [key]: next };
    });
  };

  const addListItem = (key: "registryMirrors" | "insecureRegistries") => {
    setDraft((previous) => ({ ...previous, [key]: [...previous[key], ""] }));
  };

  const removeListItem = (key: "registryMirrors" | "insecureRegistries", index: number) => {
    setDraft((previous) => ({
      ...previous,
      [key]: previous[key].filter((_, itemIndex) => itemIndex !== index),
    }));
  };

  const submit = async () => {
    setActionBusy(true);
    setActionFailed(false);
    try {
      const config: DockerDaemonConfig = {
        registryMirrors: draft.registryMirrors.map((item) => item.trim()).filter(Boolean),
        insecureRegistries: draft.insecureRegistries.map((item) => item.trim()).filter(Boolean),
        logDriver: draft.logDriver.trim(),
        logOpts: showsLogOpts(draft.logDriver)
          ? {
              maxSize: draft.logOpts.maxSize?.trim() || null,
              maxFile: draft.logOpts.maxFile?.trim() || null,
            }
          : {},
        liveRestore: draft.liveRestore,
      };
      await onApply(config);
      setConfirmOpen(false);
      toast.success(t("operations.dockerSettings.applySuccess"));
      await onReload();
    } catch {
      setActionFailed(true);
    } finally {
      setActionBusy(false);
    }
  };

  if (!settings && busy) {
    return <div className="grid h-40 place-items-center text-sm text-[hsl(var(--muted))]">{t("common.loading")}</div>;
  }

  if (!settings) {
    return (
      <div className="rounded-lg border border-red-500/30 bg-red-500/10 p-3 text-sm text-red-500">
        {t("operations.dockerSettings.loadFailed")}
      </div>
    );
  }

  const info = settings.info;
  const infoRows: { label: string; value: string }[] = [
    { label: t("operations.dockerSettings.serverVersion"), value: info.serverVersion || "—" },
    { label: t("operations.dockerSettings.storageDriver"), value: info.storageDriver || "—" },
    { label: t("operations.dockerSettings.loggingDriver"), value: info.loggingDriver || "—" },
    { label: t("operations.dockerSettings.osArch"), value: [info.operatingSystem, info.architecture].filter(Boolean).join(" / ") || "—" },
    { label: t("operations.dockerSettings.ncpu"), value: String(info.ncpu || "—") },
    { label: t("operations.dockerSettings.memory"), value: info.memTotalBytes ? formatFileSize(info.memTotalBytes, i18n.language) : "—" },
    { label: t("operations.dockerSettings.rootDir"), value: info.dockerRootDir || "—" },
    {
      label: t("operations.dockerSettings.liveRestoreEffective"),
      value: info.liveRestoreEnabled ? t("operations.dockerSettings.yes") : t("operations.dockerSettings.no"),
    },
  ];

  return (
    <div className="mx-auto flex w-full max-w-3xl flex-col gap-6">
      <div>
        <h3 className="text-sm font-medium">{t("operations.dockerSettings.engineInfo")}</h3>
        <p className="mt-1 text-xs text-[hsl(var(--muted))]">{t("operations.dockerSettings.engineInfoHint")}</p>
        <dl className="mt-3 grid gap-2 rounded-lg border p-3 text-sm sm:grid-cols-2">
          {infoRows.map((row) => (
            <div key={row.label} className="min-w-0">
              <dt className="text-xs text-[hsl(var(--muted))]">{row.label}</dt>
              <dd className="mt-0.5 truncate font-mono text-xs" title={row.value}>{row.value}</dd>
            </div>
          ))}
        </dl>
      </div>

      <div className="space-y-4">
        <div>
          <h3 className="text-sm font-medium">{t("operations.dockerSettings.daemonConfig")}</h3>
          <p className="mt-1 text-xs text-[hsl(var(--muted))]">
            {t("operations.dockerSettings.daemonConfigHint", { path: settings.configPath })}
            {!settings.configExists ? ` ${t("operations.dockerSettings.configMissing")}` : null}
            {settings.configRawPreserved ? ` ${t("operations.dockerSettings.otherKeysPreserved")}` : null}
          </p>
        </div>

        <section className="space-y-2">
          <div className="flex items-center justify-between gap-2">
            <Label>{t("operations.dockerSettings.registryMirrors")}</Label>
            <Button type="button" size="sm" variant="ghost" disabled={busy || actionBusy} onClick={() => addListItem("registryMirrors")}>
              <Plus size={14} />
              {t("operations.dockerSettings.addItem")}
            </Button>
          </div>
          {draft.registryMirrors.length === 0 ? (
            <p className="text-xs text-[hsl(var(--muted))]">{t("operations.dockerSettings.emptyList")}</p>
          ) : (
            <div className="space-y-2">
              {draft.registryMirrors.map((item, index) => (
                <div key={`mirror-${index}`} className="flex gap-2">
                  <Input
                    value={item}
                    placeholder={t("operations.dockerSettings.mirrorPlaceholder")}
                    disabled={busy || actionBusy}
                    onChange={(event) => updateList("registryMirrors", index, event.target.value)}
                  />
                  <Button type="button" size="icon" variant="ghost" aria-label={t("operations.dockerSettings.removeItem")} disabled={busy || actionBusy} onClick={() => removeListItem("registryMirrors", index)}>
                    <Trash2 size={14} />
                  </Button>
                </div>
              ))}
            </div>
          )}
        </section>

        <section className="space-y-2">
          <div className="flex items-center justify-between gap-2">
            <Label>{t("operations.dockerSettings.insecureRegistries")}</Label>
            <Button type="button" size="sm" variant="ghost" disabled={busy || actionBusy} onClick={() => addListItem("insecureRegistries")}>
              <Plus size={14} />
              {t("operations.dockerSettings.addItem")}
            </Button>
          </div>
          {draft.insecureRegistries.length === 0 ? (
            <p className="text-xs text-[hsl(var(--muted))]">{t("operations.dockerSettings.emptyList")}</p>
          ) : (
            <div className="space-y-2">
              {draft.insecureRegistries.map((item, index) => (
                <div key={`insecure-${index}`} className="flex gap-2">
                  <Input
                    value={item}
                    placeholder={t("operations.dockerSettings.insecurePlaceholder")}
                    disabled={busy || actionBusy}
                    onChange={(event) => updateList("insecureRegistries", index, event.target.value)}
                  />
                  <Button type="button" size="icon" variant="ghost" aria-label={t("operations.dockerSettings.removeItem")} disabled={busy || actionBusy} onClick={() => removeListItem("insecureRegistries", index)}>
                    <Trash2 size={14} />
                  </Button>
                </div>
              ))}
            </div>
          )}
        </section>

        <section className="grid gap-3 sm:grid-cols-2">
          <div className="space-y-1.5">
            <Label htmlFor="docker-log-driver">{t("operations.dockerSettings.logDriver")}</Label>
            <SelectControl
              id="docker-log-driver"
              label={t("operations.dockerSettings.logDriver")}
              value={(draft.logDriver || "") as LogDriver}
              disabled={busy || actionBusy}
              options={logDriverOptions}
              onValueChange={(value) => setDraft((previous) => ({ ...previous, logDriver: value }))}
            />
          </div>
          <label className="flex items-center gap-2 self-end pb-2 text-sm">
            <input
              type="checkbox"
              className="size-4"
              checked={draft.liveRestore}
              disabled={busy || actionBusy}
              onChange={(event) => setDraft((previous) => ({ ...previous, liveRestore: event.target.checked }))}
            />
            <span>{t("operations.dockerSettings.liveRestore")}</span>
          </label>
        </section>

        {showsLogOpts(draft.logDriver) && (
          <section className="grid gap-3 sm:grid-cols-2">
            <div className="space-y-1.5">
              <Label htmlFor="docker-log-max-size">{t("operations.dockerSettings.logMaxSize")}</Label>
              <Input
                id="docker-log-max-size"
                value={draft.logOpts.maxSize ?? ""}
                placeholder={t("operations.dockerSettings.logMaxSizePlaceholder")}
                disabled={busy || actionBusy}
                onChange={(event) => setDraft((previous) => ({ ...previous, logOpts: { ...previous.logOpts, maxSize: event.target.value } }))}
              />
            </div>
            <div className="space-y-1.5">
              <Label htmlFor="docker-log-max-file">{t("operations.dockerSettings.logMaxFile")}</Label>
              <Input
                id="docker-log-max-file"
                value={draft.logOpts.maxFile ?? ""}
                placeholder={t("operations.dockerSettings.logMaxFilePlaceholder")}
                disabled={busy || actionBusy}
                onChange={(event) => setDraft((previous) => ({ ...previous, logOpts: { ...previous.logOpts, maxFile: event.target.value } }))}
              />
            </div>
          </section>
        )}

        <div className="flex justify-end gap-2 pt-2">
          <Button type="button" variant="secondary" disabled={busy || actionBusy} onClick={() => void onReload()}>
            {t("common.refresh")}
          </Button>
          <Button type="button" disabled={busy || actionBusy} onClick={() => { setActionFailed(false); setConfirmOpen(true); }}>
            {t("operations.dockerSettings.saveAndRestart")}
          </Button>
        </div>
      </div>

      {confirmOpen && (
        <DialogShell title={t("operations.confirmTitle")} onClose={() => !actionBusy && setConfirmOpen(false)}>
          <p className="text-sm text-[hsl(var(--secondary))]">{t("operations.dockerSettings.confirmRestart")}</p>
          {actionFailed && <p className="mt-3 text-sm text-red-500">{t("operations.dockerSettings.applyFailed")}</p>}
          <div className="mt-5 flex justify-end gap-2">
            <Button variant="ghost" disabled={actionBusy} onClick={() => setConfirmOpen(false)}>{t("common.cancel")}</Button>
            <Button variant="danger" disabled={actionBusy} onClick={() => void submit()}>
              {t("operations.dockerSettings.saveAndRestart")}
            </Button>
          </div>
        </DialogShell>
      )}
    </div>
  );
}
