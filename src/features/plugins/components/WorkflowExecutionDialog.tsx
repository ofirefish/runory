import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "../../../components/ui/button";
import { DialogShell } from "../../../components/ui/dialog-shell";
import { Input } from "../../../components/ui/input";
import { Label } from "../../../components/ui/label";
import { pluginWorkflowGetForm } from "../../../lib/tauri/plugins";
import { useWorkflowRunStore } from "../store/workflowStore";

type JsonSchemaProperty = {
  type?: string;
  title?: string;
  description?: string;
  default?: unknown;
  format?: string;
};

type InputSchema = {
  type?: string;
  required?: string[];
  properties?: Record<string, JsonSchemaProperty>;
};

function defaultsFromSchema(schema: InputSchema | null): Record<string, string> {
  const out: Record<string, string> = {};
  if (!schema?.properties) return out;
  for (const [key, prop] of Object.entries(schema.properties)) {
    if (prop.default === undefined || prop.default === null) continue;
    out[key] = String(prop.default);
  }
  return out;
}

function coerceInputs(
  schema: InputSchema | null,
  raw: Record<string, string>,
): Record<string, unknown> {
  const out: Record<string, unknown> = {};
  for (const [key, value] of Object.entries(raw)) {
    const trimmed = value.trim();
    if (!trimmed) continue;
    const type = schema?.properties?.[key]?.type;
    if (type === "integer" || type === "number") {
      const n = Number(trimmed);
      if (!Number.isFinite(n)) continue;
      out[key] = type === "integer" ? Math.trunc(n) : n;
    } else {
      out[key] = trimmed;
    }
  }
  return out;
}

