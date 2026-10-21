import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "../../components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "../../components/ui/dialog";
import { appErrorCode } from "../../lib/app-error";
import { listNodeRuntimes, listSystemUsers, setupGit } from "../../lib/tauri/infrastructure";
import type { DeploymentSetupStatus, NodePackageManager, NodeRuntime, NodeStartMode } from "../../types/infrastructure";
import { RemotePathField } from "../files/RemotePathField";
import { deploymentConflictErrorKey } from "./deployment-app-errors";
import { DeploymentInputField as Field, DeploymentSelectField as SelectField } from "./DeploymentFields";
import { normalizeNodeDraft, validateNodeDraft, validateNodeStep, type NodeProjectDraft } from "./node-project-draft";

type Step = 1 | 2 | 3 | 4;
const wizardSteps = [1, 2, 3, 4] as const;
const packageManagers = ["npm", "yarn", "pnpm"] as const satisfies readonly NodePackageManager[];
const startModes = ["script", "entry"] as const satisfies readonly NodeStartMode[];

function WizardStepChrome({ step }: { step: Step }) {
  const { t } = useTranslation();
  const labels = [
    t("deployment.node.step.project"),
    t("deployment.node.step.git"),
    t("deployment.node.step.environment"),
    t("deployment.node.step.run"),
  ] as const;
  return (
    <ol className="mb-4 flex items-center gap-1 text-xs" aria-label={t("deployment.python.stepProgress", { current: step, total: 4 })}>
      {wizardSteps.map((item, index) => {
        const active = item === step;
        const done = item < step;
        return (
          <li key={item} className="flex min-w-0 flex-1 items-center gap-1">
            <span
              className={[
                "flex h-6 w-6 shrink-0 items-center justify-center rounded-full border text-[11px] font-medium",
                active
                  ? "border-[hsl(var(--primary))] bg-[hsl(var(--primary))] text-[hsl(var(--primary-foreground))]"
                  : done
                    ? "border-[hsl(var(--primary)/0.45)] text-[hsl(var(--primary))]"
                    : "border-[hsl(var(--border))] text-[hsl(var(--secondary))]",
              ].join(" ")}
              aria-current={active ? "step" : undefined}
            >
              {item}
            </span>
            <span className={`min-w-0 truncate ${active ? "font-medium text-foreground" : "text-[hsl(var(--secondary))]"}`}>
              {labels[index]}
            </span>
            {index < wizardSteps.length - 1 && <span className="mx-1 hidden h-px flex-1 bg-[hsl(var(--border))] sm:block" aria-hidden />}
          </li>
        );
      })}
    </ol>
  );
}

