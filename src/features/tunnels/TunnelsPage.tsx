import { ArrowUpRight, Cable, Copy, MoreHorizontal, Play, Plus, Search, Server, ShieldCheck, Square } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "../../components/ui/button";
import { ConfirmDialog } from "../../components/ui/confirm-dialog";
import { ConnectionDialog } from "../sessions/ConnectionDialog";
import { testSsh } from "../../lib/tauri/ssh";
import { Input } from "../../components/ui/input";
import { SelectControl } from "../../components/ui/select-control";
import { appErrorCode } from "../../lib/app-error";
import * as api from "../../lib/tauri/tunnels";
import { useCatalogStore } from "../../stores/catalog-store";
import { useSessionStore } from "../../stores/session-store";
import { localEndpoint, targetEndpoint, type SaveTunnelRequest, type TunnelRule, type TunnelState } from "../../types/tunnels";
import { TunnelForm } from "./TunnelForm";
import { TunnelDetails } from "./TunnelDetails";
import { TunnelOverview } from "./TunnelOverview";
import { useTunnels } from "./use-tunnels";
import "./tunnels.css";

export function TunnelsPage({ initialProfileId, onConnectProfile, onShowSession }: { initialProfileId?: string; onConnectProfile: (id: string) => void; onShowSession: (id: string) => void }) {
  const { t } = useTranslation();
  const profiles = useCatalogStore((store) => store.profiles);
  const tabs = useSessionStore((store) => store.tabs);
  const { items, error, loading, refresh } = useTunnels();
  const [query, setQuery] = useState("");
  const [profileFilter, setProfileFilter] = useState("all");
  const [stateFilter, setStateFilter] = useState<TunnelState | "all">("all");
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [form, setForm] = useState<{ rule?: TunnelRule; profileId?: string } | null>(initialProfileId ? { profileId: initialProfileId } : null);
  const [deleting, setDeleting] = useState<TunnelRule | null>(null);
  const [starting, setStarting] = useState<TunnelRule | null>(null);
  const [busy, setBusy] = useState(false);
  const [failure, setFailure] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const selected = items.find((item) => item.rule.id === selectedId);
  const nameOf = (id: string) => profiles.find((profile) => profile.id === id)?.name ?? t("tunnels.missingProfile");
  const connected = (profileId: string) => tabs.filter((tab) => tab.profileId === profileId && tab.state === "connected" && tab.sessionId);
  const startingProfile = starting ? profiles.find((profile) => profile.id === starting.profileId) : undefined;
  const visible = items.filter(({ rule, status }) => (profileFilter === "all" || rule.profileId === profileFilter) && (stateFilter === "all" || status.state === stateFilter) && `${rule.name} ${nameOf(rule.profileId)} ${rule.targetHost} ${rule.targetPort} ${rule.localPort}`.toLowerCase().includes(query.trim().toLowerCase()));
  const groupIds = [...new Set(visible.map((item) => item.rule.profileId))];

  const action = async (perform: () => Promise<unknown>) => {
    setBusy(true); setFailure(null); setNotice(null);
    try { await perform(); } catch (cause) { setFailure(appErrorCode(cause)); }
    finally { await refresh(); setBusy(false); }
  };
  const requestStart = async (rule: TunnelRule) => {
    setSelectedId(rule.id); setBusy(true); setFailure(null); setNotice(null);
    try { await api.startTunnel(rule.id); }
    catch (cause) {
      const code = appErrorCode(cause);
      if (code === "TUNNEL_CONNECTION_REQUIRED") setStarting(rule);
      else setFailure(code);
    } finally { await refresh(); setBusy(false); }
  };
  const save = async (request: SaveTunnelRequest, start: boolean) => {
    const rule = await api.saveTunnel(request);
    setForm(null); setSelectedId(rule.id);
    await refresh();
    if (start) await requestStart(rule);
  };
  const copy = async (rule: TunnelRule) => {
    try { await navigator.clipboard.writeText(localEndpoint(rule)); setNotice(t("tunnels.copied")); setFailure(null); }
    catch { setFailure("TUNNEL_CLIPBOARD_FAILED"); }
  };
  const showSession = (rule: TunnelRule, boundSession: string | null) => {
    const tab = tabs.find((item) => boundSession !== null && item.sessionId === boundSession) ?? connected(rule.profileId)[0];
    if (tab) onShowSession(tab.id); else onConnectProfile(rule.profileId);
  };
  return <main className="tunnels-page" aria-label={t("tunnels.title")}>
    <header className="tunnels-header">
      <div className="tunnels-heading"><span className="tunnels-heading-icon"><Cable size={20} aria-hidden="true" /></span><div><h1>{t("tunnels.title")}</h1><p>{t("tunnels.subtitle")}</p></div></div>
      <span className="tunnel-scope"><ShieldCheck size={14} aria-hidden="true" />{t("tunnels.localOnly")}</span>
      <Button size="sm" disabled={busy || profiles.length === 0 || !!error} onClick={() => setForm({})}><Plus size={15} />{t("tunnels.create")}</Button>
    </header>
    <TunnelOverview items={items} unavailable={loading || !!error} />
    <div className="tunnels-toolbar"><div className="tunnel-search"><Search size={15} aria-hidden="true" /><Input aria-label={t("tunnels.search")} placeholder={t("tunnels.search")} value={query} onChange={(event) => setQuery(event.target.value)} /></div><SelectControl label={t("tunnels.serverFilter")} value={profileFilter} onValueChange={setProfileFilter} options={[{ value: "all", label: t("tunnels.allServers") }, ...profiles.map((profile) => ({ value: profile.id, label: profile.name }))]} /><SelectControl<TunnelState | "all"> label={t("tunnels.state")} value={stateFilter} onValueChange={setStateFilter} options={(["all", "running", "stopped", "interrupted", "error"] as const).map((state) => ({ value: state, label: t(state === "all" ? "tunnels.allStates" : `tunnels.state.${state}`) }))} /></div>
    {(failure || error) && <p className="tunnel-error tunnel-banner" role="alert">{t(`errors.${failure ?? error}`, { defaultValue: t("errors.UNKNOWN") })}<Button size="sm" variant="ghost" onClick={() => { setFailure(null); void refresh(); }}>{t("tunnels.refresh")}</Button></p>}
    {notice && <p className="tunnel-banner" role="status">{notice}</p>}
    <div className="tunnels-body"><div className="tunnels-list" aria-busy={loading}>
      {loading ? <p className="tunnel-empty" role="status">{t("common.loading")}</p> : visible.length === 0 && <div className="tunnel-empty"><Cable size={28} aria-hidden="true" /><h2>{t(items.length ? "tunnels.noResults" : "tunnels.empty")}</h2><p>{t(profiles.length ? "tunnels.emptyHint" : "tunnels.noProfiles")}</p></div>}
      {groupIds.map((id) => <section className="tunnel-group" key={id} aria-label={nameOf(id)}><header className="tunnel-group-header"><Server size={16} aria-hidden="true" /><h2>{nameOf(id)}</h2><span>{t("tunnels.ruleCount", { count: visible.filter((item) => item.rule.profileId === id).length })}</span></header><table><thead><tr><th>{t("tunnels.name")}</th><th>{t("tunnels.local")}</th><th>{t("tunnels.target")}</th><th>{t("tunnels.state")}</th><th><span className="sr-only">{t("common.moreActions")}</span></th></tr></thead><tbody>{visible.filter((item) => item.rule.profileId === id).map(({ rule, status }) => <tr key={rule.id} data-state={status.state} className={selectedId === rule.id ? "selected" : ""}>
        <td><button type="button" className="tunnel-select" aria-pressed={selectedId === rule.id} onClick={() => setSelectedId(rule.id)}>{rule.name}</button></td><td><span className="tunnel-endpoint tunnel-endpoint-local"><i aria-hidden="true" /><code>{localEndpoint(rule)}</code></span></td><td><span className="tunnel-endpoint"><ArrowUpRight size={14} aria-hidden="true" /><code>{targetEndpoint(rule)}</code></span></td><td><span className={`tunnel-state tunnel-state-${status.state}`}><i aria-hidden="true" />{t(`tunnels.state.${status.state}`)}</span><small className={status.health === "unreachable" ? "tunnel-error" : ""}>{t(`tunnels.health.${status.health}`)}</small></td>
        <td><div className="tunnel-row-actions"><Button size="icon" variant="ghost" aria-label={t("tunnels.copyNamed", { name: rule.name })} title={t("tunnels.copy")} onClick={() => void copy(rule)}><Copy size={15} /></Button><Button size="icon" variant="ghost" disabled={busy || !!error || (status.state !== "running" && !profiles.some((profile) => profile.id === rule.profileId))} aria-label={t(status.state === "running" ? "tunnels.stopNamed" : "tunnels.startNamed", { name: rule.name })} title={t(status.state === "running" ? "tunnels.stop" : "tunnels.start")} onClick={() => status.state === "running" ? void action(() => api.stopTunnel(rule.id)) : void requestStart(rule)}>{status.state === "running" ? <Square size={15} /> : <Play size={15} />}</Button><details className="item-menu"><summary aria-label={t("tunnels.moreNamed", { name: rule.name })} title={t("common.moreActions")}><MoreHorizontal size={16} /></summary><div className="item-menu-popover"><button disabled={busy || status.state !== "running"} onClick={() => { setSelectedId(rule.id); void action(() => api.checkTunnel(rule.id)); }}>{t("tunnels.check")}</button><button disabled={busy || status.state === "running"} onClick={() => setForm({ rule })}>{t("common.edit")}</button><button disabled={busy || status.state === "running"} onClick={() => setDeleting(rule)}>{t("common.delete")}</button></div></details></div></td>
      </tr>)}</tbody></table></section>)}
    </div>{selected && <TunnelDetails tunnel={selected} profileName={nameOf(selected.rule.profileId)} busy={busy} onClose={() => setSelectedId(null)} onCopy={() => void copy(selected.rule)} onCheck={() => void action(() => api.checkTunnel(selected.rule.id))} onSession={() => showSession(selected.rule, selected.status.sessionId)} />}</div>
    <footer className="tunnels-footer"><ShieldCheck size={13} aria-hidden="true" />{t("tunnels.footer")}</footer>
    {form && <TunnelForm rule={form.rule} initialProfileId={form.profileId} profiles={profiles} onSave={save} onClose={() => setForm(null)} />}
    {deleting && <ConfirmDialog title={t("tunnels.delete")} description={t("tunnels.deleteHint", { name: deleting.name })} onConfirm={async () => { await api.deleteTunnel(deleting.id); await refresh(); }} onClose={() => setDeleting(null)} />}
    {starting && startingProfile && <ConnectionDialog key={starting.id} profile={startingProfile} mode="tunnel" onClose={() => setStarting(null)}
      onConnect={async (values) => {
        try {
          const result = await api.connectStartTunnel(starting.id, values);
          return result.credentialSaved;
        } finally { await refresh(); }
      }}
      onTest={async (values) => (await testSsh(values)).credentialSaved}
    />}
  </main>;
}
