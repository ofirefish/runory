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
import { listJavaRuntimes, listSystemUsers, setupGit } from "../../lib/tauri/infrastructure";
import type { DeploymentSetupStatus, JavaBuildTool, JavaRuntime } from "../../types/infrastructure";
import { RemotePathField } from "../files/RemotePathField";
import { deploymentConflictErrorKey } from "./deployment-app-errors";
import { DeploymentInputField as Field, DeploymentSelectField as SelectField } from "./DeploymentFields";
import { normalizeJavaDraft, validateJavaDraft, validateJavaStep, type JavaProjectDraft } from "./java-project-draft";

type Step = 1 | 2 | 3 | 4;
const wizardSteps = [1, 2, 3, 4] as const;
const buildTools = ["none", "maven", "gradle"] as const satisfies readonly JavaBuildTool[];

function WizardStepChrome({ step }: { step: Step }) {
  const { t } = useTranslation();
  const labels = [
    t("deployment.java.step.project"),
    t("deployment.java.step.git"),
    t("deployment.java.step.environment"),
    t("deployment.java.step.run"),
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

export function JavaProjectWizard({
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
  initial: JavaProjectDraft;
  sessionId: string | null;
  busy?: boolean;
  errorKey?: string | null;
  onClose: () => void;
  onSave: (draft: JavaProjectDraft) => Promise<JavaProjectDraft | void>;
  onFinish: (draft: JavaProjectDraft) => Promise<void>;
}) {
  const { t } = useTranslation();
  const [draft, setDraft] = useState(initial);
  const [step, setStep] = useState<Step>(initial.setupStep);
  const [localError, setLocalError] = useState<string | null>(null);
  const [working, setWorking] = useState(false);
  const [progressKey, setProgressKey] = useState<string | null>(null);
  const [systemUsers, setSystemUsers] = useState<string[]>([]);
  const [runtimes, setRuntimes] = useState<JavaRuntime[]>([]);
  const blocked = busy || working;
  const set = <K extends keyof JavaProjectDraft>(key: K, value: JavaProjectDraft[K]) => {
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
    void listJavaRuntimes(sessionId)
      .then((items) => { if (!cancelled) setRuntimes(items); })
      .catch(() => { if (!cancelled) setRuntimes([]); });
    return () => { cancelled = true; };
  }, [sessionId, step]);

  const mapRemoteError = (caught: unknown, fallback: string) => {
    const code = appErrorCode(caught);
    const specific = deploymentConflictErrorKey(code);
    if (specific) return specific;
    if (code === "UNSUPPORTED_REMOTE") return "deployment.java.unsupported";
    if (code === "INVALID_OPERATION") return "deployment.validation.invalid";
    return fallback;
  };

  const persist = async (nextStep: Step, status: DeploymentSetupStatus, patch?: Partial<JavaProjectDraft>) => {
    const next = normalizeJavaDraft({ ...draft, ...patch, setupStep: nextStep, setupStatus: status });
    const error = validateJavaStep(next, step);
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
      let saved: JavaProjectDraft | void;
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
      const next = normalizeJavaDraft({ ...draft, setupStep: 4, setupStatus: "ready" });
      const error = validateJavaDraft(next);
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
  const detected = runtimes.map((item) => ({ value: item.binary, label: `${item.version ? `${item.binary} (${item.version})` : item.binary}${item.isDefault ? ` · ${t("deployment.java.jdk.default")}` : ""}` }));
  const javaOptions = [
    { value: "java", label: t("deployment.java.pathJava") },
    ...(draft.javaBin !== "java" && !runtimes.some((item) => item.binary === draft.javaBin) ? [{ value: draft.javaBin, label: draft.javaBin }] : []),
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
                <Field label={t("deployment.java.name")} value={draft.name} placeholder={t("deployment.java.placeholder.name")} disabled={blocked} onChange={(value) => set("name", value)} />
                <RemotePathField
                  label={t("deployment.java.projectPath")}
                  sessionId={sessionId}
                  value={draft.projectPath}
                  placeholder={t("deployment.java.placeholder.projectPath")}
                  disabled={blocked}
                  onChange={(value) => set("projectPath", value)}
                />
                <p className="text-xs text-[hsl(var(--secondary))]">{t("deployment.java.pathHint")}</p>
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
                  label={t("deployment.java.version")}
                  value={draft.javaBin}
                  disabled={blocked}
                  options={javaOptions}
                  onChange={(value) => set("javaBin", value)}
                />
                <p className="text-xs text-[hsl(var(--secondary))]">{t("deployment.java.versionHint")}</p>
                {runtimes.length === 0 && <p className="text-xs text-amber-600">{t("deployment.java.jdk.missingHint")}</p>}
                <Field label={t("deployment.java.jarPath")} value={draft.jarPath} placeholder={t("deployment.java.placeholder.jarPath")} disabled={blocked} onChange={(value) => set("jarPath", value)} />
                <p className="text-xs text-[hsl(var(--secondary))]">{t("deployment.java.jarPathHint")}</p>
                <SelectField
                  label={t("deployment.java.buildTool")}
                  value={draft.buildTool}
                  disabled={blocked}
                  options={buildTools.map((tool) => ({ value: tool, label: t(`deployment.java.build.${tool}`) }))}
                  onChange={(value) => set("buildTool", value)}
                />
                <p className="text-xs text-[hsl(var(--secondary))]">{t("deployment.java.buildHint")}</p>
              </>
            )}
            {step === 4 && (
              <>
                <div className="grid grid-cols-3 gap-3">
                  <Field label={t("deployment.java.port")} value={draft.port} type="number" disabled={blocked} onChange={(value) => set("port", value)} />
                  <Field label={t("deployment.java.xms")} value={draft.xmsMb} type="number" disabled={blocked} onChange={(value) => set("xmsMb", value)} />
                  <Field label={t("deployment.java.xmx")} value={draft.xmxMb} type="number" disabled={blocked} onChange={(value) => set("xmxMb", value)} />
                </div>
                <Field label={t("deployment.java.jvmArgs")} value={draft.jvmArgs} placeholder={t("deployment.java.placeholder.jvmArgs")} disabled={blocked} onChange={(value) => set("jvmArgs", value)} />
                <Field label={t("deployment.java.appArgs")} value={draft.appArgs} placeholder={t("deployment.java.placeholder.appArgs")} disabled={blocked} onChange={(value) => set("appArgs", value)} />
                <p className="text-xs text-[hsl(var(--secondary))]">{t("deployment.java.argsHint")}</p>
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
                <p className="text-xs text-[hsl(var(--secondary))]">{t("deployment.java.serviceHint")}</p>
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
