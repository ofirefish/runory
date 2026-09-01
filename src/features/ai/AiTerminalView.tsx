import { AlertTriangle, BrainCircuit, Clipboard, ShieldCheck, Sparkles, Stethoscope, WandSparkles, Wrench } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "../../components/ui/button";
import { DialogShell } from "../../components/ui/dialog-shell";
import { diagnoseOutput, explainCommand, generateCommand, proposeFix } from "../../lib/tauri/ai";
import type { AiAssistantResponse, AiCommandProposal, AiRisk, AiTask } from "../../types/ai";
import { aiTaskMayUseSessionContext, aiTaskNeedsInput } from "./ai-task";

const tasks: { id: AiTask; icon: typeof Sparkles }[] = [
  { id: "explain-command", icon: BrainCircuit },
  { id: "generate-command", icon: WandSparkles },
  { id: "diagnose-output", icon: Stethoscope },
  { id: "propose-fix", icon: Wrench },
];

const riskClass: Record<AiRisk, string> = {
  low: "border-emerald-500/30 bg-emerald-500/10 text-emerald-600",
  medium: "border-amber-500/30 bg-amber-500/10 text-amber-600",
  high: "border-orange-500/30 bg-orange-500/10 text-orange-600",
  critical: "border-red-500/30 bg-red-500/10 text-red-600",
};

