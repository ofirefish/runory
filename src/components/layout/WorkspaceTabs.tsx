import { X } from "lucide-react";
import { useEffect, useRef, type MouseEvent } from "react";
import { useTranslation } from "react-i18next";
import { JumpHostIndicator } from "../../features/profiles/JumpHostIndicator";
import { OsLogo } from "../../features/profiles/OsLogo";
import type { SessionTab } from "../../stores/session-store";
import type { ServerProfile } from "../../types/domain";

export function WorkspaceTabs({ tabs, profiles, activeTabId, onSelect, onClose, onContextMenu }: {
  tabs: SessionTab[];
  profiles: ServerProfile[];
  activeTabId: string | null;
  onSelect: (tabId: string) => void;
  onClose: (tabId: string) => void;
  onContextMenu: (event: MouseEvent, tabId: string) => void;
}) {
  const { t } = useTranslation();
  const strip = useRef<HTMLDivElement>(null);
  useEffect(() => {
    strip.current?.querySelector('[aria-current="page"]')?.scrollIntoView?.({ block: "nearest", inline: "nearest" });
  }, [activeTabId, tabs.length]);
  return <nav className="workspace-tabs" aria-label={t("shell.sessions")}>
    <div className="workspace-tab-strip" ref={strip}>
      {tabs.map((tab) => {
        const profile = profiles.find((candidate) => candidate.id === tab.profileId);
        const connectionRoute = profile?.connectionRoute;
        const jumpProfile = connectionRoute?.type === "jumpHost"
          ? profiles.find((candidate) => candidate.id === connectionRoute.profileId)
          : undefined;
        const active = tab.id === activeTabId;
        const title = tab.title ?? profile?.name ?? t("terminal.unknownProfile");
        const jumpHostLabel = jumpProfile
          ? t("profile.jumpHostIndicatorNamed", { name: jumpProfile.name })
          : t("profile.jumpHostIndicator");
        return <div key={tab.id} className={`workspace-tab ${active ? "active" : ""}`} onContextMenu={(event) => onContextMenu(event, tab.id)}>
          <button type="button" className="workspace-tab-select" aria-current={active ? "page" : undefined} title={title} onClick={() => onSelect(tab.id)}>
            {profile?.osDistribution ? <OsLogo plain distribution={profile.osDistribution} state={tab.state} statusLabel={t(`status.${tab.state}`)} /> : <span className={`workspace-tab-status status-${tab.state}`} aria-label={t(`status.${tab.state}`)} />}
            {profile?.connectionRoute.type === "jumpHost" && <JumpHostIndicator label={jumpHostLabel} />}
            <span>{title}</span>
          </button>
          <button type="button" className="workspace-tab-close" aria-label={t("terminal.closeTab", { name: title })} title={t("terminal.closeTab", { name: title })} onClick={() => onClose(tab.id)}><X size={13} /></button>
        </div>;
      })}
    </div>
  </nav>;
}
