import { deploymentConflictErrorKey } from "./deployment-app-errors";
import { FileText, FolderKanban, Play, Plus, RefreshCw, Settings2, Trash2 } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "../../components/ui/button";
import { DialogShell } from "../../components/ui/dialog-shell";
import { appErrorCode } from "../../lib/app-error";
import {
  deleteDeploymentApp,
  controlPythonProject,
  readLogs,
  provisionPythonProject,
  pythonProjectStatus,
  upsertDeploymentApp,
} from "../../lib/tauri/infrastructure";
import type { DeploymentApp, DeploymentSetupStatus } from "../../types/infrastructure";
import {
  draftFromPythonApp,
  emptyPythonDraft,
  pythonConfigFromDraft,
  type PythonProjectDraft,
} from "./python-project-draft";
import { PythonProjectWizard } from "./PythonProjectWizard";

export type Pending = { description: string; run: () => Promise<void> };

export function ConfirmOperation({ pending, onClose }: { pending: Pending; onClose: () => void }) {
  const { t } = useTranslation();
  const [busy, setBusy] = useState(false);
  const [failed, setFailed] = useState(false);
  const confirm = async () => {
    setBusy(true);
    setFailed(false);
    try {
      await pending.run();
      onClose();
    } catch {
      setFailed(true);
      setBusy(false);
    }
  };
  return (
    <DialogShell title={t("deployment.confirmTitle")} onClose={onClose} closeDisabled={busy}>
      <p className="text-sm text-[hsl(var(--secondary))]">{pending.description}</p>
      <p className="mt-3 text-xs text-amber-600">{t("deployment.confirmHint")}</p>
      {busy && <p className="mt-3 text-sm text-[hsl(var(--secondary))]" aria-live="polite">{t("deployment.python.working")}</p>}
      {failed && <p className="mt-3 text-sm text-red-500">{t("deployment.error")}</p>}
      <div className="mt-5 flex justify-end gap-2">
        <Button variant="ghost" disabled={busy} onClick={onClose}>
          {t("common.cancel")}
        </Button>
        <Button disabled={busy} onClick={() => void confirm()}>
          {t("deployment.confirm")}
        </Button>
      </div>
    </DialogShell>
  );
}

function serviceName(app: DeploymentApp): string {
  return app.restart.kind === "systemd" ? app.restart.service : "";
}

function canDeploy(app: DeploymentApp): boolean {
  return app.setupStatus === "ready"
    || app.setupStatus === "provisioning"
    || app.setupStatus === "provisioned"
    || app.setupStatus === "failed"
    || (!app.setupStatus && Boolean(app.python));
}

function venvPathForApp(app: DeploymentApp): string {
  return app.venvPath?.trim() ?? "";
}