export function AiTerminalView({ sessionId, onInsertCommand }: { sessionId: string | null; onInsertCommand: (command: string) => void }) {
  const { t } = useTranslation();
  const [task, setTask] = useState<AiTask>("explain-command");
  const [input, setInput] = useState("");
  const [result, setResult] = useState<AiAssistantResponse | null>(null);
  const [pending, setPending] = useState<AiCommandProposal | null>(null);
  const [busy, setBusy] = useState(false);
  const [failed, setFailed] = useState(false);

  const run = async () => {
    if (!sessionId || (aiTaskNeedsInput(task) && !input.trim())) return;
    setBusy(true);
    setFailed(false);
    try {
      const response = task === "explain-command"
        ? await explainCommand(sessionId, input)
        : task === "generate-command"
          ? await generateCommand(sessionId, input)
          : task === "diagnose-output"
            ? await diagnoseOutput(sessionId, input)
            : await proposeFix(sessionId, input);
      setResult(response);
    } catch {
      setFailed(true);
    } finally {
      setBusy(false);
    }
  };

  if (!sessionId) return <div className="grid h-full place-items-center p-6 text-sm text-[hsl(var(--muted))]">{t("ai.connectRequired")}</div>;

  return <div className="h-full overflow-y-auto bg-[hsl(var(--background))] p-4 md:p-6">
    <div className="mx-auto grid max-w-5xl gap-5">
      <header className="flex items-start gap-3"><div className="rounded-lg bg-blue-500/10 p-2 text-blue-500"><Sparkles size={20} /></div><div><h2 className="font-semibold">{t("ai.title")}</h2><p className="mt-1 text-sm text-[hsl(var(--secondary))]">{t("ai.localFirst")}</p></div></header>
      <div className="grid grid-cols-2 gap-2 md:grid-cols-4">
        {tasks.map(({ id, icon: Icon }) => <button key={id} type="button" className={`rounded-lg border p-3 text-left text-sm transition-colors ${task === id ? "border-blue-500 bg-blue-500/10" : "hover:bg-[hsl(var(--elevated))]"}`} onClick={() => { setTask(id); setResult(null); setFailed(false); }}><Icon className="mb-2" size={17} /><span className="font-medium">{t(`ai.task.${id}`)}</span><span className="mt-1 block text-xs text-[hsl(var(--muted))]">{t(`ai.description.${id}`)}</span></button>)}
      </div>
      <section className="rounded-xl border bg-[hsl(var(--surface))] p-4">
        <label className="grid gap-2 text-sm"><span className="font-medium">{t("ai.inputLabel")}</span><textarea className="min-h-32 resize-y rounded-md border bg-transparent p-3 font-mono text-sm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blue-500" value={input} onChange={(event) => setInput(event.target.value)} placeholder={t(`ai.placeholder.${task}`)} spellCheck={false} /></label>
        {aiTaskMayUseSessionContext(task) && <p className="mt-2 text-xs text-[hsl(var(--muted))]">{t("ai.contextHint")}</p>}
        {failed && <p className="mt-3 text-sm text-red-500">{t("ai.error")}</p>}
        <Button className="mt-4" disabled={busy || (aiTaskNeedsInput(task) && !input.trim())} onClick={() => void run()}><Sparkles size={16} />{busy ? t("common.loading") : t("ai.run")}</Button>
      </section>
      {result && <section className="grid gap-4 rounded-xl border bg-[hsl(var(--surface))] p-4">
        <div className="flex flex-wrap items-center gap-2 text-xs"><span className={`rounded-full border px-2 py-1 ${riskClass[result.risk]}`}>{t(`ai.risk.${result.risk}`)}</span><span className="rounded-full border px-2 py-1">{t(`ai.purpose.${result.purpose}`)}</span><span className="text-[hsl(var(--muted))]">{t("ai.provider")}: {result.provider}</span>{result.contextUsed && <span className="text-blue-500">{t("ai.contextUsed")}</span>}</div>
        {result.diagnosis && <div><h3 className="text-sm font-medium">{t("ai.diagnosisTitle")}</h3><p className="mt-1 text-sm text-[hsl(var(--secondary))]">{t(`ai.diagnosis.${result.diagnosis}`)}</p></div>}
        {result.signals.length > 0 && <div><h3 className="text-sm font-medium">{t("ai.signals")}</h3><div className="mt-2 flex flex-wrap gap-2">{result.signals.map((signal) => <span key={signal} className="rounded bg-[hsl(var(--elevated))] px-2 py-1 text-xs">{t(`ai.signal.${signal}`)}</span>)}</div></div>}
        <div><h3 className="text-sm font-medium">{t("ai.proposals")}</h3>{result.proposals.length === 0 ? <p className="mt-2 text-sm text-[hsl(var(--muted))]">{t("ai.noProposal")}</p> : <div className="mt-2 grid gap-2">{result.proposals.map((proposal) => <div key={proposal.command} className="flex flex-col gap-3 rounded-lg border p-3 sm:flex-row sm:items-center"><code className="min-w-0 flex-1 overflow-x-auto whitespace-pre text-sm">{proposal.command}</code><div className="flex shrink-0 gap-2"><Button variant="ghost" size="sm" onClick={() => void navigator.clipboard?.writeText(proposal.command)}><Clipboard size={14} />{t("ai.copy")}</Button><Button size="sm" onClick={() => setPending(proposal)}><ShieldCheck size={14} />{t("ai.reviewInsert")}</Button></div></div>)}</div>}</div>
      </section>}
    </div>
    {pending && <DialogShell title={t("ai.confirmTitle")} onClose={() => setPending(null)}><div className="flex gap-3 rounded-lg border border-amber-500/30 bg-amber-500/10 p-3"><AlertTriangle className="shrink-0 text-amber-600" size={20} /><div><p className="text-sm">{t("ai.confirmDescription")}</p><code className="mt-3 block overflow-x-auto whitespace-pre rounded bg-slate-950 p-3 text-xs text-slate-100">{pending.command}</code><p className="mt-2 text-xs text-[hsl(var(--muted))]">{t("ai.insertHint")}</p></div></div><div className="mt-5 flex justify-end gap-2"><Button variant="ghost" onClick={() => setPending(null)}>{t("common.cancel")}</Button><Button onClick={() => { onInsertCommand(pending.command); setPending(null); }}>{t("ai.insert")}</Button></div></DialogShell>}
  </div>;
}
