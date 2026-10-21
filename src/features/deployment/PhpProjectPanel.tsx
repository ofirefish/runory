import { FileText, FolderKanban, Play, Plus, RefreshCw, Settings2, Trash2 } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "../../components/ui/button";
import { appErrorCode } from "../../lib/app-error";
import {
  controlPhpProject,
  deleteDeploymentApp,
  provisionPhpProject,
  pythonProjectStatus,
  readLogs,
  upsertDeploymentApp,
} from "../../lib/tauri/infrastructure";
import type { DeploymentApp, DeploymentSetupStatus } from "../../types/infrastructure";
import { deploymentConflictErrorKey } from "./deployment-app-errors";
import { draftFromPhpApp, emptyPhpDraft, phpConfigFromDraft, type PhpProjectDraft } from "./php-project-draft";
import { PhpProjectWizard } from "./PhpProjectWizard";
import { ConfirmOperation, type Pending } from "./PythonProjectPanel";

function serviceName(app: DeploymentApp): string {
  return app.restart.kind === "systemd" ? app.restart.service : "";
}

function canDeploy(app: DeploymentApp): boolean {
  return app.setupStatus === "ready" || app.setupStatus === "provisioning" || app.setupStatus === "provisioned" || app.setupStatus === "failed";
}

function appPayload(app: DeploymentApp, setupStatus: DeploymentSetupStatus) {
  return {
    id: app.id,
    profileId: app.profileId,
    name: app.name,
    repositoryPath: app.repositoryPath,
    remoteUrl: app.remoteUrl,
    branch: app.branch,
    build: app.build,
    restart: app.restart,
    projectKind: "php" as const,
    php: app.php ?? null,
    setupStep: app.setupStep ?? 4,
    setupStatus,
    sourceKind: app.sourceKind,
    venvPath: app.venvPath,
  };
}

/** systemd state of the project's php-fpm unit (the status IPC is shared with Python projects). */
const statusStates = ["active", "inactive", "failed", "activating", "deactivating"];