export function PythonProjectPanel({
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
  const [form, setForm] = useState<PythonProjectDraft | null>(null);
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
        next[app.id] = result.success && ["active", "inactive", "failed", "activating", "deactivating"].includes(state) ? state : "unknown";
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

  const saveDraft = async (draft: PythonProjectDraft): Promise<PythonProjectDraft> => {
    setFormBusy(true);
    setFormError(null);
    try {
      const includePython = draft.setupStep >= 3;
      const saved = await upsertDeploymentApp({
        id: draft.id,
        profileId,
        name: draft.name,
        repositoryPath: draft.projectPath,
        remoteUrl: draft.sourceKind === "gitClone" ? draft.remoteUrl : "",
        branch: draft.branch || "master",
        build: "pip",
        restart: { kind: "none" },
        projectKind: "python",
        python: includePython ? pythonConfigFromDraft(draft) : null,
        setupStep: draft.setupStep,
        setupStatus: draft.setupStatus,
        sourceKind: draft.sourceKind,
        venvPath: draft.useExistingVenv ? draft.venvPath : "",
      });
      await onChanged();
      const next = draftFromPythonApp(saved);
      setForm(next);
      return next;
    } catch (caught) {
      const code = appErrorCode(caught);
      setFormError(
        deploymentConflictErrorKey(code) ?? (code === "INVALID_OPERATION"
          ? "deployment.validation.invalid"
          : code === "UNSUPPORTED_REMOTE"
            ? "deployment.python.unsupported"
            : "deployment.error"),
      );
      throw caught;
    } finally {
      setFormBusy(false);
    }
  };

  const writeSetupStatus = async (app: DeploymentApp, setupStatus: DeploymentSetupStatus) => {
    await upsertDeploymentApp({
      id: app.id,
      profileId: app.profileId,
      name: app.name,
      repositoryPath: app.repositoryPath,
      remoteUrl: app.remoteUrl,
      branch: app.branch,
      build: app.build,
      restart: app.restart,
      projectKind: app.projectKind ?? "python",
      python: app.python ?? null,
      setupStep: app.setupStep ?? 4,
      setupStatus,
      sourceKind: app.sourceKind,
      venvPath: app.venvPath,
    });
    await onChanged();
  };

  const runProvision = async (app: DeploymentApp) => {
    if (!sessionId || !app.python) throw new Error("disconnected");
    await writeSetupStatus(app, "provisioning");
    try {
      const result = await provisionPythonProject(
        sessionId,
        app.name,
        app.repositoryPath,
        app.python,
        venvPathForApp(app),
      );
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
    if (!app.python || !sessionId) return;
    setPending({
      description: t("deployment.python.provisionConfirm", { name: app.name }),
      run: async () => {
        await runProvision(app);
      },
    });
  };

  const controlApp = (app: DeploymentApp, action: "start" | "stop" | "restart") => {
    if (!sessionId) return;
    setPending({
      description: t("deployment.python.controlConfirm", { name: app.name, action: t(`deployment.python.action.${action}`) }),
      run: async () => {
        try {
          const result = await controlPythonProject(sessionId, app.id, action);
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
    setForm(emptyPythonDraft());
  };

  const openEdit = (app: DeploymentApp) => {
    setFormError(null);
    setForm(draftFromPythonApp(app));
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
            <h3 className="text-base font-medium text-foreground">{t("deployment.python.emptyTitle")}</h3>
            <p className="max-w-sm text-sm text-[hsl(var(--secondary))]">{t("deployment.python.emptyHint")}</p>
          </div>
          <Button onClick={openCreate}>
            <Plus size={15} />
            {t("deployment.python.addProject")}
          </Button>
          {!sessionId && <p className="text-xs text-amber-600">{t("deployment.connectRequiredAction")}</p>}
        </div>
      ) : (
        <>
          <div className="flex h-11 shrink-0 items-center gap-2 border-b px-3">
            <Button size="sm" onClick={openCreate}>
              <Plus size={15} />
              {t("deployment.python.addProject")}
            </Button>
            <Button
              variant="ghost"
              size="icon"
              className="ml-auto"
              disabled={!sessionId || loadingStatus}
              aria-label={t("common.refresh")}
              onClick={() => void refreshStatuses()}
            >
              <RefreshCw size={16} className={loadingStatus ? "animate-spin" : undefined} />
            </Button>
          </div>
          <div className="min-h-0 flex-1 overflow-auto p-4">
            {!sessionId && (
              <p className="mb-3 text-xs text-amber-600">{t("deployment.connectRequiredAction")}</p>
            )}
            {error && <div className="mb-4 rounded-lg border border-red-500/30 bg-red-500/10 p-3 text-sm text-red-500">{t("deployment.error")}</div>}
            <div className="overflow-hidden rounded-lg border">
              <div className="overflow-x-auto">
                <table className="w-full min-w-[40rem] text-left text-sm">
                  <thead className="sticky top-0 bg-[hsl(var(--elevated))] text-xs text-[hsl(var(--secondary))]">
                    <tr>
                      <th className="px-3 py-2 font-medium">{t("deployment.python.name")}</th>
                      <th className="px-3 py-2 font-medium">{t("deployment.python.status")}</th>
                      <th className="hidden px-3 py-2 font-medium md:table-cell">{t("deployment.python.projectPath")}</th>
                      <th className="px-3 py-2 font-medium">{t("deployment.python.port")}</th>
                      <th className="hidden px-3 py-2 font-medium sm:table-cell">{t("deployment.python.startup")}</th>
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
                          <td className="hidden max-w-64 truncate px-3 py-2 font-mono text-xs md:table-cell" title={app.repositoryPath}>
                            {app.repositoryPath}
                          </td>
                          <td className="px-3 py-2">{app.python && app.python.port > 0 ? app.python.port : "—"}</td>
                          <td className="hidden px-3 py-2 sm:table-cell">{app.python ? t(`deployment.python.startup.${app.python.startup}`) : "—"}</td>
                          <td className="px-3 py-2">
                            <div className="flex flex-wrap gap-1">
                              {deployable ? (
                                <Button
                                  size="sm"
                                  variant="ghost"
                                  disabled={deployDisabled}
                                  title={!sessionId ? t("deployment.connectRequiredAction") : undefined}
                                  onClick={() => deployApp(app)}
                                >
                                  <Play size={13} />
                                  {t("deployment.python.deploy")}
                                </Button>
                              ) : (
                                <Button size="sm" variant="ghost" onClick={() => openEdit(app)}>
                                  {t("deployment.python.continue")}
                                </Button>
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
            {output && (
              <pre className="mt-4 max-h-64 overflow-auto whitespace-pre-wrap rounded-lg bg-slate-950 p-4 font-mono text-xs text-slate-100">{output}</pre>
            )}
          </div>
        </>
      )}
      {form && (
        <PythonProjectWizard
          title={t(form.id ? "deployment.python.editProject" : "deployment.python.addProject")}
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
              build: "pip",
              restart: { kind: "none" },
              projectKind: "python",
              python: pythonConfigFromDraft(saved),
              setupStep: 4,
              setupStatus: "ready",
              sourceKind: saved.sourceKind,
              venvPath: saved.useExistingVenv ? saved.venvPath : "",
              updatedAtEpochSeconds: 0,
            };
            setPending({
              description: t("deployment.python.createConfirm", { name: saved.name }),
              run: async () => {
                await runProvision(appLike);
              },
            });
          }}
        />
      )}
      {pending && <ConfirmOperation pending={pending} onClose={() => setPending(null)} />}
    </div>
  );
}
