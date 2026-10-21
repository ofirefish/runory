import { useEffect, useState } from "react";
import { deploymentConflictErrorKey } from "./deployment-app-errors";
import { useTranslation } from "react-i18next";
import { Button } from "../../components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "../../components/ui/dialog";
import { Tabs, TabsList, TabsTrigger } from "../../components/ui/tabs";
import { appErrorCode } from "../../lib/app-error";
import {
  listPythonRuntimes,
  listSystemUsers,
  setupGit,
} from "../../lib/tauri/infrastructure";
import type {
  DeploymentSetupStatus,
  PythonProtocol,
  PythonStartupMethod,
} from "../../types/infrastructure";
import { normalizePythonDraft, validatePythonDraft, validatePythonStep, type PythonProjectDraft } from "./python-project-draft";
import { RemotePathField } from "../files/RemotePathField";
import { DeploymentInputField as Field, DeploymentSelectField as SelectField } from "./DeploymentFields";

const startupMethods = ["command", "uwsgi", "gunicorn"] as const satisfies readonly PythonStartupMethod[];
const protocols = ["wsgi", "asgi"] as const satisfies readonly PythonProtocol[];
const wizardSteps = [1, 2, 3, 4] as const;

function isStartupMethod(value: string): value is PythonStartupMethod {
  return startupMethods.some((method) => method === value);
}

function isProtocol(value: string): value is PythonProtocol {
  return protocols.some((protocol) => protocol === value);
}

function WizardStepChrome({ step }: { step: 1 | 2 | 3 | 4 }) {
  const { t } = useTranslation();
  const labels = [
    t("deployment.python.step.project"),
    t("deployment.python.step.git"),
    t("deployment.python.step.environment"),
    t("deployment.python.step.startup"),
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
            {index < wizardSteps.length - 1 && (
              <span className="mx-1 hidden h-px flex-1 bg-[hsl(var(--border))] sm:block" aria-hidden />
            )}
          </li>
        );
      })}
    </ol>
  );
}

