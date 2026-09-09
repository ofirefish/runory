import { Clock3, FolderPlus, LayoutGrid, List, Plus, Search, Server, X } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { ConfirmDialog } from "../ui/confirm-dialog";
import { GroupSection } from "../../features/groups/GroupSection";
import { GroupDialog } from "../../features/groups/GroupDialog";
import { moveId } from "../../features/groups/reorder";
import { HostItem } from "../../features/profiles/HostItem";
import { ProfileDialog } from "../../features/profiles/ProfileDialog";
import { filterProfiles } from "../../features/profiles/search";
import { ServerProfileDetails } from "../../features/profiles/ServerProfileDetails";
import { useProfileGroupDrag } from "../../features/profiles/use-profile-group-drag";
import { useCatalogStore } from "../../stores/catalog-store";
import { useSessionStore } from "../../stores/session-store";
import type { HostGroup, ServerProfile } from "../../types/domain";
import { Button } from "../ui/button";
import { Input } from "../ui/input";
import { ServerSyncStatus } from "./ServerSyncStatus";

type DialogState =
  | { kind: "group"; group?: HostGroup }
  | { kind: "profile"; profile?: ServerProfile; groupId?: string | null }
  | { kind: "delete-group"; group: HostGroup }
  | { kind: "delete-profile"; profile: ServerProfile }
  | null;

