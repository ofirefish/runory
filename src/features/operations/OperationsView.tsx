import { Box, FileText, Play, RefreshCw, RotateCcw, ServerCog, Square, Workflow } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { Button } from "../../components/ui/button";
import { DialogShell } from "../../components/ui/dialog-shell";
import { Input } from "../../components/ui/input";
import { actOnDocker, actOnDockerImage, actOnDockerNetwork, actOnDockerVolume, actOnNginx, actOnPm2, applyDockerSettings, deleteDockerRegistries, getDockerSettings, listDocker, listDockerImages, listDockerNetworks, listDockerRegistries, listDockerVolumes, listPm2, readLogs, upsertDockerRegistry } from "../../lib/tauri/infrastructure";
import type { DockerContainer, DockerDaemonConfig, DockerEngineSettingsView, DockerImage, DockerImageAction, DockerNetwork, DockerNetworkAction, DockerRegistry, DockerRegistryUpsertInput, DockerVolume, DockerVolumeAction, LogSource, Pm2Process, ResourceAction } from "../../types/infrastructure";
import { formatFileSize } from "../files/sftp-format";
import { DockerPanel, type DockerTab } from "./DockerPanel";
import { LogSourceSelect } from "./LogSourceSelect";
import { cn } from "../../lib/utils";

type PendingAction =
  | { kind: "docker" | "pm2"; target: string; label: string; action: ResourceAction }
  | { kind: "nginx"; target: "nginx"; action: "test" | "reload" };

type OperationsSection = PendingAction["kind"] | "logs";

function actionLabel(action: PendingAction): string {
  return action.kind === "nginx" ? action.target : action.label;
}

function ActionDialog({ action, onConfirm, onClose }: { action: PendingAction; onConfirm: () => Promise<void>; onClose: () => void }) {
  const { t } = useTranslation(); const [busy, setBusy] = useState(false); const [failed, setFailed] = useState(false);
  const confirm = async () => { setBusy(true); setFailed(false); try { await onConfirm(); onClose(); } catch { setFailed(true); setBusy(false); } };
  return <DialogShell title={t("operations.confirmTitle")} onClose={onClose}><p className="text-sm text-[hsl(var(--secondary))]">{t("operations.confirmDescription", { action: t(`operations.action.${action.action}`), target: actionLabel(action) })}</p>{failed && <p className="mt-3 text-sm text-red-500">{t("operations.error")}</p>}<div className="mt-5 flex justify-end gap-2"><Button variant="ghost" onClick={onClose}>{t("common.cancel")}</Button><Button variant={action.action === "stop" ? "danger" : "default"} disabled={busy} onClick={() => void confirm()}>{t(`operations.action.${action.action}`)}</Button></div></DialogShell>;
}