export function PythonProjectWizard({
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
  initial: PythonProjectDraft;
  sessionId: string | null;
  busy?: boolean;
  errorKey?: string | null;
  onClose: () => void;
  onSave: (draft: PythonProjectDraft) => Promise<PythonProjectDraft | void>;
  onFinish: (draft: PythonProjectDraft) => Promise<void>;
}) {
  const { t } = useTranslation();
  const [draft, setDraft] = useState(initial);
  const [step, setStep] = useState<1 | 2 | 3 | 4>(initial.setupStep);
  const [localError, setLocalError] = useState<string | null>(null);
  const [working, setWorking] = useState(false);
  const [progressKey, setProgressKey] = useState<string | null>(null);
  const [systemUsers, setSystemUsers] = useState<string[]>([]);
  const [runtimes, setRuntimes] = useState<string[]>([]);
  const blocked = busy || working;
  const set = <K extends keyof PythonProjectDraft>(key: K, value: PythonProjectDraft[K]) => {
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
    void listPythonRuntimes(sessionId)
      .then((items) => { if (!cancelled) setRuntimes(items.map((item) => item.binary)); })
      .catch(() => { if (!cancelled) setRuntimes([]); });
    return () => { cancelled = true; };
  }, [sessionId, step]);

  const mapRemoteError = (caught: unknown, fallback: string) => {
    const code = appErrorCode(caught);
    const specificError = deploymentConflictErrorKey(code);
    if (specificError) return specificError;
    if (code === "UNSUPPORTED_REMOTE") return "deployment.python.unsupported";
    if (code === "INVALID_OPERATION") return "deployment.validation.invalid";
    return fallback;
  };

  const persist = async (
    nextStep: 1 | 2 | 3 | 4,
    status: DeploymentSetupStatus,
    patch?: Partial<PythonProjectDraft>,
  ) => {
    const currentStep = step;
    const next = normalizePythonDraft({
      ...draft,
      ...patch,
      setupStep: nextStep,
      setupStatus: status,
    });
    const error = status === "ready" ? validatePythonDraft(next) : validatePythonStep(next, currentStep);
    if (error) {
      setLocalError(error);
      return false;
    }

    setWorking(true);
    setLocalError(null);
    try {
      if (currentStep === 2 && next.sourceKind === "gitClone") {
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

      // Environment prepare runs during provision (finish / Deploy), after startup
      // method is known, so gunicorn/uwsgi/uvicorn packages match the final choice.

      setProgressKey("deployment.python.saving");
      let saved: PythonProjectDraft | void;
      try {
        saved = await onSave(next);
      } catch {
        return false;
      }
      const merged = saved ?? next;
      setDraft(merged);
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
      return;
    }
    if (step === 2) {
      const remoteUrl = draft.remoteUrl.trim();
      if (remoteUrl) {
        await persist(3, "draft", { sourceKind: "gitClone", remoteUrl });
      } else {
        await persist(3, "draft", { sourceKind: "existingPath", remoteUrl: "" });
      }
      return;
    }
    if (step === 3) {
      await persist(4, "draft");
      return;
    }
    const next = normalizePythonDraft({
      ...draft,
      protocol: draft.startup === "uwsgi" ? "wsgi" : draft.protocol,
      setupStep: 4,
      setupStatus: "ready",
    });
    const error = validatePythonDraft(next);
    if (error) {
      setLocalError(error);
      return;
    }
    setWorking(true);
    setProgressKey(sessionId ? "deployment.python.saving" : "deployment.python.savingOffline");
    try {
      await onFinish(next);
    } catch {
      // Parent records the user-facing error.
    } finally {
      setWorking(false);
      setProgressKey(null);
    }
  };

  const setStartup = (value: PythonStartupMethod) => {
    setDraft((previous) => ({
      ...previous,
      startup: value,
      protocol: value === "uwsgi" ? "wsgi" : previous.protocol,
      application: value === "uwsgi" && previous.application.includes(":")
        ? previous.application.split(":").at(-1) ?? "application"
        : previous.application,
    }));
    setLocalError(null);
  };

  const shownError = localError ?? errorKey;
  const showProtocol = draft.startup === "gunicorn" || draft.startup === "uwsgi";

  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (!open && !blocked) onClose();
      }}
    >
      <DialogContent
        className="gap-0 p-0 pb-0 sm:max-w-xl sm:pb-0"
        closeDisabled={blocked}
        onPointerDownOutside={(event) => {
          if (blocked) event.preventDefault();
        }}
        onEscapeKeyDown={(event) => {
          if (blocked) event.preventDefault();
        }}
      >
        <DialogHeader className="px-5 py-4">
          <DialogTitle>{title}</DialogTitle>
        </DialogHeader>

        <div className="min-h-0 flex-1 overflow-y-auto px-5 py-4">
          <WizardStepChrome step={step} />
          <div className="grid gap-3">
            {step === 1 && (
              <>
                <Field
                  label={t("deployment.python.name")}
                  value={draft.name}
                  placeholder={t("deployment.python.placeholder.name")}
                  disabled={blocked}
                  onChange={(value) => set("name", value)}
                />
                <RemotePathField
                  label={t("deployment.python.projectPath")}
                  sessionId={sessionId}
                  value={draft.projectPath}
                  placeholder={t("deployment.python.placeholder.projectPath")}
                  disabled={blocked}
                  onChange={(value) => set("projectPath", value)}
                />
              </>
            )}
            {step === 2 && (
              <>
                <p className="text-sm text-[hsl(var(--secondary))]">{t("deployment.python.gitStepHint")}</p>
                <Field
                  label={t("deployment.remoteUrl")}
                  value={draft.remoteUrl}
                  placeholder={t("deployment.python.placeholder.remoteUrl")}
                  disabled={blocked}
                  onChange={(value) => set("remoteUrl", value)}
                />
                <Field
                  label={t("deployment.branch")}
                  value={draft.branch}
                  placeholder={t("deployment.python.placeholder.branch")}
                  disabled={blocked}
                  onChange={(value) => set("branch", value)}
                />
                {!sessionId && draft.remoteUrl.trim() !== "" && (
                  <p className="text-xs text-amber-600">{t("deployment.connectRequiredAction")}</p>
                )}
              </>
            )}
            {step === 3 && (
              <>
                {runtimes.length > 0 ? (
                  <SelectField
                    label={t("deployment.python.environment")}
                    value={draft.pythonBin}
                    disabled={blocked}
                    options={runtimes.map((binary) => ({ value: binary, label: binary }))}
                    onChange={(value) => set("pythonBin", value)}
                  />
                ) : (
                  <Field
                    label={t("deployment.python.environment")}
                    value={draft.pythonBin}
                    disabled={blocked}
                    onChange={(value) => set("pythonBin", value)}
                  />
                )}
                <label className="flex items-center gap-2 text-sm">
                  <input
                    type="checkbox"
                    checked={draft.useExistingVenv}
                    disabled={blocked}
                    onChange={(event) => set("useExistingVenv", event.target.checked)}
                  />
                  {t("deployment.python.useExistingVenv")}
                </label>
                {draft.useExistingVenv ? (
                  <RemotePathField
                    label={t("deployment.python.venvPath")}
                    sessionId={sessionId}
                    value={draft.venvPath}
                    placeholder={t("deployment.python.placeholder.venvPath")}
                    disabled={blocked}
                    onChange={(value) => set("venvPath", value)}
                  />
                ) : (
                  <p className="text-xs text-[hsl(var(--secondary))]">{t("deployment.python.defaultVenv")}</p>
                )}
                <Field
                  label={t("deployment.python.requirementsPath")}
                  value={draft.requirementsPath}
                  placeholder={t("deployment.python.placeholder.requirementsPath")}
                  disabled={blocked}
                  onChange={(value) => set("requirementsPath", value)}
                />
                <p className="text-xs text-[hsl(var(--secondary))]">{t("deployment.python.envPrepareHint")}</p>
              </>
            )}
            {step === 4 && (
              <>
                <Tabs
                  value={draft.startup}
                  onValueChange={(value) => {
                    if (!blocked && isStartupMethod(value)) setStartup(value);
                  }}
                >
                  <TabsList>
                    {startupMethods.map((method) => (
                      <TabsTrigger key={method} value={method} disabled={blocked}>
                        {t(`deployment.python.startup.${method}`)}
                      </TabsTrigger>
                    ))}
                  </TabsList>
                </Tabs>
                <p className="text-xs text-[hsl(var(--secondary))]">{t(`deployment.python.help.${draft.startup}`)}</p>
                {showProtocol && (
                  <>
                    <Tabs
                      value={draft.startup === "uwsgi" ? "wsgi" : draft.protocol}
                      onValueChange={(value) => {
                        if (blocked || draft.startup === "uwsgi") return;
                        if (isProtocol(value)) set("protocol", value);
                      }}
                    >
                      <TabsList>
                        {protocols.map((protocol) => (
                          <TabsTrigger
                            key={protocol}
                            value={protocol}
                            disabled={blocked || (draft.startup === "uwsgi" && protocol === "asgi")}
                          >
                            {t(`deployment.python.protocol.${protocol}`)}
                          </TabsTrigger>
                        ))}
                      </TabsList>
                    </Tabs>
                    <p className="text-xs text-[hsl(var(--secondary))]">
                      {t(`deployment.python.help.${draft.startup === "uwsgi" ? "wsgi" : draft.protocol}`)}
                    </p>
                    {draft.startup === "uwsgi" && (
                      <p className="text-xs text-amber-600">{t("deployment.python.help.uwsgi")}</p>
                    )}
                  </>
                )}
                {draft.startup !== "command" && (
                  <Field
                    label={t("deployment.python.port")}
                    value={draft.port}
                    placeholder={t("deployment.python.placeholder.port")}
                    disabled={blocked}
                    onChange={(value) => set("port", value)}
                  />
                )}
                {draft.startup === "command" && (
                  <Field
                    label={t("deployment.python.startupCommand")}
                    value={draft.startupCommand}
                    placeholder={t("deployment.python.placeholder.startupCommand")}
                    disabled={blocked}
                    onChange={(value) => set("startupCommand", value)}
                  />
                )}
                {draft.startup !== "command" && (
                  <Field
                    label={t("deployment.python.entryFile")}
                    value={draft.entryFile}
                    placeholder={t("deployment.python.placeholder.entryFile")}
                    disabled={blocked}
                    onChange={(value) => set("entryFile", value)}
                  />
                )}
                {draft.startup !== "command" && (
                  <Field
                    label={t(draft.startup === "uwsgi" ? "deployment.python.callable" : "deployment.python.application")}
                    value={draft.application}
                    placeholder={draft.startup === "uwsgi" ? "application" : t("deployment.python.placeholder.application")}
                    disabled={blocked}
                    onChange={(value) => set("application", value)}
                  />
                )}
                {draft.startup !== "command" && (
                  <div className="grid grid-cols-2 gap-3">
                    <Field label={t("deployment.python.processes")} value={draft.processes} type="number" disabled={blocked} onChange={(value) => set("processes", value)} />
                    <Field label={t("deployment.python.threads")} value={draft.threads} type="number" disabled={blocked || (draft.startup === "gunicorn" && draft.protocol === "asgi")} onChange={(value) => set("threads", value)} />
                  </div>
                )}
                <label className="flex items-center gap-2 text-sm">
                  <input type="checkbox" checked={draft.autoStart} disabled={blocked} onChange={(event) => set("autoStart", event.target.checked)} />
                  {t("deployment.python.autoStart")}
                </label>
                <p className="text-xs text-[hsl(var(--secondary))]">{t("deployment.python.serviceHint")}</p>
                {systemUsers.length > 0 ? (
                  <SelectField
                    label={t("deployment.python.runUser")}
                    value={draft.runUser}
                    disabled={blocked}
                    options={systemUsers.map((user) => ({ value: user, label: user }))}
                    onChange={(value) => set("runUser", value)}
                  />
                ) : (
                  <Field
                    label={t("deployment.python.runUser")}
                    value={draft.runUser}
                    disabled={blocked}
                    onChange={(value) => set("runUser", value)}
                  />
                )}
                {!sessionId && (
                  <p className="text-xs text-amber-600">{t("deployment.python.finishOfflineHint")}</p>
                )}
              </>
            )}
          </div>
          {progressKey && (
            <p className="mt-3 text-sm text-[hsl(var(--secondary))]" aria-live="polite">{t(progressKey)}</p>
          )}
          {shownError && <p className="mt-3 text-sm text-red-500">{t(shownError)}</p>}
        </div>

        <DialogFooter className="px-5 pb-[max(0.75rem,env(safe-area-inset-bottom))] sm:pb-3">
          <Button variant="ghost" disabled={blocked} onClick={onClose}>{t("common.cancel")}</Button>
          {step > 1 && (
            <Button
              variant="secondary"
              className="border border-[hsl(var(--border))] text-[hsl(var(--secondary))] hover:text-[hsl(var(--foreground))]"
              disabled={blocked}
              onClick={() => setStep((step - 1) as 1 | 2 | 3 | 4)}
            >
              {t("deployment.python.back")}
            </Button>
          )}
          <Button disabled={blocked} onClick={() => void goNext()}>
            {step === 4
              ? (sessionId ? t("deployment.python.finish") : t("deployment.python.finishOffline"))
              : t("deployment.python.next")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