export function ServerManagement({ query, onQueryChange, onConnectProfile, onCreateTunnel, onOpenSync = () => undefined, onOpenAuth = () => undefined }: { query: string; onQueryChange: (query: string) => void; onConnectProfile: (profileId: string) => void; onCreateTunnel?: (profileId: string) => void; onOpenSync?: () => void; onOpenAuth?: () => void }) {
  const { t } = useTranslation();
  const [dialog, setDialog] = useState<DialogState>(null);
  const [filter, setFilter] = useState<"all" | "recent">("all");
  const [view, setView] = useState<"grid" | "list">(() => {
    try { return localStorage.getItem("runory.serversView") === "list" ? "list" : "grid"; }
    catch { return "grid"; }
  });
  const searchInput = useRef<HTMLInputElement>(null);
  const catalogRef = useRef<HTMLDivElement>(null);
  const managementRef = useRef<HTMLElement>(null);
  const drag = useProfileGroupDrag(managementRef);
  const { groups, profiles, selectedProfileId, loading, errorCode, selectProfile, updateGroup, deleteGroup, reorderGroups, deleteProfile, reorderProfiles } = useCatalogStore();
  const selectedProfile = profiles.find((profile) => profile.id === selectedProfileId) ?? null;
  const sessionTabs = useSessionStore((store) => store.tabs);
  const visibleProfiles = filterProfiles(groups, profiles, query);
  const ungroupedProfiles = visibleProfiles.filter((profile) => profile.groupId === null);
  const recentProfiles = visibleProfiles.filter((profile) => profile.lastConnectedAt).sort((left, right) => new Date(right.lastConnectedAt ?? 0).getTime() - new Date(left.lastConnectedAt ?? 0).getTime());
  const searching = query.trim().length > 0;
  const sortingEnabled = !searching && filter === "all";
  const shownCount = filter === "recent" ? recentProfiles.length : visibleProfiles.length;
  const connectedCount = profiles.filter((profile) => sessionTabs.some((tab) => tab.profileId === profile.id && tab.state === "connected")).length;

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      const target = event.target;
      if (event.key === "/" && !event.ctrlKey && !event.metaKey && target instanceof HTMLElement && !target.closest("input,textarea,[contenteditable='true'],[role='dialog']")) {
        event.preventDefault(); searchInput.current?.focus();
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, []);

  const changeView = (next: "grid" | "list") => {
    setView(next);
    try { localStorage.setItem("runory.serversView", next); } catch { /* Layout remains usable when storage is unavailable. */ }
  };
  const closeDetails = () => {
    selectProfile(null);
    catalogRef.current?.querySelector<HTMLButtonElement>(".host-item.active .host-select")?.focus();
  };
  const groupLabels = (name: string) => ({ add: t("profile.createTitle"), edit: t("common.edit"), delete: t("common.delete"), moveUp: t("group.moveUp", { name }), moveDown: t("group.moveDown", { name }), more: t("common.moreActions") });
  const moveGroup = (id: string, direction: -1 | 1) => {
    const current = groups.map((group) => group.id);
    const orderedIds = moveId(current, id, direction);
    if (orderedIds !== current) void reorderGroups(orderedIds).catch(() => undefined);
  };
  const moveProfile = (profile: ServerProfile, siblings: ServerProfile[], direction: -1 | 1) => {
    const current = siblings.map((item) => item.id);
    const orderedIds = moveId(current, profile.id, direction);
    if (orderedIds !== current) void reorderProfiles(profile.groupId, orderedIds).catch(() => undefined);
  };
  const host = (profile: ServerProfile, siblings: ServerProfile[]) => {
    const index = siblings.findIndex((item) => item.id === profile.id);
    const state = sessionTabs.find((tab) => tab.profileId === profile.id && tab.state === "connected")?.state ?? sessionTabs.find((tab) => tab.profileId === profile.id)?.state ?? "idle";
    const connectionRoute = profile.connectionRoute;
    const jumpProfile = connectionRoute.type === "jumpHost" ? profiles.find((candidate) => candidate.id === connectionRoute.profileId) : undefined;
    return <HostItem key={profile.id}
      name={profile.name} address={`${profile.username}@${profile.host}${profile.port === 22 ? "" : `:${profile.port}`}`}
      active={selectedProfileId === profile.id} state={state} osDistribution={profile.osDistribution}
      summary={`${t(profile.authMethod === "password" ? "profile.password" : "profile.privateKey")}${profile.connectionRoute.type === "jumpHost" ? ` · ${t("profile.routeJumpHost")}` : ""}`}
      jumpHostLabel={profile.connectionRoute.type === "jumpHost" ? jumpProfile ? t("profile.jumpHostIndicatorNamed", { name: jumpProfile.name }) : t("profile.jumpHostIndicator") : undefined}
      labels={{ edit: t("profile.editTitle"), delete: t("profile.deleteTitle"), moveUp: t("profile.moveUp", { name: profile.name }), moveDown: t("profile.moveDown", { name: profile.name }), more: t("common.moreActions"), status: t(`status.${state}`), connect: t("serverManagement.openSession"), drag: t("serverManagement.dragHost", { name: profile.name }) }}
      onDragPointerDown={(event) => drag.startDrag(event, profile.id)} dragging={drag.draggedId === profile.id} dragDisabled={drag.moving || loading || dialog !== null}
      onSelect={() => selectProfile(profile.id)}
      onCreateTunnel={onCreateTunnel ? () => onCreateTunnel(profile.id) : undefined} tunnelLabel={t("tunnels.create")}
      onConnect={() => { selectProfile(profile.id); onConnectProfile(profile.id); }}
      onEdit={() => setDialog({ kind: "profile", profile })} onDelete={() => setDialog({ kind: "delete-profile", profile })}
      onMoveUp={sortingEnabled ? () => moveProfile(profile, siblings, -1) : undefined}
      onMoveDown={sortingEnabled ? () => moveProfile(profile, siblings, 1) : undefined}
      moveUpDisabled={index === 0} moveDownDisabled={index === siblings.length - 1} />;
  };

  return <main ref={managementRef} className={`server-management${drag.draggedId ? " server-management-dragging" : ""}`} aria-label={t("serverManagement.title")}
    onPointerDownCapture={drag.onPointerDownCapture} onClickCapture={drag.onClickCapture} onDoubleClickCapture={drag.onClickCapture}>
    <header className="server-management-header">
      <div className="server-management-heading">
        <span className="server-heading-icon"><Server size={17} aria-hidden="true" /></span>
        <div className="server-heading-copy">
          <div><h1>{t("sidebar.servers")}</h1><span className="server-total">{profiles.length}</span></div>
          <span className="server-connected"><span className="host-status-dot status-connected" aria-hidden="true" />{t("serverManagement.connectedCount", { count: connectedCount })}</span>
        </div>
      </div>
      <div className="server-header-actions">
        <ServerSyncStatus onOpenSync={onOpenSync} onOpenAuth={onOpenAuth} />
        <Button className="server-create-group" variant="secondary" size="sm" onClick={() => setDialog({ kind: "group" })}><FolderPlus size={15} aria-hidden="true" />{t("group.createTitle")}</Button>
        <Button size="sm" onClick={() => setDialog({ kind: "profile" })}><Plus size={15} aria-hidden="true" />{t("profile.createTitle")}</Button>
      </div>
    </header>
    <div className="server-toolbar">
      <div className="explorer-search"><Search size={15} aria-hidden="true" /><Input ref={searchInput} value={query} onChange={(event) => onQueryChange(event.target.value)} aria-label={t("sidebar.search")} placeholder={t("sidebar.search")} />
        {searching ? <button type="button" aria-label={t("serverManagement.clearSearch")} title={t("serverManagement.clearSearch")} onClick={() => { onQueryChange(""); searchInput.current?.focus(); }}><X size={14} /></button> : <kbd>/</kbd>}
      </div>
      <div className="server-filter-switch" role="group" aria-label={t("serverManagement.filter")}>
        <button type="button" aria-pressed={filter === "all"} onClick={() => setFilter("all")}>{t("serverManagement.allServers")}</button>
        <button type="button" aria-pressed={filter === "recent"} onClick={() => setFilter("recent")}><Clock3 size={14} aria-hidden="true" />{t("sidebar.recentConnections")}</button>
      </div>
      <span className="server-result-count" role="status">{t("serverManagement.showingCount", { count: shownCount, total: profiles.length })}</span>
      <div className="server-view-switch" role="group" aria-label={t("serverManagement.view")}>
        <button type="button" aria-pressed={view === "grid"} aria-label={t("serverManagement.gridView")} title={t("serverManagement.gridView")} onClick={() => changeView("grid")}><LayoutGrid size={16} /></button>
        <button type="button" aria-pressed={view === "list"} aria-label={t("serverManagement.listView")} title={t("serverManagement.listView")} onClick={() => changeView("list")}><List size={16} /></button>
      </div>
    </div>
    {(drag.moving || drag.notice) && <p className={`server-move-notice${drag.notice ? " server-move-error" : ""}`} role={drag.notice ? "alert" : "status"}>{drag.moving ? t("serverManagement.movingHost") : drag.notice && t("serverManagement.moveFailed", { name: drag.notice.name })}</p>}
    <div className="server-management-body">
      {drag.draggedId && <div className="server-group-drop-tray" aria-label={t("serverManagement.dropTargets")}>
        <p role="status">{t("serverManagement.dragHint")}</p>
        <div>{[...groups.map((group) => ({ id: group.id, name: group.name })), { id: "", name: t("sidebar.ungrouped") }].map((group) => <div key={group.id} data-server-group={group.id} className={drag.overGroup === group.id ? "server-group-drop-active" : ""}>{group.name}</div>)}</div>
      </div>}
      <div ref={catalogRef} className={`server-catalog server-catalog-${view}`} aria-label={t("sidebar.servers")} aria-busy={loading}>
        {errorCode && <p role="alert" className="server-catalog-error">{t("errors.loadFailed")}</p>}
        {loading ? <p className="server-catalog-message" role="status">{t("common.loading")}</p> : <>
          {shownCount === 0 && <div className="server-catalog-empty"><Search size={28} aria-hidden="true" /><h2>{t(searching ? "serverManagement.noResults" : filter === "recent" ? "serverManagement.noRecent" : "sidebar.empty")}</h2>
            {searching ? <Button variant="secondary" size="sm" onClick={() => onQueryChange("")}>{t("serverManagement.clearSearch")}</Button> : filter === "all" && <Button size="sm" onClick={() => setDialog({ kind: "profile" })}><Plus size={15} />{t("profile.createTitle")}</Button>}
          </div>}
          {filter === "recent" ? <div className="group-hosts">{recentProfiles.map((profile) => host(profile, recentProfiles))}</div> : <>
            {groups.map((group, groupIndex) => {
              const items = visibleProfiles.filter((profile) => profile.groupId === group.id);
              if (searching && items.length === 0) return null;
              return <GroupSection key={group.id} name={group.name} count={items.length} collapsed={!searching && group.collapsed} labels={groupLabels(group.name)}
                dropTargetId={group.id} dropActive={drag.draggedId !== null && drag.overGroup === group.id}
                onToggle={() => void updateGroup({ id: group.id, name: group.name, sortOrder: group.sortOrder, collapsed: !group.collapsed }).catch(() => undefined)}
                onAdd={() => setDialog({ kind: "profile", groupId: group.id })} onEdit={() => setDialog({ kind: "group", group })} onDelete={() => setDialog({ kind: "delete-group", group })}
                onMoveUp={sortingEnabled ? () => moveGroup(group.id, -1) : undefined} onMoveDown={sortingEnabled ? () => moveGroup(group.id, 1) : undefined}
                moveUpDisabled={groupIndex === 0} moveDownDisabled={groupIndex === groups.length - 1}>{items.map((profile) => host(profile, items))}</GroupSection>;
            })}
            {ungroupedProfiles.length > 0 && <GroupSection name={t("sidebar.ungrouped")} count={ungroupedProfiles.length} system dropTargetId="" dropActive={drag.draggedId !== null && drag.overGroup === ""} labels={groupLabels(t("sidebar.ungrouped"))} onAdd={() => setDialog({ kind: "profile", groupId: null })}>{ungroupedProfiles.map((profile) => host(profile, ungroupedProfiles))}</GroupSection>}
          </>}
        </>}
      </div>
      {selectedProfile && <ServerProfileDetails profile={selectedProfile} groupName={groups.find((group) => group.id === selectedProfile.groupId)?.name ?? t("sidebar.ungrouped")}
        onCreateTunnel={onCreateTunnel ? () => onCreateTunnel(selectedProfile.id) : undefined}
        onClose={closeDetails} onConnect={() => onConnectProfile(selectedProfile.id)} onEdit={() => setDialog({ kind: "profile", profile: selectedProfile })} onDelete={() => setDialog({ kind: "delete-profile", profile: selectedProfile })} />}
    </div>
    {dialog?.kind === "group" && <GroupDialog group={dialog.group} onClose={() => setDialog(null)} />}
    {dialog?.kind === "profile" && <ProfileDialog profile={dialog.profile} initialGroupId={dialog.groupId} onClose={() => setDialog(null)} />}
    {dialog?.kind === "delete-group" && <ConfirmDialog title={t("group.deleteTitle")} description={t("group.deleteDescription", { name: dialog.group.name })} onConfirm={() => deleteGroup(dialog.group.id)} onClose={() => setDialog(null)} />}
    {dialog?.kind === "delete-profile" && <ConfirmDialog title={t("profile.deleteTitle")} description={sessionTabs.some((tab) => tab.profileId === dialog.profile.id && tab.sessionId) ? t("profile.deleteActiveDescription", { name: dialog.profile.name, count: sessionTabs.filter((tab) => tab.profileId === dialog.profile.id && tab.sessionId).length }) : t("profile.deleteDescription", { name: dialog.profile.name })} onConfirm={() => deleteProfile(dialog.profile.id)} onClose={() => setDialog(null)} />}
  </main>;
}