export function WorkflowExecutionDialog({
  open,
  onClose,
  pluginId,
  workflowId,
  hostId,
  sessionId,
}: {
  open: boolean;
  onClose: () => void;
  pluginId: string;
  workflowId: string;
  hostId?: string;
  sessionId?: string;
}) {
  const { t } = useTranslation();
  const { plan, run, steps, busy, error, planWorkflow, executePlan, cancelRun, reset } =
    useWorkflowRunStore();
  const [phase, setPhase] = useState<"idle" | "planned" | "running" | "done">("idle");
  const [schema, setSchema] = useState<InputSchema | null>(null);
  const [formTitle, setFormTitle] = useState<string>("");
  const [values, setValues] = useState<Record<string, string>>({});
  const [formError, setFormError] = useState<string | null>(null);

  useEffect(() => {
    if (!open) return;
    let cancelled = false;
    void (async () => {
      try {
        const form = await pluginWorkflowGetForm(pluginId, workflowId);
        if (cancelled) return;
        const inputSchema = (form.inputSchema ?? null) as InputSchema | null;
        setSchema(inputSchema);
        setFormTitle(typeof form.name === "string" ? form.name : "");
        setValues(defaultsFromSchema(inputSchema));
      } catch {
        if (!cancelled) {
          setSchema(null);
          setValues({});
        }
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [open, pluginId, workflowId]);

  const fields = useMemo(() => {
    if (!schema?.properties) return [] as Array<[string, JsonSchemaProperty]>;
    return Object.entries(schema.properties);
  }, [schema]);

  const required = schema?.required ?? [];

  if (!open) return null;

  const close = () => {
    reset();
    setPhase("idle");
    setFormError(null);
    onClose();
  };

  const planNow = async () => {
    setFormError(null);
    const missing = required.filter((key) => !(values[key]?.trim()));
    if (missing.length > 0) {
      setFormError("missingInputs");
      return;
    }
    try {
      const next = await planWorkflow({
        pluginId,
        workflowId,
        hostId,
        sessionId,
        inputs: coerceInputs(schema, values),
      });
      setPhase(next.requiresApproval ? "planned" : "planned");
    } catch {
      // error already stored on the workflow store
    }
  };

  const executeNow = async () => {
    if (!plan) return;
    setPhase("running");
    try {
      await executePlan(plan.id);
      setPhase("done");
    } catch {
      setPhase("planned");
    }
  };

  const errorMessage = (() => {
    if (formError === "missingInputs") return t("plugins.workflow.missingInputs");
    if (!error) return null;
    if (error === "planFailed" || error === "executeFailed") {
      return t(`plugins.workflow.${error}`);
    }
    return t(`errors.${error}`, { defaultValue: t("plugins.workflow.planFailed") });
  })();

  return (
    <DialogShell title={t("plugins.workflow.title")} onClose={close} size="form">
      <div className="space-y-4 p-4">
        <div className="space-y-1">
          <p className="text-sm text-muted-foreground">
            {pluginId} / {workflowId}
          </p>
          {formTitle ? <p className="text-sm font-medium">{formTitle}</p> : null}
        </div>

        {phase === "idle" && fields.length > 0 && (
          <div className="max-h-72 space-y-3 overflow-y-auto pr-1">
            {fields.map(([key, prop]) => (
              <div key={key} className="space-y-1.5">
                <Label htmlFor={`wf-${key}`}>
                  {prop.title ?? key}
                  {required.includes(key) ? " *" : ""}
                </Label>
                <Input
                  id={`wf-${key}`}
                  type={prop.type === "integer" || prop.type === "number" ? "number" : "text"}
                  value={values[key] ?? ""}
                  placeholder={prop.description}
                  onChange={(event) =>
                    setValues((prev) => ({ ...prev, [key]: event.target.value }))
                  }
                />
              </div>
            ))}
          </div>
        )}

        {phase === "idle" && (
          <Button type="button" disabled={busy} onClick={() => void planNow()}>
            {t("plugins.workflow.plan")}
          </Button>
        )}
        {plan && (
          <div className="rounded-md border border-border p-3 text-sm">
            <p>
              {t("plugins.workflow.risk")}: {plan.risk}
            </p>
            {plan.changeset?.requiresChangeset && (
              <div className="mt-3 space-y-2 border-t border-border pt-3">
                <p className="font-medium">{t("plugins.workflow.changesetTitle")}</p>
                <p className="text-xs text-muted-foreground">{t("plugins.workflow.changesetHint")}</p>
                {plan.changeset.changeSetId && (
                  <p className="font-mono text-[11px] text-muted-foreground">
                    {t("plugins.workflow.changesetBound")}: {plan.changeset.changeSetId.slice(0, 8)}
                    {plan.changeset.changeSetVersion != null
                      ? ` v${plan.changeset.changeSetVersion}`
                      : ""}
                  </p>
                )}
                <ul className="space-y-2 text-xs">
                  {plan.changeset.operations.map((op, index) => (
                    <li key={`${op.kind}-${index}`} className="rounded border border-border/60 px-2 py-1.5">
                      {op.kind === "remoteCommand" ? (
                        <>
                          <p className="font-medium">{op.summary}</p>
                          <p className="mt-1 font-mono text-[11px] text-muted-foreground">
                            {op.privilege === "sudo" ? "sudo " : ""}
                            {op.executable} {op.args.join(" ")}
                          </p>
                        </>
                      ) : (
                        <p>{op.summary}</p>
                      )}
                    </li>
                  ))}
                </ul>
              </div>
            )}
            <ul className="mt-2 list-disc pl-4 text-xs text-muted-foreground">
              {plan.steps.map((step) => (
                <li key={step.stepId}>
                  {step.plan.summary} ({step.plan.risk})
                </li>
              ))}
            </ul>
            {phase !== "done" && (
              <Button type="button" className="mt-3" disabled={busy} onClick={() => void executeNow()}>
                {plan.requiresApproval ? t("plugins.workflow.approveAndRun") : t("plugins.workflow.run")}
              </Button>
            )}
          </div>
        )}
        {(steps.length > 0 || run) && (
          <div className="space-y-2">
            <p className="text-sm font-medium">{t("plugins.workflow.results")}</p>
            {steps.map((step) => (
              <div key={step.stepId} className="rounded border border-border p-2 text-xs">
                <p>
                  {step.stepId} · {step.state}
                </p>
                {step.stdout && (
                  <pre className="mt-1 max-h-40 overflow-auto whitespace-pre-wrap">{step.stdout}</pre>
                )}
                {step.stderr && (
                  <pre className="mt-1 max-h-24 overflow-auto whitespace-pre-wrap text-destructive">
                    {step.stderr}
                  </pre>
                )}
              </div>
            ))}
            {run?.runId && phase === "running" && (
              <Button type="button" variant="secondary" onClick={() => void cancelRun(run.runId)}>
                {t("plugins.workflow.cancel")}
              </Button>
            )}
          </div>
        )}
        {errorMessage && <p className="text-sm text-destructive">{errorMessage}</p>}
      </div>
    </DialogShell>
  );
}