export function NodeProjectWizard({
  title,
  initial,
  sessionId,
  busy,
  errorKey,
  onClose,
  onSave,
  onFinish,
}: {
  title: string;
  initial: NodeProjectDraft;
  sessionId: string | null;
  busy?: boolean;
  errorKey?: string | null;
  onClose: () => void;
  onSave: (draft: NodeProjectDraft) => Promise<NodeProjectDraft | void>;
  onFinish: (draft: NodeProjectDraft) => Promise<void>;
}) {
  const { t } = useTranslation();
  const [draft, setDraft] = useState(initial);
  const [step, setStep] = useState<Step>(initial.setupStep);
  const [localError, setLocalError] = useState<string | null>(null);
  const [working, setWorking] = useState(false);
  const [progressKey, setProgressKey] = useState<string | null>(null);
  const [systemUsers, setSystemUsers] = useState<string[]>([]);
  const [runtimes, setRuntimes] = useState<NodeRuntime[]>([]);
  const blocked = busy || working;
  const set = <K extends keyof NodeProjectDraft>(key: K, value: NodeProjectDraft[K]) => {
    setDraft((previous) => ({ ...previous, [key]: value }));
    setLocalError(null);
  };

  useEffect(() => {
    if (!sessionId || step !== 4) return;
    let cancelled = false;
    void listSystemUsers(sessionId)
      .then((users) => { if (!cancelled) setSystemUsers(users); })
      .catch(() => { if (!cancelled) setSystemUsers([]); });
    return () => { cancelled = true; };
  }, [sessionId, step]);

  useEffect(() => {
    if (!sessionId || step !== 3) return;
    let cancelled = false;
    void listNodeRuntimes(sessionId)
      .then((items) => { if (!cancelled) setRuntimes(items); })
      .catch(() => { if (!cancelled) setRuntimes([]); });
    return () => { cancelled = true; };
  }, [sessionId, step]);

  const mapRemoteError = (caught: unknown, fallback: string) => {
    const code = appErrorCode(caught);
    const specific = deploymentConflictErrorKey(code);
    if (specific) return specific;
    if (code === "UNSUPPORTED_REMOTE") return "deployment.node.unsupported";
    if (code === "INVALID_OPERATION") return "deployment.validation.invalid";
    return fallback;
  };

  const persist = async (nextStep: Step, status: DeploymentSetupStatus, patch?: Partial<NodeProjectDraft>) => {
    const next = normalizeNodeDraft({ ...draft, ...patch, setupStep: nextStep, setupStatus: status });
    const error = validateNodeStep(next, step);
    if (error) {
      setLocalError(error);
      return false;
    }
    setWorking(true);
    setLocalError(null);
    try {
      if (step === 2 && next.sourceKind === "gitClone") {
        if (!sessionId) {
          setLocalError("deployment.connectRequiredAction");
          return false;
        }
        setProgressKey("deployment.python.cloning");
        try {
          const result = await setupGit(sessionId, next.projectPath, next.remoteUrl, next.branch);
          if (!result.success) {
            setLocalError("deployment.python.gitFailed");
            return false;
          }
        } catch (caught) {
          setLocalError(mapRemoteError(caught, "deployment.python.gitFailed"));
          return false;
        }
      }
      setProgressKey("deployment.python.saving");
      let saved: NodeProjectDraft | void;
      try {
        saved = await onSave(next);
      } catch {
        return false;
      }
      setDraft(saved ?? next);
      setStep(nextStep);
      return true;
    } finally {
      setWorking(false);
      setProgressKey(null);
    }
  };

  const goNext = async () => {
    if (blocked) return;
    if (step === 1) {
      await persist(2, "draft");
    } else if (step === 2) {
      const remoteUrl = draft.remoteUrl.trim();
      await persist(3, "draft", remoteUrl ? { sourceKind: "gitClone", remoteUrl } : { sourceKind: "existingPath", remoteUrl: "" });
    } else if (step === 3) {
      await persist(4, "draft");
    } else {
      const next = normalizeNodeDraft({ ...draft, setupStep: 4, setupStatus: "ready" });
      const error = validateNodeDraft(next);
      if (error) {
        setLocalError(error);
        return;
      }
      setWorking(true);
      setProgressKey(sessionId ? "deployment.python.saving" : "deployment.python.savingOffline");
      try {
        await onFinish(next);
      } catch {
        // The parent records the user-facing error.
      } finally {
        setWorking(false);
        setProgressKey(null);
      }
    }
  };

  const shownError = localError ?? errorKey;
  const detected = runtimes.map((item) => ({ value: item.binary, label: `${item.version ? `${item.binary} (${item.version})` : item.binary}${item.isDefault ? ` · ${t("deployment.node.defaultTag")}` : ""}` }));
  const nodeOptions = [
    { value: "node", label: t("deployment.node.pathNode") },
    ...(draft.nodeBin !== "node" && !runtimes.some((item) => item.binary === draft.nodeBin) ? [{ value: draft.nodeBin, label: draft.nodeBin }] : []),
    ...detected,
  ];

  return (
    <Dialog open onOpenChange={(open) => { if (!open && !blocked) onClose(); }}>
      <DialogContent
        className="gap-0 p-0 pb-0 sm:max-w-xl sm:pb-0"
        closeDisabled={blocked}
        onPointerDownOutside={(event) => { if (blocked) event.preventDefault(); }}
        onEscapeKeyDown={(event) => { if (blocked) event.preventDefault(); }}
      >
        <DialogHeader className="px-5 py-4">
          <DialogTitle>{title}</DialogTitle>
        </DialogHeader>
        <div className="min-h-0 flex-1 overflow-y-auto px-5 py-4">
          <WizardStepChrome step={step} />
          <div className="grid gap-3">
            {step === 1 && (
              <>
                <Field label={t("deployment.node.name")} value={draft.name} placeholder={t("deployment.node.placeholder.name")} disabled={blocked} onChange={(value) => set("name", value)} />
                <RemotePathField
                  label={t("deployment.node.projectPath")}
                  sessionId={sessionId}
                  value={draft.projectPath}
                  placeholder={t("deployment.node.placeholder.projectPath")}
                  disabled={blocked}
                  onChange={(value) => set("projectPath", value)}
                />
                <p className="text-xs text-[hsl(var(--secondary))]">{t("deployment.node.pathHint")}</p>
              </>
            )}
            {step === 2 && (
              <>
                <p className="text-sm text-[hsl(var(--secondary))]">{t("deployment.python.gitStepHint")}</p>
                <Field label={t("deployment.remoteUrl")} value={draft.remoteUrl} placeholder={t("deployment.python.placeholder.remoteUrl")} disabled={blocked} onChange={(value) => set("remoteUrl", value)} />
                <Field label={t("deployment.branch")} value={draft.branch} placeholder={t("deployment.python.placeholder.branch")} disabled={blocked} onChange={(value) => set("branch", value)} />
                {!sessionId && draft.remoteUrl.trim() !== "" && <p className="text-xs text-amber-600">{t("deployment.connectRequiredAction")}</p>}
              </>
            )}
            {step === 3 && (
              <>
                <SelectField
                  label={t("deployment.node.version")}
                  value={draft.nodeBin}
                  disabled={blocked}
                  options={nodeOptions}
                  onChange={(value) => set("nodeBin", value)}
                />
                <p className="text-xs text-[hsl(var(--secondary))]">{t("deployment.node.versionHint")}</p>
                {runtimes.length === 0 && <p className="text-xs text-amber-600">{t("deployment.node.missingHint")}</p>}
                <SelectField
                  label={t("deployment.node.packageManager")}
                  value={draft.packageManager}
                  disabled={blocked}
                  options={packageManagers.map((manager) => ({ value: manager, label: manager }))}
                  onChange={(value) => set("packageManager", value)}
                />
                <label className="flex items-center gap-2 text-sm">
                  <input type="checkbox" checked={draft.installDeps} disabled={blocked} onChange={(event) => set("installDeps", event.target.checked)} />
                  {t("deployment.node.installDeps")}
                </label>
                <Field label={t("deployment.node.buildScript")} value={draft.buildScript} placeholder={t("deployment.node.placeholder.buildScript")} disabled={blocked} onChange={(value) => set("buildScript", value)} />
                <p className="text-xs text-[hsl(var(--secondary))]">{t("deployment.node.buildHint")}</p>
                <SelectField
                  label={t("deployment.node.startMode")}
                  value={draft.startMode}
                  disabled={blocked}
                  options={startModes.map((mode) => ({ value: mode, label: t(`deployment.node.mode.${mode}`) }))}
                  onChange={(value) => set("startMode", value)}
                />
                {draft.startMode === "script" ? (
                  <Field label={t("deployment.node.script")} value={draft.script} placeholder={t("deployment.node.placeholder.script")} disabled={blocked} onChange={(value) => set("script", value)} />
                ) : (
                  <Field label={t("deployment.node.entryFile")} value={draft.entryFile} placeholder={t("deployment.node.placeholder.entryFile")} disabled={blocked} onChange={(value) => set("entryFile", value)} />
                )}
                <p className="text-xs text-[hsl(var(--secondary))]">{t(`deployment.node.modeHint.${draft.startMode}`)}</p>
              </>
            )}
            {step === 4 && (
              <>
                <div className="grid grid-cols-2 gap-3">
                  <Field label={t("deployment.node.port")} value={draft.port} type="number" disabled={blocked} onChange={(value) => set("port", value)} />
                  <Field label={t("deployment.node.nodeEnv")} value={draft.nodeEnv} disabled={blocked} onChange={(value) => set("nodeEnv", value)} />
                </div>
                <p className="text-xs text-[hsl(var(--secondary))]">{t("deployment.node.portHint")}</p>
                <Field label={t("deployment.node.appArgs")} value={draft.appArgs} placeholder={t("deployment.node.placeholder.appArgs")} disabled={blocked} onChange={(value) => set("appArgs", value)} />
                <label className="flex items-center gap-2 text-sm">
                  <input type="checkbox" checked={draft.autoStart} disabled={blocked} onChange={(event) => set("autoStart", event.target.checked)} />
                  {t("deployment.python.autoStart")}
                </label>
                {systemUsers.length > 0 ? (
                  <SelectField
                    label={t("deployment.python.runUser")}
                    value={draft.runUser}
                    disabled={blocked}
                    options={(systemUsers.includes(draft.runUser) ? systemUsers : [draft.runUser, ...systemUsers]).map((user) => ({ value: user, label: user }))}
                    onChange={(value) => set("runUser", value)}
                  />
                ) : (
                  <Field label={t("deployment.python.runUser")} value={draft.runUser} disabled={blocked} onChange={(value) => set("runUser", value)} />
                )}
                <p className="text-xs text-[hsl(var(--secondary))]">{t("deployment.node.serviceHint")}</p>
                {!sessionId && <p className="text-xs text-amber-600">{t("deployment.python.finishOfflineHint")}</p>}
              </>
            )}
          </div>
          {progressKey && <p className="mt-3 text-sm text-[hsl(var(--secondary))]" aria-live="polite">{t(progressKey)}</p>}
          {shownError && <p className="mt-3 text-sm text-red-500">{t(shownError)}</p>}
        </div>
        <DialogFooter className="px-5 pb-[max(0.75rem,env(safe-area-inset-bottom))] sm:pb-3">
          <Button variant="ghost" disabled={blocked} onClick={onClose}>{t("common.cancel")}</Button>
          {step > 1 && (
            <Button
              variant="secondary"
              className="border border-[hsl(var(--border))] text-[hsl(var(--secondary))] hover:text-[hsl(var(--foreground))]"
              disabled={blocked}
              onClick={() => setStep((step - 1) as Step)}
            >
              {t("deployment.python.back")}
            </Button>
          )}
          <Button disabled={blocked} onClick={() => void goNext()}>
            {step === 4 ? (sessionId ? t("deployment.python.finish") : t("deployment.python.finishOffline")) : t("deployment.python.next")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