export function PhpProjectPanel({
  sessionId,
  profileId,
  apps,
  onChanged,
}: {
  sessionId: string | null;
  profileId: string;
  apps: DeploymentApp[];
  onChanged: () => Promise<void>;
}) {
  const { t } = useTranslation();
  const [form, setForm] = useState<PhpProjectDraft | null>(null);
  const [formBusy, setFormBusy] = useState(false);
  const [formError, setFormError] = useState<string | null>(null);
  const [pending, setPending] = useState<Pending | null>(null);
  const [statuses, setStatuses] = useState<Record<string, string>>({});
  const [output, setOutput] = useState("");
  const [error, setError] = useState(false);
  const [loadingStatus, setLoadingStatus] = useState(false);
  const [loadingLogs, setLoadingLogs] = useState(false);
  const refreshId = useRef(0);

  const refreshStatuses = useCallback(async () => {
    const requestId = ++refreshId.current;
    if (!sessionId) {
      setStatuses({});
      return;
    }
    setLoadingStatus(true);
    const next: Record<string, string> = {};
    for (const app of apps) {
      const service = serviceName(app);
      if (!service || !canDeploy(app)) {
        next[app.id] = "unknown";
        continue;
      }
      try {
        const result = await pythonProjectStatus(sessionId, service);
        const state = result.output.trim();
        next[app.id] = result.success && statusStates.includes(state) ? state : "unknown";
      } catch {
        next[app.id] = "unknown";
      }
    }
    if (requestId === refreshId.current) {
      setStatuses(next);
      setLoadingStatus(false);
    }
  }, [apps, sessionId]);

  useEffect(() => {
    const generation = refreshId;
    void refreshStatuses();
    return () => { generation.current++; };
  }, [refreshStatuses]);

  const saveDraft = async (draft: PhpProjectDraft): Promise<PhpProjectDraft> => {
    setFormBusy(true);
    setFormError(null);
    try {
      const saved = await upsertDeploymentApp({
        id: draft.id,
        profileId,
        name: draft.name,
        repositoryPath: draft.projectPath,
        remoteUrl: draft.sourceKind === "gitClone" ? draft.remoteUrl : "",
        branch: draft.branch || "master",
        build: "none",
        restart: { kind: "none" },
        projectKind: "php",
        php: draft.setupStep >= 3 ? phpConfigFromDraft(draft) : null,
        setupStep: draft.setupStep,
        setupStatus: draft.setupStatus,
        sourceKind: draft.sourceKind,
      });
      await onChanged();
      const next = draftFromPhpApp(saved);
      setForm(next);
      return next;
    } catch (caught) {
      const code = appErrorCode(caught);
      setFormError(
        deploymentConflictErrorKey(code) ?? (code === "INVALID_OPERATION" ? "deployment.validation.invalid" : "deployment.error"),
      );
      throw caught;
    } finally {
      setFormBusy(false);
    }
  };

  const writeSetupStatus = async (app: DeploymentApp, setupStatus: DeploymentSetupStatus) => {
    await upsertDeploymentApp(appPayload(app, setupStatus));
    await onChanged();
  };

  const runProvision = async (app: DeploymentApp) => {
    if (!sessionId || !app.php) throw new Error("disconnected");
    await writeSetupStatus(app, "provisioning");
    try {
      const result = await provisionPhpProject(sessionId, app.name, app.repositoryPath, app.php);
      setOutput(result.output || t("deployment.success"));
      setError(!result.success);
      await writeSetupStatus(app, result.success ? "provisioned" : "failed");
      await refreshStatuses();
      if (!result.success) throw new Error("provision failed");
    } catch (caught) {
      setError(true);
      try {
        await writeSetupStatus(app, "failed");
      } catch {
        // Keep the original failure visible.
      }
      throw caught;
    }
  };

  const deployApp = (app: DeploymentApp) => {
    if (!app.php || !sessionId) return;
    setPending({
      description: t("deployment.php.provisionConfirm", { name: app.name }),
      run: async () => { await runProvision(app); },
    });
  };

  const controlApp = (app: DeploymentApp, action: "start" | "stop" | "restart") => {
    if (!sessionId) return;
    setPending({
      description: t("deployment.php.controlConfirm", { name: app.name, action: t(`deployment.python.action.${action}`) }),
      run: async () => {
        try {
          const result = await controlPhpProject(sessionId, app.id, action);
          setOutput(result.success ? t("deployment.python.controlSuccess") : result.output);
          setError(!result.success);
          if (!result.success) throw new Error("control failed");
        } catch (caught) {
          setError(true);
          throw caught;
        } finally {
          await refreshStatuses();
        }
      },
    });
  };

  const showLogs = async (app: DeploymentApp) => {
    if (!sessionId || !serviceName(app)) return;
    setLoadingLogs(true);
    try {
      const result = await readLogs(sessionId, "service", serviceName(app), 200);
      setOutput(result.output || t("deployment.python.noLogs"));
      setError(!result.success);
    } catch {
      setError(true);
    } finally {
      setLoadingLogs(false);
    }
  };

  const removeApp = (app: DeploymentApp) => {
    setPending({
      description: t("deployment.appDeleteConfirm"),
      run: async () => {
        await deleteDeploymentApp(profileId, app.id);
        await onChanged();
        setOutput(t("deployment.appDeleted"));
        setError(false);
      },
    });
  };

  const openCreate = () => {
    setFormError(null);
    setForm(emptyPhpDraft());
  };

  const openEdit = (app: DeploymentApp) => {
    setFormError(null);
    setForm(draftFromPhpApp(app));
  };

  return (
    <div className="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden">
      {apps.length === 0 ? (
        <div className="flex min-h-0 flex-1 flex-col items-center justify-center gap-3 px-6 text-center">
          {error && <div className="mb-2 rounded-lg border border-red-500/30 bg-red-500/10 px-3 py-2 text-sm text-red-500">{t("deployment.error")}</div>}
          <div className="flex h-12 w-12 items-center justify-center rounded-full border border-[hsl(var(--border))] bg-[hsl(var(--elevated))] text-[hsl(var(--secondary))]">
            <FolderKanban size={22} />
          </div>
          <div className="grid gap-1">
            <h3 className="text-base font-medium text-foreground">{t("deployment.php.emptyTitle")}</h3>
            <p className="max-w-sm text-sm text-[hsl(var(--secondary))]">{t("deployment.php.emptyHint")}</p>
          </div>
          <Button onClick={openCreate}>
            <Plus size={15} />
            {t("deployment.php.addProject")}
          </Button>
          {!sessionId && <p className="text-xs text-amber-600">{t("deployment.connectRequiredAction")}</p>}
        </div>
      ) : (
        <>
          <div className="flex h-11 shrink-0 items-center gap-2 border-b px-3">
            <Button size="sm" onClick={openCreate}>
              <Plus size={15} />
              {t("deployment.php.addProject")}
            </Button>
            <Button variant="ghost" size="icon" className="ml-auto" disabled={!sessionId || loadingStatus} aria-label={t("common.refresh")} onClick={() => void refreshStatuses()}>
              <RefreshCw size={16} className={loadingStatus ? "animate-spin" : undefined} />
            </Button>
          </div>
          <div className="min-h-0 flex-1 overflow-auto p-4">
            {!sessionId && <p className="mb-3 text-xs text-amber-600">{t("deployment.connectRequiredAction")}</p>}
            {error && <div className="mb-4 rounded-lg border border-red-500/30 bg-red-500/10 p-3 text-sm text-red-500">{t("deployment.error")}</div>}
            <div className="overflow-hidden rounded-lg border">
              <div className="overflow-x-auto">
                <table className="w-full min-w-[40rem] text-left text-sm">
                  <thead className="sticky top-0 bg-[hsl(var(--elevated))] text-xs text-[hsl(var(--secondary))]">
                    <tr>
                      <th className="px-3 py-2 font-medium">{t("deployment.php.name")}</th>
                      <th className="px-3 py-2 font-medium">{t("deployment.python.status")}</th>
                      <th className="hidden px-3 py-2 font-medium md:table-cell">{t("deployment.php.projectPath")}</th>
                      <th className="px-3 py-2 font-medium">{t("deployment.php.port")}</th>
                      <th className="hidden px-3 py-2 font-medium sm:table-cell">{t("deployment.php.version")}</th>
                      <th className="px-3 py-2 font-medium">{t("deployment.python.actions")}</th>
                    </tr>
                  </thead>
                  <tbody>
                    {apps.map((app) => {
                      const deployable = canDeploy(app);
                      const deployDisabled = !sessionId || !deployable || app.setupStatus === "provisioning";
                      return (
                        <tr key={app.id} className="border-t">
                          <td className="px-3 py-2 font-medium">{app.name}</td>
                          <td className="px-3 py-2">
                            <span className={statuses[app.id] === "active" ? "text-emerald-500" : "text-[hsl(var(--secondary))]"}>
                              {app.setupStatus === "provisioning"
                                ? t("deployment.python.status.provisioning")
                                : deployable
                                  ? t(`deployment.python.status.${statuses[app.id] ?? "unknown"}`)
                                  : t("deployment.python.status.draft")}
                            </span>
                          </td>
                          <td className="hidden max-w-64 truncate px-3 py-2 font-mono text-xs md:table-cell" title={app.repositoryPath}>{app.repositoryPath}</td>
                          <td className="px-3 py-2">{app.php && app.php.port > 0 ? app.php.port : "—"}</td>
                          <td className="hidden px-3 py-2 sm:table-cell">{app.php?.phpBin ?? "—"}</td>
                          <td className="px-3 py-2">
                            <div className="flex flex-wrap gap-1">
                              {deployable ? (
                                <Button size="sm" variant="ghost" disabled={deployDisabled} title={!sessionId ? t("deployment.connectRequiredAction") : undefined} onClick={() => deployApp(app)}>
                                  <Play size={13} />
                                  {t("deployment.python.deploy")}
                                </Button>
                              ) : (
                                <Button size="sm" variant="ghost" onClick={() => openEdit(app)}>{t("deployment.python.continue")}</Button>
                              )}
                              {deployable && serviceName(app) && (
                                <>
                                  {(["start", "stop", "restart"] as const).map((action) => (
                                    <Button key={action} size="sm" variant="ghost" disabled={deployDisabled || Boolean(pending)} onClick={() => controlApp(app, action)}>
                                      {t(`deployment.python.action.${action}`)}
                                    </Button>
                                  ))}
                                  <Button size="sm" variant="ghost" disabled={!sessionId || loadingLogs} onClick={() => void showLogs(app)}>
                                    <FileText size={13} />{t("deployment.python.logs")}
                                  </Button>
                                </>
                              )}
                              <Button size="sm" variant="ghost" disabled={app.setupStatus === "provisioning"} onClick={() => openEdit(app)}>
                                <Settings2 size={13} />
                                {t("deployment.python.settings")}
                              </Button>
                              <Button size="sm" variant="ghost" className="text-red-500" disabled={app.setupStatus === "provisioning"} onClick={() => removeApp(app)}>
                                <Trash2 size={13} />
                                {t("common.delete")}
                              </Button>
                            </div>
                          </td>
                        </tr>
                      );
                    })}
                  </tbody>
                </table>
              </div>
            </div>
            {output && <pre className="mt-4 max-h-64 overflow-auto whitespace-pre-wrap rounded-lg bg-slate-950 p-4 font-mono text-xs text-slate-100">{output}</pre>}
          </div>
        </>
      )}
      {form && (
        <PhpProjectWizard
          title={t(form.id ? "deployment.php.editProject" : "deployment.php.addProject")}
          initial={form}
          sessionId={sessionId}
          busy={formBusy}
          errorKey={formError}
          onClose={() => setForm(null)}
          onSave={saveDraft}
          onFinish={async (draft) => {
            const saved = await saveDraft(draft);
            setForm(null);
            if (!sessionId) {
              setOutput(t("deployment.python.savedReady"));
              setError(false);
              return;
            }
            const appLike: DeploymentApp = {
              id: saved.id ?? "",
              profileId,
              name: saved.name,
              repositoryPath: saved.projectPath,
              remoteUrl: saved.sourceKind === "gitClone" ? saved.remoteUrl : "",
              branch: saved.branch,
              build: "none",
              restart: { kind: "none" },
              projectKind: "php",
              php: phpConfigFromDraft(saved),
              setupStep: 4,
              setupStatus: "ready",
              sourceKind: saved.sourceKind,
              venvPath: "",
              updatedAtEpochSeconds: 0,
            };
            setPending({
              description: t("deployment.php.createConfirm", { name: saved.name }),
              run: async () => { await runProvision(appLike); },
            });
          }}
        />
      )}
      {pending && <ConfirmOperation pending={pending} onClose={() => setPending(null)} />}
    </div>
  );
}
