import { CheckCircle2 } from "lucide-react";
import { useTranslation } from "react-i18next";
import type { AgentRun } from "../../types/agentic";

export function RunResult({ run }: { run: AgentRun }) {
  const { t } = useTranslation();
  return <section className="grid gap-4 rounded-xl border p-4">
    <div className="flex items-center gap-2"><CheckCircle2 size={18} /><h3 className="font-medium">{t("agentic.timeline")}</h3><span className="text-xs">{run.state} · {run.usedToolCalls}/{run.maxToolCalls}</span></div>
    <p className="text-xs text-muted-foreground">{t("agentic.metrics", { tokens: run.metrics.inputTokens + run.metrics.outputTokens, cache: run.metrics.cacheHits, duration: run.metrics.durationMs })}</p>
    <div className="grid gap-2">{run.activities.map((item) => <div key={item.invocationId} className="flex justify-between rounded border p-2 text-sm"><span>{item.toolName}</span><span className={item.success ? "text-emerald-600" : "text-red-500"}>{item.success ? t("agent.auditSucceeded") : item.errorCode}</span></div>)}</div>
    {run.diagnosis && <div className="rounded border p-3"><h4 className="font-medium">{t("agentic.diagnosis")}</h4><p className="mt-1 text-sm">{run.diagnosis.rootCauseCode} · {Math.round(run.diagnosis.confidence * 100)}%</p><p className="mt-1 text-sm">{run.diagnosis.recommendedActionCode}</p></div>}
    <details><summary className="cursor-pointer text-sm">{t("agentic.evidence", { count: run.evidence.length + run.externalEvidence.length })}</summary><pre className="mt-2 overflow-auto rounded bg-slate-950 p-3 text-xs text-slate-100">{JSON.stringify([...run.evidence, ...run.externalEvidence], null, 2)}</pre></details>
  </section>;
}
