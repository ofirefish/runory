import type { Session } from "@supabase/supabase-js";
import { CreditCard, LogIn, LogOut, Settings, UserRound } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { Popover, PopoverContent, PopoverTrigger } from "../ui/popover";
import { cloudConfigured } from "../../lib/supabase/client";
import {
  cloudSession,
  cloudSignOut,
  cloudProfileUpdatedEvent,
  loadMyCloudProfile,
  onCloudAuthStateChange,
} from "../../lib/supabase/cloud";
import { loadCloudAvatar, loadLatestCachedCloudAvatar } from "../../lib/cloud-avatar";
import { lockCloudPolicy } from "../../lib/tauri/cloud-policy";
import { useCloudIdentityStore } from "../../stores/cloud-identity-store";

type UserIdentity = {
  avatarUrl: string | null;
  displayName: string;
  email: string | null;
};

function fallbackIdentity(session: Session | null, localLabel: string): UserIdentity {
  const email = session?.user.email ?? null;
  return {
    avatarUrl: null,
    displayName: email?.split("@")[0] || localLabel,
    email,
  };
}

function initials(value: string) {
  const segments = value.trim().split(/\s+/).filter(Boolean);
  if (segments.length > 1) return `${segments[0][0]}${segments.at(-1)?.[0] ?? ""}`.toUpperCase();
  return value.slice(0, 2).toUpperCase();
}

export function UserMenu({ onOpenAccount, onOpenAuth, onOpenSettings, onOpenPricing }: { onOpenAccount: () => void; onOpenAuth: () => void; onOpenSettings: () => void; onOpenPricing: () => void }) {
  const { t } = useTranslation();
  const [open, setOpen] = useState(false);
  const [session, setSession] = useState<Session | null>(null);
  const [identity, setIdentity] = useState<UserIdentity>(() => fallbackIdentity(null, t("userMenu.localUser")));
  const [signOutFailed, setSignOutFailed] = useState(false);
  const setSharedAvatarUrl = useCloudIdentityStore((store) => store.setAvatarUrl);

  useEffect(() => {
    setSharedAvatarUrl(identity.avatarUrl);
  }, [identity.avatarUrl, setSharedAvatarUrl]);

  useEffect(() => {
    if (!cloudConfigured) {
      setIdentity(fallbackIdentity(null, t("userMenu.localUser")));
      return;
    }
    let active = true;
    let currentSession: Session | null = null;
    let currentAvatarPath: string | null = null;
    let currentAvatarVersion: number | null = null;
    let currentAvatarUrl: string | null = null;
    const applySession = async (nextSession: Session | null, preserveIdentity = false) => {
      if (!active) return;
      currentSession = nextSession;
      setSession(nextSession);
      const fallback = fallbackIdentity(nextSession, t("userMenu.localUser"));
      if (!preserveIdentity) setIdentity(fallback);
      if (!nextSession) {
        currentAvatarPath = null;
        currentAvatarVersion = null;
        currentAvatarUrl = null;
        return;
      }
      try {
        const profile = await loadMyCloudProfile(nextSession.user.id);
        if (profile?.avatar_path !== currentAvatarPath || profile?.avatar_version !== currentAvatarVersion) {
          currentAvatarUrl = profile?.avatar_path
            ? await loadCloudAvatar(nextSession.user.id, profile.avatar_path, profile.avatar_version)
            : null;
          currentAvatarPath = profile?.avatar_path ?? null;
          currentAvatarVersion = profile?.avatar_version ?? null;
        }
        if (active) setIdentity({ avatarUrl: currentAvatarUrl, displayName: profile?.display_name || fallback.displayName, email: fallback.email });
      } catch {
        // Keep the account recognizable offline without persisting its signed URL.
        const cachedAvatarUrl = await loadLatestCachedCloudAvatar(nextSession.user.id).catch(() => null);
        if (active && cachedAvatarUrl) {
          setIdentity({ ...fallback, avatarUrl: cachedAvatarUrl });
        }
      }
    };
    void cloudSession().then(applySession).catch(() => undefined);
    const subscription = onCloudAuthStateChange((_event, nextSession) => { void applySession(nextSession); });
    const onProfileUpdated = () => { if (currentSession) void applySession(currentSession, true); };
    window.addEventListener(cloudProfileUpdatedEvent, onProfileUpdated);
    return () => { active = false; subscription.unsubscribe(); window.removeEventListener(cloudProfileUpdatedEvent, onProfileUpdated); };
  }, [t]);

  const avatarLabel = useMemo(() => initials(identity.displayName), [identity.displayName]);
  const openPanel = (action: () => void) => { setOpen(false); action(); };
  const signOut = async () => {
    setSignOutFailed(false);
    try {
      await lockCloudPolicy();
      await cloudSignOut();
      setSession(null);
      setIdentity(fallbackIdentity(null, t("userMenu.localUser")));
      setOpen(false);
    } catch { setSignOutFailed(true); }
  };

  return <Popover open={open} onOpenChange={(nextOpen) => { setOpen(nextOpen); if (nextOpen) setSignOutFailed(false); }}>
    <PopoverTrigger asChild>
      <button type="button" className="rail-user-trigger" aria-label={t("userMenu.open")} title={t("userMenu.open")}>
        <span className="rail-avatar" aria-hidden="true">
          {identity.avatarUrl ? <img src={identity.avatarUrl} alt="" /> : session ? <span>{avatarLabel}</span> : <UserRound size={19} />}
          <i data-online={Boolean(session)} />
        </span>
        <span>{t("userMenu.label")}</span>
      </button>
    </PopoverTrigger>
    <PopoverContent side="right" align="end" sideOffset={10} className="user-menu-popover">
      <header className="user-menu-identity">
        <span className="user-menu-avatar" aria-hidden="true">
          {identity.avatarUrl ? <img src={identity.avatarUrl} alt="" /> : session ? avatarLabel : <UserRound size={22} />}
        </span>
        <span className="user-menu-copy">
          <strong>{identity.displayName}</strong>
          <small>{identity.email ?? t("userMenu.localMode")}</small>
        </span>
        <span className="user-menu-status">{t(session ? "userMenu.synced" : "userMenu.localBadge")}</span>
      </header>
      <div className="user-menu-actions" role="menu">
        <button type="button" role="menuitem" onClick={() => openPanel(session ? onOpenAccount : onOpenAuth)}>
          {session ? <UserRound size={16} /> : <LogIn size={16} />}
          <span><strong>{t(session ? "userMenu.accountSettings" : "userMenu.signIn")}</strong><small>{t(session ? "userMenu.accountSettingsHint" : "userMenu.signInHint")}</small></span>
        </button>
        <button type="button" role="menuitem" onClick={() => openPanel(onOpenPricing)}>
          <CreditCard size={16} />
          <span><strong>{t("userMenu.subscription")}</strong><small>{t("userMenu.subscriptionHint")}</small></span>
        </button>
        <button type="button" role="menuitem" onClick={() => openPanel(onOpenSettings)}>
          <Settings size={16} />
          <span><strong>{t("userMenu.preferences")}</strong><small>{t("userMenu.preferencesHint")}</small></span>
        </button>
      </div>
      {session && <div className="user-menu-signout">
        <button type="button" onClick={() => void signOut()}><LogOut size={15} />{t("cloud.signOut")}</button>
        {signOutFailed && <p role="alert">{t("userMenu.signOutError")}</p>}
      </div>}
    </PopoverContent>
  </Popover>;
}
