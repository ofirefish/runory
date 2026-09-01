import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "../../components/ui/button";
import { Input } from "../../components/ui/input";
import { clearAgentModelApiKey, configureAgentModel, getAgentModel, testAgentModel } from "../../lib/tauri/agentic";
import type { ModelProviderKind, ModelProviderStatus } from "../../types/agentic";

const presets: Record<Exclude<ModelProviderKind, "local" | "open-ai-compatible">, { baseUrl: string; model: string }> = {
  "deep-seek": { baseUrl: "https://api.deepseek.com", model: "deepseek-v4-pro" },
  glm: { baseUrl: "https://open.bigmodel.cn/api/paas/v4", model: "glm-5.2" },
};

export function AgentModelSettings() {
  const { t } = useTranslation();
  const apiKey = useRef<HTMLInputElement>(null);
  const [status, setStatus] = useState<ModelProviderStatus | null>(null);
  const [kind, setKind] = useState<ModelProviderKind>("local");
  const [baseUrl, setBaseUrl] = useState("");
  const [model, setModel] = useState("runory-local-doctor-v2");
  const [maxContextTokens, setMaxContextTokens] = useState(8192);
  const [remember, setRemember] = useState(false);
  const [busy, setBusy] = useState(false);
  const [failed, setFailed] = useState(false);
  const [tested, setTested] = useState(false);

  const applyStatus = (value: ModelProviderStatus) => {
    setStatus(value);
    setKind(value.kind);
    setBaseUrl(value.baseUrl);
    setModel(value.model);
    setMaxContextTokens(value.maxContextTokens);
  };

  useEffect(() => {
    void getAgentModel().then(applyStatus).catch(() => setFailed(true));
  }, []);

  const selectKind = (next: ModelProviderKind) => {
    setKind(next);
    if (next === "local") {
      setBaseUrl("");
      setModel("runory-local-doctor-v2");
      setMaxContextTokens(8192);
    } else if (next === "deep-seek" || next === "glm") {
      setBaseUrl(presets[next].baseUrl);
      setModel(presets[next].model);
      setMaxContextTokens(131072);
    }
  };

  const save = async () => {
    setBusy(true);
    setFailed(false);
    setTested(false);
    try {
      const value = apiKey.current?.value || null;
      applyStatus(await configureAgentModel({ kind, baseUrl, model, maxContextTokens, apiKey: value, rememberApiKey: remember }));
      if (apiKey.current) apiKey.current.value = "";
    } catch {
      setFailed(true);
      if (apiKey.current) apiKey.current.value = "";
    } finally {
      setBusy(false);
    }
  };

  const clearKey = async () => {
    setBusy(true);
    setFailed(false);
    try { applyStatus(await clearAgentModelApiKey()); } catch { setFailed(true); } finally { setBusy(false); }
  };

  const test = async () => {
    setBusy(true); setFailed(false); setTested(false);
    try { await testAgentModel(); setTested(true); } catch { setFailed(true); } finally { setBusy(false); }
  };

  return <section className="mt-4 border-t pt-4">
    <h3 className="text-xs font-medium text-[hsl(var(--secondary))]">{t("settings.agentModel")}</h3>
    <p className="mt-1 text-xs text-[hsl(var(--muted))]">{t("settings.agentModelHint")}</p>
    <div className="mt-3 grid gap-2">
      <label className="grid gap-1 text-xs"><span>{t("settings.modelProvider")}</span><select className="h-9 rounded-md border bg-[hsl(var(--surface))] px-2 text-sm" value={kind} onChange={(event) => selectKind(event.target.value as ModelProviderKind)}>
        <option value="local">{t("settings.modelProvider.local")}</option>
        <option value="deep-seek">DeepSeek</option>
        <option value="glm">GLM</option>
        <option value="open-ai-compatible">{t("settings.modelProvider.compatible")}</option>
      </select></label>
      {kind !== "local" && <>
        <label className="grid gap-1 text-xs"><span>{t("settings.modelBaseUrl")}</span><Input value={baseUrl} onChange={(event) => setBaseUrl(event.target.value)} placeholder="https://api.example.com/v1" /></label>
        <label className="grid gap-1 text-xs"><span>{t("settings.modelName")}</span><Input value={model} onChange={(event) => setModel(event.target.value)} /></label>
        <label className="grid gap-1 text-xs"><span>{t("settings.modelContext")}</span><Input type="number" min={256} max={1000000} value={maxContextTokens} onChange={(event) => setMaxContextTokens(Number(event.target.value))} /></label>
        <label className="grid gap-1 text-xs"><span>{t("settings.modelApiKey")}</span><Input ref={apiKey} type="password" autoComplete="off" placeholder={status?.apiKeyConfigured ? t("settings.modelApiKeyConfigured") : t("settings.modelApiKeyRequired")} /></label>
        <label className="flex items-center gap-2 text-xs"><input type="checkbox" checked={remember} onChange={(event) => setRemember(event.target.checked)} />{t("settings.rememberModelApiKey")}</label>
        <p className="text-[11px] text-[hsl(var(--muted))]">{t("settings.modelSecurityHint")}</p>
      </>}
      {failed && <p className="text-xs text-red-500">{t("settings.modelError")}</p>}
      {tested && <p className="text-xs text-emerald-600">{t("settings.modelTestSucceeded")}</p>}
      <div className="flex flex-wrap gap-2"><Button size="sm" disabled={busy} onClick={() => void save()}>{t("common.save")}</Button><Button size="sm" variant="secondary" disabled={busy || !status?.apiKeyConfigured} onClick={() => void test()}>{t("settings.testModel")}</Button>{status?.apiKeyConfigured && kind !== "local" && <Button size="sm" variant="secondary" disabled={busy} onClick={() => void clearKey()}>{t("settings.clearModelApiKey")}</Button>}</div>
    </div>
  </section>;
}
