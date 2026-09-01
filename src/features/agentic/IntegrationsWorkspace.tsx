import { Network, RefreshCw } from "lucide-react";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "../../components/ui/button";
import { Input } from "../../components/ui/input";
import { configureMcp, listMcp, refreshSkills, removeMcp, setMcpEnabled, setMcpToolEnabled, setSkillEnabled } from "../../lib/tauri/agentic";
import type { McpServerConfig, Skill } from "../../types/agentic";

export default function IntegrationsWorkspace() {
  const { t } = useTranslation();
  const [skills, setSkills] = useState<Skill[]>([]);
  const [mcp, setMcp] = useState<McpServerConfig[]>([]);
  const [label, setLabel] = useState("");
  const [endpoint, setEndpoint] = useState("");
  const [token, setToken] = useState("");
  const [busy, setBusy] = useState(false);
  const [failed, setFailed] = useState(false);

  const reloadSkills = async () => setSkills(await refreshSkills());
  const reloadMcp = async () => setMcp(await listMcp());
  useEffect(() => {
    void refreshSkills().then(setSkills).catch(() => setFailed(true));
    void listMcp().then(setMcp).catch(() => setFailed(true));
  }, []);
  const save = async () => {
    setBusy(true); setFailed(false);
    try { await configureMcp(crypto.randomUUID(), label.trim(), endpoint.trim(), token || null); setToken(""); setLabel(""); setEndpoint(""); await reloadMcp(); } catch { setFailed(true); } finally { setBusy(false); }
  };

  return <div className="grid gap-4">
    {failed && <p className="rounded border border-red-500/30 bg-red-500/5 p-3 text-sm text-red-500">{t("agent.error")}</p>}
    <div className="grid gap-4 lg:grid-cols-2">
      <section className="rounded-xl border p-4">
        <div className="flex items-center justify-between"><h3 className="font-medium">{t("agentic.skills")}</h3><Button size="icon" variant="ghost" onClick={() => void reloadSkills()}><RefreshCw size={15} /></Button></div>
        <div className="mt-3 grid gap-2">{skills.map((skill) => <div key={skill.manifest.id} className="rounded border p-3 text-sm"><div className="flex items-center justify-between gap-2"><span>{skill.manifest.id} · {skill.manifest.version}</span><input type="checkbox" checked={skill.enabled} disabled={skill.permissionReviewCodes.length > 0} onChange={async (event) => { await setSkillEnabled(skill.manifest.id, event.target.checked); await reloadSkills(); }} /></div><p className="mt-1 text-xs text-[hsl(var(--muted))]">{skill.origin} · {skill.manifest.riskCeiling} · {skill.permissionReviewCodes.join(", ") || t("agentic.permissionOk")}</p></div>)}</div>
      </section>
      <section className="rounded-xl border p-4">
        <div className="flex items-center gap-2"><Network size={17} /><h3 className="font-medium">{t("agentic.mcp")}</h3></div>
        <div className="mt-3 grid gap-2">
          <Input value={label} onChange={(event) => setLabel(event.target.value)} placeholder={t("agentic.mcpLabel")} />
          <Input value={endpoint} onChange={(event) => setEndpoint(event.target.value)} placeholder="https://…" />
          <Input type="password" value={token} onChange={(event) => setToken(event.target.value)} placeholder={t("agentic.mcpToken")} />
          <Button disabled={busy || !label.trim() || !endpoint.trim()} onClick={() => void save()}>{t("agentic.connectMcp")}</Button>
          {mcp.map((server) => <div key={server.id} className="rounded border p-3 text-sm">
            <div className="flex items-center justify-between gap-2"><label className="flex items-center gap-2"><input type="checkbox" checked={server.enabled} onChange={async (event) => { await setMcpEnabled(server.id, event.target.checked); await reloadMcp(); }} /><span>{server.label}</span></label><Button variant="ghost" size="sm" onClick={async () => { await removeMcp(server.id); await reloadMcp(); }}>{t("common.delete")}</Button></div>
            <p className="mt-1 text-xs text-[hsl(var(--muted))]">{server.transportEra ?? t("agentic.mcpReconnectRequired")} · {server.protocolVersion ?? "—"}</p>
            {server.tools.map((tool) => <label key={tool.name} className="mt-2 flex items-center justify-between gap-2 text-xs"><span>{tool.name}{!tool.readOnly ? ` · ${t("agentic.writeBlocked")}` : ""}{tool.requiresArguments ? ` · ${t("agentic.argumentsRequired")}` : ""}</span><input type="checkbox" checked={tool.enabled} disabled={!tool.readOnly} onChange={async (event) => { await setMcpToolEnabled(server.id, tool.name, event.target.checked); await reloadMcp(); }} /></label>)}
          </div>)}
        </div>
      </section>
    </div>
  </div>;
}