export function OperationsView({ sessionId, active }: { sessionId: string | null; active: boolean }) {
  const { t, i18n } = useTranslation();
  const [section, setSection] = useState<OperationsSection>("docker");
  const [containers, setContainers] = useState<DockerContainer[]>([]);
  const [images, setImages] = useState<DockerImage[]>([]);
  const [imagesLoaded, setImagesLoaded] = useState(false);
  const [networks, setNetworks] = useState<DockerNetwork[]>([]);
  const [networksLoaded, setNetworksLoaded] = useState(false);
  const [volumes, setVolumes] = useState<DockerVolume[]>([]);
  const [volumesLoaded, setVolumesLoaded] = useState(false);
  const [registries, setRegistries] = useState<DockerRegistry[]>([]);
  const [registriesLoaded, setRegistriesLoaded] = useState(false);
  const [settings, setSettings] = useState<DockerEngineSettingsView | null>(null);
  const [settingsLoaded, setSettingsLoaded] = useState(false);
  const [settingsBusy, setSettingsBusy] = useState(false);
  const [dockerTab, setDockerTab] = useState<DockerTab>("containers");
  const [processes, setProcesses] = useState<Pm2Process[]>([]);
  const [pending, setPending] = useState<PendingAction | null>(null);
  const [loading, setLoading] = useState(false);
  const [errors, setErrors] = useState<Partial<Record<OperationsSection, boolean>>>({});
  const [outputs, setOutputs] = useState<Partial<Record<OperationsSection, string>>>({});
  const error = errors[section] ?? false;
  const output = outputs[section] ?? "";
  const [logSource, setLogSource] = useState<LogSource>("system");
  const [logTarget, setLogTarget] = useState("");
  const [logLines, setLogLines] = useState(200);

  const refreshContainers = useCallback(async () => {
    if (!sessionId) return;
    setContainers(await listDocker(sessionId));
  }, [sessionId]);

  const refreshImages = useCallback(async () => {
    if (!sessionId) return;
    setImages(await listDockerImages(sessionId));
    setImagesLoaded(true);
  }, [sessionId]);

  const refreshNetworks = useCallback(async () => {
    if (!sessionId) return;
    setNetworks(await listDockerNetworks(sessionId));
    setNetworksLoaded(true);
  }, [sessionId]);

  const refreshVolumes = useCallback(async () => {
    if (!sessionId) return;
    setVolumes(await listDockerVolumes(sessionId));
    setVolumesLoaded(true);
  }, [sessionId]);

  const refreshRegistries = useCallback(async () => {
    if (!sessionId) return;
    setRegistries(await listDockerRegistries(sessionId));
    setRegistriesLoaded(true);
  }, [sessionId]);

  const refreshSettings = useCallback(async () => {
    if (!sessionId) return;
    setSettingsBusy(true);
    try {
      setSettings(await getDockerSettings(sessionId));
      setSettingsLoaded(true);
      setErrors((previous) => ({ ...previous, docker: false }));
    } catch {
      setSettings(null);
      setSettingsLoaded(true);
      setErrors((previous) => ({ ...previous, docker: true }));
      throw new Error("docker settings load failed");
    } finally {
      setSettingsBusy(false);
    }
  }, [sessionId]);

  const refresh = useCallback(async () => {
    if (!sessionId) return;
    setLoading(true);
    setErrors((previous) => ({ ...previous, [section]: false }));
    try {
      if (section === "docker") {
        await refreshContainers();
        if (imagesLoaded) await refreshImages();
        if (networksLoaded) await refreshNetworks();
        if (volumesLoaded) await refreshVolumes();
        if (registriesLoaded) await refreshRegistries();
        if (settingsLoaded) {
          try {
            await refreshSettings();
          } catch {
            // Settings panel surfaces its own failure state.
          }
        }
      } else if (section === "pm2") {
        setProcesses(await listPm2(sessionId));
      }
    } catch {
      setErrors((previous) => ({ ...previous, [section]: true }));
    } finally {
      setLoading(false);
    }
  }, [imagesLoaded, networksLoaded, registriesLoaded, refreshContainers, refreshImages, refreshNetworks, refreshRegistries, refreshSettings, refreshVolumes, section, sessionId, settingsLoaded, volumesLoaded]);

  useEffect(() => {
    if (active) void refresh();
  }, [active, refresh]);

  const ensureImages = useCallback(async () => {
    if (!sessionId || imagesLoaded) return;
    setLoading(true);
    setErrors((previous) => ({ ...previous, docker: false }));
    try {
      await refreshImages();
    } catch {
      setErrors((previous) => ({ ...previous, docker: true }));
    } finally {
      setLoading(false);
    }
  }, [imagesLoaded, refreshImages, sessionId]);

  const ensureNetworks = useCallback(async () => {
    if (!sessionId || networksLoaded) return;
    setLoading(true);
    setErrors((previous) => ({ ...previous, docker: false }));
    try {
      await refreshNetworks();
    } catch {
      setErrors((previous) => ({ ...previous, docker: true }));
    } finally {
      setLoading(false);
    }
  }, [networksLoaded, refreshNetworks, sessionId]);

  const ensureVolumes = useCallback(async () => {
    if (!sessionId || volumesLoaded) return;
    setLoading(true);
    setErrors((previous) => ({ ...previous, docker: false }));
    try {
      await refreshVolumes();
    } catch {
      setErrors((previous) => ({ ...previous, docker: true }));
    } finally {
      setLoading(false);
    }
  }, [refreshVolumes, sessionId, volumesLoaded]);

  const ensureRegistries = useCallback(async () => {
    if (!sessionId || registriesLoaded) return;
    setLoading(true);
    setErrors((previous) => ({ ...previous, docker: false }));
    try {
      await refreshRegistries();
    } catch {
      setErrors((previous) => ({ ...previous, docker: true }));
    } finally {
      setLoading(false);
    }
  }, [refreshRegistries, registriesLoaded, sessionId]);

  const ensureSettings = useCallback(async () => {
    if (!sessionId || settingsLoaded) return;
    try {
      await refreshSettings();
    } catch {
      // refreshSettings already records the docker error state
    }
  }, [refreshSettings, sessionId, settingsLoaded]);

  if (!sessionId) {
    return <div className="grid h-full place-items-center text-sm text-[hsl(var(--muted))]">{t("operations.connectRequired")}</div>;
  }

  const clearDockerOutput = () => {
    setOutputs((previous) => {
      if (previous.docker == null) return previous;
      const next = { ...previous };
      delete next.docker;
      return next;
    });
  };

  const run = async (action: PendingAction) => {
    if (action.kind === "docker") {
      const result = await actOnDocker(sessionId, action.target, action.action as ResourceAction);
      if (!result.success) {
        throw new Error(result.output.trim() || "docker action failed");
      }
      toast.success(t(`operations.dockerContainers.${action.action}Success`, { name: action.label }));
      clearDockerOutput();
      setErrors((previous) => ({ ...previous, docker: false }));
      await refresh();
      return;
    }

    const result = action.kind === "pm2"
      ? await actOnPm2(sessionId, action.target, action.action as ResourceAction)
      : await actOnNginx(sessionId, action.action as "test" | "reload");
    setOutputs((previous) => ({ ...previous, [action.kind]: result.output }));
    setErrors((previous) => ({ ...previous, [action.kind]: !result.success }));
    await refresh();
  };

  const runImageAction = async (action: DockerImageAction) => {
    const result = await actOnDockerImage(sessionId, action);
    if (!result.success) {
      throw new Error(result.output.trim() || "docker image action failed");
    }
    // Image workspace refreshes the table; do not dump raw docker CLI output into the page.
    clearDockerOutput();
    setErrors((previous) => ({ ...previous, docker: false }));
    await refreshImages();
    if (action.type === "createContainer") await refreshContainers();
  };

  const runNetworkAction = async (action: DockerNetworkAction) => {
    const result = await actOnDockerNetwork(sessionId, action);
    if (!result.success) {
      throw new Error(result.output.trim() || "docker network action failed");
    }
    clearDockerOutput();
    setErrors((previous) => ({ ...previous, docker: false }));
    await refreshNetworks();
  };

  const runVolumeAction = async (action: DockerVolumeAction) => {
    const result = await actOnDockerVolume(sessionId, action);
    if (!result.success) {
      throw new Error(result.output.trim() || "docker volume action failed");
    }
    clearDockerOutput();
    setErrors((previous) => ({ ...previous, docker: false }));
    await refreshVolumes();
  };

  const runRegistryUpsert = async (input: DockerRegistryUpsertInput) => {
    await upsertDockerRegistry(sessionId, input);
    clearDockerOutput();
    setErrors((previous) => ({ ...previous, docker: false }));
    await refreshRegistries();
  };

  const runRegistryDelete = async (ids: string[]) => {
    const result = await deleteDockerRegistries(sessionId, ids);
    if (!result.success) {
      throw new Error(result.output.trim() || "docker registry delete failed");
    }
    clearDockerOutput();
    setErrors((previous) => ({ ...previous, docker: false }));
    await refreshRegistries();
  };

  const runApplySettings = async (config: DockerDaemonConfig) => {
    const result = await applyDockerSettings(sessionId, config, true);
    if (!result.success) {
      throw new Error(result.output.trim() || "docker settings apply failed");
    }
    clearDockerOutput();
    setErrors((previous) => ({ ...previous, docker: false }));
  };

  const actionButtons = (kind: "docker" | "pm2", target: string, label: string, state?: string) => (
    <div className="flex gap-1">
      {(["start", "stop", "restart"] as const).map((action) => {
        const startDisabled = kind === "docker" && action === "start" && state === "running";
        return (
          <Button
            key={action}
            variant="ghost"
            size="icon"
            className="h-7 w-7"
            disabled={startDisabled}
            title={t(`operations.action.${action}`)}
            aria-label={t("operations.resourceAction", { action: t(`operations.action.${action}`), target: label })}
            onClick={() => setPending({ kind, target, label, action })}
          >
            {action === "start" ? <Play size={13} /> : action === "stop" ? <Square size={13} /> : <RotateCcw size={13} />}
          </Button>
        );
      })}
    </div>
  );

  const sections = [
    { id: "docker" as const, icon: Box },
    { id: "pm2" as const, icon: Workflow },
    { id: "nginx" as const, icon: ServerCog },
    { id: "logs" as const, icon: FileText },
  ];

  const hideDockerUnsupported =
    section === "docker" && (dockerTab === "settings" || dockerTab === "onlineImages");
  const showUnsupported = error && !hideDockerUnsupported;

  return (
    <section className="flex h-full min-h-0 flex-col bg-[hsl(var(--surface))]" aria-label={t("operations.title")}>
      <nav className="flex h-11 shrink-0 items-center gap-1 border-b px-3" aria-label={t("operations.title")}>
        {sections.map((item) => (
          <Button key={item.id} variant={section === item.id ? "secondary" : "ghost"} size="sm" onClick={() => setSection(item.id)}>
            <item.icon size={15} />
            {t(`operations.${item.id}`)}
          </Button>
        ))}
        <Button variant="ghost" size="icon" className="ml-auto" disabled={loading} aria-label={t("common.refresh")} onClick={() => void refresh()}>
          <RefreshCw size={16} className={loading ? "animate-spin" : undefined} />
        </Button>
      </nav>
      <div className={cn("min-h-0 flex-1", section === "logs" ? "flex flex-col overflow-hidden p-4" : section === "docker" ? "flex flex-col overflow-hidden" : "overflow-auto p-4")}>
        {section !== "docker" && showUnsupported && (
          <div className="mb-4 shrink-0 rounded-lg border border-red-500/30 bg-red-500/10 p-3 text-sm text-red-500">
            {t("operations.unsupported")}
          </div>
        )}
        {section === "docker" && (
          <DockerPanel
            sessionId={sessionId}
            containers={containers}
            images={images}
            networks={networks}
            volumes={volumes}
            registries={registries}
            imagesBusy={loading}
            networksBusy={loading && networksLoaded}
            volumesBusy={loading && volumesLoaded}
            registriesBusy={loading && registriesLoaded}
            settings={settings}
            settingsBusy={settingsBusy || (loading && settingsLoaded)}
            unsupportedMessage={showUnsupported ? t("operations.unsupported") : null}
            onTabChange={setDockerTab}
            onImagesTab={() => void ensureImages()}
            onNetworksTab={() => void ensureNetworks()}
            onVolumesTab={() => void ensureVolumes()}
            onRegistriesTab={() => void ensureRegistries()}
            onSettingsTab={() => void ensureSettings()}
            onReloadSettings={async () => { await refreshSettings(); }}
            onApplySettings={runApplySettings}
            onImageAction={runImageAction}
            onNetworkAction={runNetworkAction}
            onVolumeAction={runVolumeAction}
            onRegistryUpsert={runRegistryUpsert}
            onRegistryDelete={runRegistryDelete}
            actionButtons={actionButtons}
          />
        )}
        {section === "pm2" && (
          <div className="overflow-hidden rounded-lg border">
            <table className="w-full text-left text-sm">
              <thead className="bg-[hsl(var(--elevated))] text-xs">
                <tr>
                  <th className="px-3 py-2">{t("operations.name")}</th>
                  <th className="px-3 py-2">{t("operations.status")}</th>
                  <th className="px-3 py-2">CPU</th>
                  <th className="px-3 py-2">MEM</th>
                  <th className="w-28 px-3 py-2">{t("operations.actions")}</th>
                </tr>
              </thead>
              <tbody>
                {processes.map((item) => (
                  <tr key={item.id} className="border-t">
                    <td className="px-3 py-2">{item.name}</td>
                    <td className="px-3 py-2">{item.status}</td>
                    <td className="px-3 py-2">{item.cpuPercent}%</td>
                    <td className="px-3 py-2">{formatFileSize(item.memoryBytes, i18n.language)}</td>
                    <td className="px-3 py-2">{actionButtons("pm2", String(item.id), item.name)}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
        {section === "nginx" && (
          <div className="rounded-lg border p-4">
            <h3 className="font-medium">{t("operations.nginxTools")}</h3>
            <p className="mt-1 text-sm text-[hsl(var(--secondary))]">{t("operations.nginxHint")}</p>
            <div className="mt-4 flex gap-2">
              <Button variant="secondary" onClick={() => setPending({ kind: "nginx", target: "nginx", action: "test" })}>{t("operations.action.test")}</Button>
              <Button onClick={() => setPending({ kind: "nginx", target: "nginx", action: "reload" })}>{t("operations.action.reload")}</Button>
            </div>
          </div>
        )}
        {section === "logs" && (
          <div className="grid shrink-0 gap-2 md:grid-cols-[180px_1fr_120px_auto]">
            <LogSourceSelect value={logSource} onValueChange={setLogSource} />
            <Input value={logTarget} disabled={!(["docker", "pm2", "service"] as LogSource[]).includes(logSource)} placeholder={t("operations.logTarget")} onChange={(event) => setLogTarget(event.target.value)} />
            <Input type="number" min={20} max={5000} value={logLines} onChange={(event) => setLogLines(Number(event.target.value))} />
            <Button onClick={async () => {
              setErrors((previous) => ({ ...previous, logs: false }));
              try {
                const result = await readLogs(sessionId, logSource, logTarget || null, logLines);
                setOutputs((previous) => ({ ...previous, logs: result.output }));
              } catch {
                setErrors((previous) => ({ ...previous, logs: true }));
              }
            }}>{t("operations.readLogs")}</Button>
          </div>
        )}
        {(section === "logs" || (output && section !== "docker")) && (
          <pre className={cn("overflow-auto whitespace-pre-wrap rounded-lg bg-slate-950 p-4 font-mono text-xs text-slate-100", section === "logs" ? "mt-4 min-h-0 flex-1" : "mx-4 mb-4 mt-4 max-h-96")}>{output}</pre>
        )}
      </div>
      {pending && <ActionDialog action={pending} onClose={() => setPending(null)} onConfirm={() => run(pending)} />}
    </section>
  );
}
