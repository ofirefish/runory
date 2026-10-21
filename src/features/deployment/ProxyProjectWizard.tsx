import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "../../components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "../../components/ui/dialog";
import type { DeploymentSetupStatus, ProxyHostHeader } from "../../types/infrastructure";
import { DeploymentInputField as Field, DeploymentSelectField as SelectField } from "./DeploymentFields";
import { normalizeProxyDraft, validateProxyDraft, validateProxyStep, type ProxyProjectDraft } from "./proxy-project-draft";

type Step = 1 | 2 | 3;
const wizardSteps = [1, 2, 3] as const;
const hostHeaders = ["original", "target"] as const satisfies readonly ProxyHostHeader[];

function WizardStepChrome({ step }: { step: Step }) {
  const { t } = useTranslation();
  const labels = [
    t("deployment.proxy.step.target"),
    t("deployment.proxy.step.site"),
    t("deployment.proxy.step.options"),
  ] as const;
  return (
    <ol className="mb-4 flex items-center gap-1 text-xs" aria-label={t("deployment.python.stepProgress", { current: step, total: 3 })}>
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

export function ProxyProjectWizard({
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
  initial: ProxyProjectDraft;
  sessionId: string | null;
  busy?: boolean;
  errorKey?: string | null;
  onClose: () => void;
  onSave: (draft: ProxyProjectDraft) => Promise<ProxyProjectDraft | void>;
  onFinish: (draft: ProxyProjectDraft) => Promise<void>;
}) {
  const { t } = useTranslation();
  const [draft, setDraft] = useState(initial);
  const [step, setStep] = useState<Step>(initial.setupStep);
  const [localError, setLocalError] = useState<string | null>(null);
  const [working, setWorking] = useState(false);
  const [progressKey, setProgressKey] = useState<string | null>(null);
  const blocked = busy || working;
  const set = <K extends keyof ProxyProjectDraft>(key: K, value: ProxyProjectDraft[K]) => {
    setDraft((previous) => ({ ...previous, [key]: value }));
    setLocalError(null);
  };

  const persist = async (nextStep: Step, status: DeploymentSetupStatus) => {
    const next = normalizeProxyDraft({ ...draft, setupStep: nextStep, setupStatus: status });
    const error = validateProxyStep(next, step);
    if (error) {
      setLocalError(error);
      return false;
    }
    setWorking(true);
    setLocalError(null);
    setProgressKey("deployment.python.saving");
    try {
      let saved: ProxyProjectDraft | void;
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
      await persist(3, "draft");
    } else {
      const next = normalizeProxyDraft({ ...draft, setupStep: 3, setupStatus: "ready" });
      const error = validateProxyDraft(next);
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
                <Field label={t("deployment.proxy.name")} value={draft.name} placeholder={t("deployment.proxy.placeholder.name")} disabled={blocked} onChange={(value) => set("name", value)} />
                <Field label={t("deployment.proxy.targetUrl")} value={draft.targetUrl} placeholder={t("deployment.proxy.placeholder.targetUrl")} disabled={blocked} onChange={(value) => set("targetUrl", value)} />
                <p className="text-xs text-[hsl(var(--secondary))]">{t("deployment.proxy.targetUrlHint")}</p>
              </>
            )}
            {step === 2 && (
              <>
                <div className="grid grid-cols-2 gap-3">
                  <Field label={t("deployment.proxy.port")} value={draft.port} type="number" disabled={blocked} onChange={(value) => set("port", value)} />
                  <Field label={t("deployment.proxy.serverName")} value={draft.serverName} placeholder={t("deployment.proxy.placeholder.serverName")} disabled={blocked} onChange={(value) => set("serverName", value)} />
                </div>
                <p className="text-xs text-[hsl(var(--secondary))]">{t("deployment.proxy.siteHint")}</p>
              </>
            )}
            {step === 3 && (
              <>
                <SelectField
                  label={t("deployment.proxy.hostHeader")}
                  value={draft.hostHeader}
                  disabled={blocked}
                  options={hostHeaders.map((mode) => ({ value: mode, label: t(`deployment.proxy.host.${mode}`) }))}
                  onChange={(value) => set("hostHeader", value)}
                />
                <p className="text-xs text-[hsl(var(--secondary))]">{t(`deployment.proxy.hostHint.${draft.hostHeader}`)}</p>
                <div className="grid grid-cols-2 gap-3">
                  <Field label={t("deployment.proxy.maxBody")} value={draft.maxBodyMb} type="number" disabled={blocked} onChange={(value) => set("maxBodyMb", value)} />
                  <Field label={t("deployment.proxy.timeout")} value={draft.timeoutSeconds} type="number" disabled={blocked} onChange={(value) => set("timeoutSeconds", value)} />
                </div>
                <label className="flex items-center gap-2 text-sm">
                  <input type="checkbox" checked={draft.websocket} disabled={blocked} onChange={(event) => set("websocket", event.target.checked)} />
                  {t("deployment.proxy.websocket")}
                </label>
                <p className="text-xs text-[hsl(var(--secondary))]">{t("deployment.proxy.serviceHint")}</p>
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
            {step === 3 ? (sessionId ? t("deployment.python.finish") : t("deployment.python.finishOffline")) : t("deployment.python.next")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
