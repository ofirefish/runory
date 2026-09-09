import type { Session } from "@supabase/supabase-js";
import { ImagePlus, Save, Trash2, UserCircle } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { Button } from "../../components/ui/button";
import { Input } from "../../components/ui/input";
import {
  cloudProfileUpdatedEvent,
  deleteCloudAvatar,
  loadMyCloudProfile,
  setMyCloudAvatar,
  updateMyCloudDisplayName,
  uploadCloudAvatar,
} from "../../lib/supabase/cloud";
import { clearCloudAvatarCache, loadCloudAvatar } from "../../lib/cloud-avatar";
import type { CloudUserProfile } from "../../types/cloud";
import { prepareCloudAvatar } from "./cloud-account";

export function CloudAccountPanel({ session }: { session: Session }) {
  const { t } = useTranslation();
  const fileInput = useRef<HTMLInputElement>(null);
  const [profile, setProfile] = useState<CloudUserProfile | null>(null);
  const [displayName, setDisplayName] = useState("");
  const [avatarUrl, setAvatarUrl] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const showProfileError = useCallback(() => {
    toast.error(t("cloud.avatarError"), { id: "cloud-profile-error" });
  }, [t]);

  const refreshAvatarUrl = async (path: string | null, version: number) => {
    setAvatarUrl(path ? await loadCloudAvatar(session.user.id, path, version) : null);
  };

  useEffect(() => {
    let active = true;
    void loadMyCloudProfile(session.user.id).then(async (value) => {
      if (!active || !value) return;
      setProfile(value);
      setDisplayName(value.display_name);
      const url = value.avatar_path
        ? await loadCloudAvatar(session.user.id, value.avatar_path, value.avatar_version)
        : null;
      if (active) setAvatarUrl(url);
    }).catch(() => { if (active) showProfileError(); });
    return () => { active = false; };
  }, [session.user.id, showProfileError]);

  const saveDisplayName = async () => {
    const normalized = displayName.trim();
    if (!normalized || normalized.length > 64) return;
    setBusy(true);
    try {
      const next = await updateMyCloudDisplayName(normalized);
      setProfile(next); setDisplayName(next.display_name);
      window.dispatchEvent(new Event(cloudProfileUpdatedEvent));
      toast.success(t("cloud.profileSaved"));
    } catch { showProfileError(); } finally { setBusy(false); }
  };

  const uploadAvatar = async (file: File) => {
    if (!profile) return;
    setBusy(true);
    let uploadedPath: string | null = null;
    try {
      const blob = await prepareCloudAvatar(file);
      uploadedPath = await uploadCloudAvatar(session.user.id, blob);
      const next = await setMyCloudAvatar(uploadedPath, profile.avatar_version);
      const oldPath = profile.avatar_path;
      setProfile(next);
      await refreshAvatarUrl(next.avatar_path, next.avatar_version);
      if (oldPath) void deleteCloudAvatar(oldPath).catch(() => undefined);
      window.dispatchEvent(new Event(cloudProfileUpdatedEvent));
      toast.success(t("cloud.profileSaved"));
    } catch {
      if (uploadedPath) void deleteCloudAvatar(uploadedPath).catch(() => undefined);
      showProfileError();
    } finally {
      if (fileInput.current) fileInput.current.value = "";
      setBusy(false);
    }
  };

  const removeAvatar = async () => {
    if (!profile?.avatar_path) return;
    setBusy(true);
    const oldPath = profile.avatar_path;
    try {
      const next = await setMyCloudAvatar(null, profile.avatar_version);
      setProfile(next); setAvatarUrl(null);
      void clearCloudAvatarCache(session.user.id).catch(() => undefined);
      void deleteCloudAvatar(oldPath).catch(() => undefined);
      window.dispatchEvent(new Event(cloudProfileUpdatedEvent));
      toast.success(t("cloud.profileSaved"));
    } catch { showProfileError(); } finally { setBusy(false); }
  };

  return <section className="settings-card">
    <h4>{t("cloud.accountTitle")}</h4>
    <div className="mt-3 flex items-center gap-3">
      <div className="flex h-14 w-14 shrink-0 items-center justify-center overflow-hidden rounded-full border bg-[hsl(var(--surface-raised))]" aria-label={t("cloud.avatar")}>
        {avatarUrl ? <img src={avatarUrl} alt={t("cloud.avatar")} className="h-full w-full object-cover" /> : <UserCircle size={34} className="text-[hsl(var(--muted))]" />}
      </div>
      <div className="min-w-0 flex-1">
        <p className="truncate text-xs font-medium">{session.user.email}</p>
        <p className="mt-1 text-xs text-[hsl(var(--muted))]">{t(session.user.email_confirmed_at ? "cloud.emailVerified" : "cloud.emailUnverified")}</p>
      </div>
    </div>
    <div className="mt-3 grid gap-2">
      <Input value={displayName} maxLength={64} onChange={(event) => setDisplayName(event.target.value)} placeholder={t("cloud.displayName")} aria-label={t("cloud.displayName")} />
      <div className="flex flex-wrap gap-2">
        <Button size="sm" variant="secondary" disabled={busy || !displayName.trim()} onClick={() => void saveDisplayName()}><Save size={14} />{t("cloud.saveProfile")}</Button>
        <input ref={fileInput} className="hidden" type="file" accept="image/jpeg,image/png,image/webp" onChange={(event) => { const file = event.target.files?.[0]; if (file) void uploadAvatar(file); }} />
        <Button size="sm" variant="secondary" disabled={busy || !profile} onClick={() => fileInput.current?.click()}><ImagePlus size={14} />{t("cloud.avatarUpload")}</Button>
        {profile?.avatar_path && <Button size="sm" variant="ghost" disabled={busy} onClick={() => void removeAvatar()}><Trash2 size={14} />{t("cloud.avatarRemove")}</Button>}
      </div>
      <p className="text-xs text-[hsl(var(--muted))]">{t("cloud.avatarHint")}</p>
    </div>
  </section>;
}
