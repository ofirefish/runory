import { FileText, FolderKanban, Play, Plus, Settings2, Trash2 } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "../../components/ui/button";
import { appErrorCode } from "../../lib/app-error";
import {
  deleteDeploymentApp,
  provisionProxyProject,
  readLogs,
  upsertDeploymentApp,
} from "../../lib/tauri/infrastructure";
import type { DeploymentApp, DeploymentSetupStatus } from "../../types/infrastructure";
import { deploymentConflictErrorKey } from "./deployment-app-errors";
import { draftFromProxyApp, emptyProxyDraft, proxyConfigFromDraft, type ProxyProjectDraft } from "./proxy-project-draft";
import { ProxyProjectWizard } from "./ProxyProjectWizard";
import { ConfirmOperation, type Pending } from "./PythonProjectPanel";

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
    projectKind: "reverseProxy" as const,
    proxy: app.proxy ?? null,
    setupStep: app.setupStep ?? 3,
    setupStatus,
    sourceKind: app.sourceKind,
    venvPath: app.venvPath,
  };
}

/** A static site has no process, so the list shows the last deployment result instead of a live service state. */
export function ProxyProjectPanel({
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
  const [form, setForm] = useState<ProxyProjectDraft | null>(null);
  const [formBusy, setFormBusy] = useState(false);
  const [formError, setFormError] = useState<string | null>(null);
  const [pending, setPending] = useState<Pending | null>(null);
  const [output, setOutput] = useState("");
  const [error, setError] = useState(false);
  const [loadingLogs, setLoadingLogs] = useState(false);

  const saveDraft = async (draft: ProxyProjectDraft): Promise<ProxyProjectDraft> => {
    setFormBusy(true);
    setFormError(null);
    try {
      const saved = await upsertDeploymentApp({
        id: draft.id,
        profileId,
        name: draft.name,
        repositoryPath: "",
        remoteUrl: "",
        branch: "main",
        build: "none",
        restart: { kind: "none" },
        projectKind: "reverseProxy",
        proxy: draft.setupStep >= 2 ? proxyConfigFromDraft(draft) : null,
        setupStep: draft.setupStep,
        setupStatus: draft.setupStatus,
        sourceKind: "existingPath",
      });
      await onChanged();
      const next = draftFromProxyApp(saved);
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
    if (!sessionId || !app.proxy) throw new Error("disconnected");
    await writeSetupStatus(app, "provisioning");
    try {
      const result = await provisionProxyProject(sessionId, app.name, app.proxy);
      setOutput(result.output || t("deployment.success"));
      setError(!result.success);
      await writeSetupStatus(app, result.success ? "provisioned" : "failed");
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
    if (!app.proxy || !sessionId) return;
    setPending({
      description: t("deployment.proxy.provisionConfirm", { name: app.name }),
      run: async () => { await runProvision(app); },
    });
  };

  const showLogs = async () => {
    if (!sessionId) return;
    setLoadingLogs(true);
    try {
      const result = await readLogs(sessionId, "nginx-error", null, 200);
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
    setForm(emptyProxyDraft());
  };

  const openEdit = (app: DeploymentApp) => {
    setFormError(null);
    setForm(draftFromProxyApp(app));
  };

  return (
    <div className="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden">
      {apps.length === 0 ? (
        <div className="flex min-h-0 flex-1 flex-col items-center justify-center gap-3 px-6 text-center">
          <div className="flex h-12 w-12 items-center justify-center rounded-full border border-[hsl(var(--border))] bg-[hsl(var(--elevated))] text-[hsl(var(--secondary))]">
            <FolderKanban size={22} />
          </div>
          <div className="grid gap-1">
            <h3 className="text-base font-medium text-foreground">{t("deployment.proxy.emptyTitle")}</h3>
            <p className="max-w-sm text-sm text-[hsl(var(--secondary))]">{t("deployment.proxy.emptyHint")}</p>
          </div>
          <Button onClick={openCreate}>
            <Plus size={15} />
            {t("deployment.proxy.addProject")}
          </Button>
          {!sessionId && <p className="text-xs text-amber-600">{t("deployment.connectRequiredAction")}</p>}
        </div>
      ) : (
        <>
          <div className="flex h-11 shrink-0 items-center gap-2 border-b px-3">
            <Button size="sm" onClick={openCreate}>
              <Plus size={15} />
              {t("deployment.proxy.addProject")}
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
                      <th className="px-3 py-2 font-medium">{t("deployment.proxy.name")}</th>
                      <th className="px-3 py-2 font-medium">{t("deployment.python.status")}</th>
                      <th className="hidden px-3 py-2 font-medium md:table-cell">{t("deployment.proxy.targetUrl")}</th>
                      <th className="px-3 py-2 font-medium">{t("deployment.proxy.port")}</th>
                      <th className="hidden px-3 py-2 font-medium sm:table-cell">{t("deployment.proxy.serverName")}</th>
                      <th className="px-3 py-2 font-medium">{t("deployment.python.actions")}</th>
                    </tr>
                  </thead>
                  <tbody>
                    {apps.map((app) => {
                      const deployable = canDeploy(app);
                      const deployDisabled = !sessionId || !deployable || app.setupStatus === "provisioning";
                      const status = app.setupStatus ?? "draft";
                      return (
                        <tr key={app.id} className="border-t">
                          <td className="px-3 py-2 font-medium">{app.name}</td>
                          <td className="px-3 py-2">
                            <span className={status === "provisioned" ? "text-emerald-500" : status === "failed" ? "text-red-500" : "text-[hsl(var(--secondary))]"}>
                              {t(`deployment.proxy.status.${status}`)}
                            </span>
                          </td>
                          <td className="hidden max-w-64 truncate px-3 py-2 font-mono text-xs md:table-cell" title={app.proxy?.targetUrl}>{app.proxy?.targetUrl ?? "—"}</td>
                          <td className="px-3 py-2">{app.proxy && app.proxy.port > 0 ? app.proxy.port : "—"}</td>
                          <td className="hidden max-w-48 truncate px-3 py-2 sm:table-cell" title={app.proxy?.serverName}>{app.proxy?.serverName || "—"}</td>
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
                              {deployable && (
                                <Button size="sm" variant="ghost" disabled={!sessionId || loadingLogs} onClick={() => void showLogs()}>
                                  <FileText size={13} />{t("deployment.python.logs")}
                                </Button>
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
        <ProxyProjectWizard
          title={t(form.id ? "deployment.proxy.editProject" : "deployment.proxy.addProject")}
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
              repositoryPath: "",
              remoteUrl: "",
              branch: "main",
              build: "none",
              restart: { kind: "none" },
              projectKind: "reverseProxy",
              proxy: proxyConfigFromDraft(saved),
              setupStep: 3,
              setupStatus: "ready",
              sourceKind: "existingPath",
              venvPath: "",
              updatedAtEpochSeconds: 0,
            };
            setPending({
              description: t("deployment.proxy.createConfirm", { name: saved.name }),
              run: async () => { await runProvision(appLike); },
            });
          }}
        />
      )}
      {pending && <ConfirmOperation pending={pending} onClose={() => setPending(null)} />}
    </div>
  );
}
