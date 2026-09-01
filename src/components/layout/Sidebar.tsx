import { Clock3, FolderPlus, Plus, Search, Server, X } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { ConfirmDialog } from "../ui/confirm-dialog";
import { GroupSection } from "../../features/groups/GroupSection";
import { GroupDialog } from "../../features/groups/GroupDialog";
import { moveId } from "../../features/groups/reorder";
import { HostItem } from "../../features/profiles/HostItem";
import { ProfileDialog } from "../../features/profiles/ProfileDialog";
import { filterProfiles } from "../../features/profiles/search";
import { SettingsPanel } from "../../features/settings/SettingsPanel";
import { useCatalogStore } from "../../stores/catalog-store";
import { useSessionStore } from "../../stores/session-store";
import type { HostGroup, ServerProfile } from "../../types/domain";
import { Button } from "../ui/button";
import { Input } from "../ui/input";

type DialogState =
  | { kind: "group"; group?: HostGroup }
  | { kind: "profile"; profile?: ServerProfile; groupId?: string | null }
  | { kind: "delete-group"; group: HostGroup }
  | { kind: "delete-profile"; profile: ServerProfile }
  | null;

export function Sidebar({ mobileOpen = false, onMobileClose, query, onQueryChange, newProfileRequest, settingsRequest, onConnectProfile }: { mobileOpen?: boolean; onMobileClose?: () => void; query: string; onQueryChange: (query: string) => void; newProfileRequest: number; settingsRequest: number; onConnectProfile: (profileId: string) => void }) {
  const { t } = useTranslation(); const [settingsOpen, setSettingsOpen] = useState(false); const [dialog, setDialog] = useState<DialogState>(null);
  const searchInput = useRef<HTMLInputElement>(null);
  const [explorerWidth, setExplorerWidth] = useState(() => Math.min(420, Math.max(220, Number(localStorage.getItem("runory.explorerWidth")) || 260)));
  useEffect(() => { if (newProfileRequest > 0) setDialog({ kind: "profile" }); }, [newProfileRequest]);
  useEffect(() => { if (settingsRequest > 0) setSettingsOpen(true); }, [settingsRequest]);
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      const target = event.target as HTMLElement;
      if (event.key === "/" && !event.ctrlKey && !event.metaKey && !target.closest("input,textarea,[contenteditable='true']")) { event.preventDefault(); searchInput.current?.focus(); }
    };
    window.addEventListener("keydown", onKeyDown); return () => window.removeEventListener("keydown", onKeyDown);
  }, []);
  const { groups, profiles, selectedProfileId, loading, errorCode, selectProfile, updateGroup, deleteGroup, reorderGroups, deleteProfile, reorderProfiles } = useCatalogStore();
  const sessionTabs = useSessionStore((store) => store.tabs);
  const visibleProfiles = filterProfiles(groups, profiles, query);
  const ungroupedProfiles = visibleProfiles.filter((profile) => profile.groupId === null);
  const recentProfiles = visibleProfiles.filter((profile) => profile.lastConnectedAt).sort((left, right) => new Date(right.lastConnectedAt ?? 0).getTime() - new Date(left.lastConnectedAt ?? 0).getTime()).slice(0, 3);
  const searching = query.trim().length > 0;
  const sortingEnabled = !searching;
  const groupLabels = (name: string) => ({ add: t("profile.createTitle"), edit: t("common.edit"), delete: t("common.delete"), moveUp: t("group.moveUp", { name }), moveDown: t("group.moveDown", { name }), more: t("common.moreActions") });
  const moveGroup = (id: string, direction: -1 | 1) => { const current = groups.map((group) => group.id); const orderedIds = moveId(current, id, direction); if (orderedIds !== current) void reorderGroups(orderedIds).catch(() => undefined); };
  const moveProfile = (profile: ServerProfile, siblings: ServerProfile[], direction: -1 | 1) => { const current = siblings.map((item) => item.id); const orderedIds = moveId(current, profile.id, direction); if (orderedIds !== current) void reorderProfiles(profile.groupId, orderedIds).catch(() => undefined); };
  const beginResize = (event: React.PointerEvent<HTMLDivElement>) => {
    const startX = event.clientX; const startWidth = explorerWidth; let finalWidth = startWidth;
    const move = (moveEvent: PointerEvent) => { finalWidth = Math.min(420, Math.max(220, startWidth + moveEvent.clientX - startX)); setExplorerWidth(finalWidth); };
    const stop = () => { window.removeEventListener("pointermove", move); window.removeEventListener("pointerup", stop); localStorage.setItem("runory.explorerWidth", String(finalWidth)); };
    window.addEventListener("pointermove", move); window.addEventListener("pointerup", stop, { once: true });
  };
  const host = (profile: ServerProfile, siblings: ServerProfile[]) => { const index = siblings.findIndex((item) => item.id === profile.id); const state = sessionTabs.find((tab) => tab.profileId === profile.id && tab.state === "connected")?.state ?? sessionTabs.find((tab) => tab.profileId === profile.id)?.state ?? "idle"; return <HostItem key={profile.id} name={profile.name} address={`${profile.username}@${profile.host}${profile.port === 22 ? "" : `:${profile.port}`}`} active={selectedProfileId === profile.id} state={state} osDistribution={profile.osDistribution} labels={{ edit: t("profile.editTitle"), delete: t("profile.deleteTitle"), moveUp: t("profile.moveUp", { name: profile.name }), moveDown: t("profile.moveDown", { name: profile.name }), more: t("common.moreActions"), status: t(`status.${state}`) }} onSelect={() => { selectProfile(profile.id); onMobileClose?.(); }} onConnect={() => { selectProfile(profile.id); onMobileClose?.(); onConnectProfile(profile.id); }} onEdit={() => setDialog({ kind: "profile", profile })} onDelete={() => setDialog({ kind: "delete-profile", profile })} onMoveUp={sortingEnabled ? () => moveProfile(profile, siblings, -1) : undefined} onMoveDown={sortingEnabled ? () => moveProfile(profile, siblings, 1) : undefined} moveUpDisabled={index === 0} moveDownDisabled={index === siblings.length - 1} />; };
  return <aside style={{ "--explorer-width": `${explorerWidth}px` } as React.CSSProperties} className={`resource-explorer fixed bottom-0 left-0 top-[52px] z-50 flex w-[min(88vw,340px)] shrink-0 flex-col border-r bg-[hsl(var(--sidebar))] transition-transform md:static md:z-auto md:w-[var(--explorer-width)] md:min-w-[220px] md:translate-x-0 ${mobileOpen ? "translate-x-0" : "-translate-x-full"}`}>
    <div className="explorer-resize-handle" role="separator" aria-label={t("shell.resizeExplorer")} aria-orientation="vertical" onPointerDown={beginResize} />
    <div className="explorer-search-area"><div className="explorer-search"><Search size={15} aria-hidden="true" /><Input ref={searchInput} value={query} onChange={(event) => onQueryChange(event.target.value)} aria-label={t("sidebar.search")} placeholder={t("sidebar.search")} /><kbd>/</kbd></div><Button variant="ghost" size="icon" className="h-8 w-8" aria-label={t("profile.createTitle")} title={t("profile.createTitle")} onClick={() => setDialog({ kind: "profile" })}><Plus size={15} /></Button><Button variant="ghost" size="icon" className="h-8 w-8 md:hidden" aria-label={t("mobile.closeNavigation")} onClick={onMobileClose}><X size={18} /></Button></div>
    <nav className="resource-tree min-h-0 flex-1 overflow-y-auto">{loading && <p className="px-4 py-4 text-sm text-[hsl(var(--muted))]">{t("common.loading")}</p>}{errorCode && <p className="mx-3 rounded-md border border-red-500/40 bg-red-500/10 p-2 text-xs text-red-500">{t("errors.loadFailed")}</p>}{!loading && profiles.length === 0 && groups.length === 0 && <p className="px-4 py-4 text-sm text-[hsl(var(--muted))]">{t("sidebar.empty")}</p>}{groups.map((group, groupIndex) => { const items = visibleProfiles.filter((profile) => profile.groupId === group.id); if (searching && items.length === 0) return null; return <GroupSection key={group.id} name={group.name} count={items.length} collapsed={!searching && group.collapsed} labels={groupLabels(group.name)} onToggle={() => void updateGroup({ id: group.id, name: group.name, sortOrder: group.sortOrder, collapsed: !group.collapsed }).catch(() => undefined)} onAdd={() => setDialog({ kind: "profile", groupId: group.id })} onEdit={() => setDialog({ kind: "group", group })} onDelete={() => setDialog({ kind: "delete-group", group })} onMoveUp={sortingEnabled ? () => moveGroup(group.id, -1) : undefined} onMoveDown={sortingEnabled ? () => moveGroup(group.id, 1) : undefined} moveUpDisabled={groupIndex === 0} moveDownDisabled={groupIndex === groups.length - 1}>{items.map((profile) => host(profile, items))}</GroupSection>; })}{(ungroupedProfiles.length > 0 || groups.length === 0) && <GroupSection name={t("sidebar.ungrouped")} count={ungroupedProfiles.length} system labels={groupLabels(t("sidebar.ungrouped"))} onAdd={() => setDialog({ kind: "profile", groupId: null })}>{ungroupedProfiles.map((profile) => host(profile, ungroupedProfiles))}</GroupSection>}{recentProfiles.length > 0 && <section className="recent-section"><header><span>{t("sidebar.recentConnections")}</span><span>{recentProfiles.length}</span></header><div>{recentProfiles.map((profile) => <button key={profile.id} type="button" className="recent-host" onClick={() => { selectProfile(profile.id); onMobileClose?.(); }} onDoubleClick={() => onConnectProfile(profile.id)}><span className="recent-icon"><Server size={13} /></span><span className="host-copy"><span className="host-name">{profile.name}</span><span className="host-address">{profile.username}@{profile.host}</span></span><Clock3 size={14} className="recent-clock" /></button>)}</div></section>}</nav>
    <div className="explorer-footer"><Button variant="ghost" className="w-full justify-start" onClick={() => setDialog({ kind: "group" })}><FolderPlus size={16} aria-hidden="true" />{t("group.createTitle")}<Plus size={14} className="ml-auto" /></Button></div>
    {settingsOpen && <SettingsPanel onClose={() => setSettingsOpen(false)} />}
    {dialog?.kind === "group" && <GroupDialog group={dialog.group} onClose={() => setDialog(null)} />}
    {dialog?.kind === "profile" && <ProfileDialog profile={dialog.profile} initialGroupId={dialog.groupId} onClose={() => setDialog(null)} />}
    {dialog?.kind === "delete-group" && <ConfirmDialog title={t("group.deleteTitle")} description={t("group.deleteDescription", { name: dialog.group.name })} onConfirm={() => deleteGroup(dialog.group.id)} onClose={() => setDialog(null)} />}
    {dialog?.kind === "delete-profile" && <ConfirmDialog title={t("profile.deleteTitle")} description={sessionTabs.some((tab) => tab.profileId === dialog.profile.id && tab.sessionId) ? t("profile.deleteActiveDescription", { name: dialog.profile.name, count: sessionTabs.filter((tab) => tab.profileId === dialog.profile.id && tab.sessionId).length }) : t("profile.deleteDescription", { name: dialog.profile.name })} onConfirm={() => deleteProfile(dialog.profile.id)} onClose={() => setDialog(null)} />}
  </aside>;
}
