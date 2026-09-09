import type { Session } from "@supabase/supabase-js";
import { Github, LogOut } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { Button } from "../../components/ui/button";
import { Input } from "../../components/ui/input";
import { cloudConfigured, cloudEndpoint, cloudPublishableKey } from "../../lib/supabase/client";
import {
  cloudOAuthErrorEvent,
  cloudSession,
  cloudSignIn,
  cloudSignInWithGitHub,
  cloudSignInWithGoogle,
  cloudSignOut,
  cloudSignUp,
  ensureMyCloudProfile,
  ensurePersonalWorkspace,
  listOrganizations,
  onCloudAuthStateChange,
  requestCloudPasswordReset,
} from "../../lib/supabase/cloud";
import { lockCloudPolicy, refreshCloudPolicy } from "../../lib/tauri/cloud-policy";
import { cloudSessionPersistenceChangedEvent, cloudSessionPersistenceFailed } from "../../lib/supabase/session-persistence";
import { CloudAccountPanel } from "./CloudAccountPanel";

function GoogleLogo() {
  return <svg data-brand-icon="google" aria-hidden="true" viewBox="0 0 48 48" className="h-[15px] w-[15px] shrink-0">
    <path fill="#FFC107" d="M43.61 20H24v8h11.3C33.65 32.66 29.22 36 24 36c-6.63 0-12-5.37-12-12s5.37-12 12-12c3.06 0 5.84 1.15 7.96 3.04l5.66-5.66C34.05 6.05 29.27 4 24 4 12.95 4 4 12.95 4 24s8.95 20 20 20 20-8.95 20-20c0-1.34-.14-2.65-.39-4Z" />
    <path fill="#FF3D00" d="m6.31 14.69 6.57 4.82C14.66 15.11 18.96 12 24 12c3.06 0 5.84 1.15 7.96 3.04l5.66-5.66C34.05 6.05 29.27 4 24 4c-7.68 0-14.35 4.34-17.69 10.69Z" />
    <path fill="#4CAF50" d="M24 44c5.17 0 9.86-1.98 13.41-5.19l-6.19-5.24A11.9 11.9 0 0 1 24 36c-5.2 0-9.62-3.32-11.28-7.95l-6.52 5.02A20 20 0 0 0 24 44Z" />
    <path fill="#1976D2" d="M43.61 20H24v8h11.3a12.03 12.03 0 0 1-4.09 5.58l6.19 5.24C36.97 39.21 44 34 44 24c0-1.34-.14-2.65-.39-4Z" />
  </svg>;
}

async function refreshPolicyCredentials(session: Session) {
  if (!session.expires_at || !cloudEndpoint || !cloudPublishableKey) return;
  await refreshCloudPolicy({ supabaseUrl: cloudEndpoint, publishableKey: cloudPublishableKey, accessToken: session.access_token, expiresAt: session.expires_at });
}

export function AccountSettings({ onOpenCloud = () => undefined, onAuthenticated }: { onOpenCloud?: () => void; onAuthenticated?: (session: Session) => void }) {
  const { t } = useTranslation();
  const email = useRef<HTMLInputElement>(null);
  const password = useRef<HTMLInputElement>(null);
  const [session, setSession] = useState<Session | null>(null);
  const [busy, setBusy] = useState(false);
  const [workspaceFailed, setWorkspaceFailed] = useState(false);
  const [authValidationError, setAuthValidationError] = useState<string | null>(null);
  const [oauthPending, setOAuthPending] = useState(false);
  const [workspaceCount, setWorkspaceCount] = useState<number | null>(null);
  const [persistenceFailed, setPersistenceFailed] = useState(cloudSessionPersistenceFailed);
  const showAccountError = useCallback(() => {
    toast.error(t("cloud.accountError"), { id: "cloud-account-error" });
  }, [t]);

  const bootstrapAccount = useCallback(async (nextSession: Session) => {
    const fallbackName = nextSession.user.email?.split("@")[0]?.slice(0, 64) || nextSession.user.id.slice(0, 8);
    await Promise.all([ensureMyCloudProfile(fallbackName), ensurePersonalWorkspace()]);
    const organizations = await listOrganizations();
    setWorkspaceCount(organizations.length);
    setWorkspaceFailed(false);
  }, []);

  const acceptSession = useCallback((nextSession: Session) => {
    setSession(nextSession);
    setOAuthPending(false);
    onAuthenticated?.(nextSession);
  }, [onAuthenticated]);

  useEffect(() => {
    if (!cloudConfigured) return;
    void cloudSession().then((value) => {
      if (!value) return;
      acceptSession(value);
      void bootstrapAccount(value).catch(() => setWorkspaceFailed(true));
    }).catch(showAccountError);
  }, [acceptSession, bootstrapAccount, showAccountError]);

  useEffect(() => {
    if (!cloudConfigured) return;
    const subscription = onCloudAuthStateChange((event, nextSession) => {
      if (event === "SIGNED_OUT") {
        setSession(null);
        queueMicrotask(() => void lockCloudPolicy());
      } else if (nextSession && (event === "SIGNED_IN" || event === "TOKEN_REFRESHED" || event === "INITIAL_SESSION")) {
        acceptSession(nextSession);
        queueMicrotask(() => void refreshPolicyCredentials(nextSession).catch(() => undefined));
        if (event === "SIGNED_IN") queueMicrotask(() => void bootstrapAccount(nextSession).catch(() => setWorkspaceFailed(true)));
      }
    });
    return () => subscription.unsubscribe();
  }, [acceptSession, bootstrapAccount]);

  useEffect(() => {
    const onOAuthError = () => { setOAuthPending(false); showAccountError(); };
    window.addEventListener(cloudOAuthErrorEvent, onOAuthError);
    return () => window.removeEventListener(cloudOAuthErrorEvent, onOAuthError);
  }, [showAccountError]);

  useEffect(() => {
    const onPersistenceChange = () => setPersistenceFailed(cloudSessionPersistenceFailed());
    window.addEventListener(cloudSessionPersistenceChangedEvent, onPersistenceChange);
    return () => window.removeEventListener(cloudSessionPersistenceChangedEvent, onPersistenceChange);
  }, []);

  const authenticate = async (signup: boolean) => {
    const emailValue = email.current?.value.trim();
    const passwordValue = password.current?.value;
    if (!emailValue) { setAuthValidationError("cloud.authEmailRequired"); email.current?.focus(); return; }
    if (!email.current?.validity.valid) { setAuthValidationError("cloud.authEmailInvalid"); email.current?.focus(); return; }
    if (!passwordValue) { setAuthValidationError("cloud.authPasswordRequired"); password.current?.focus(); return; }
    if (signup && passwordValue.length < 8) { setAuthValidationError("cloud.authPasswordTooShort"); password.current?.focus(); return; }
    setAuthValidationError(null); setBusy(true);
    try {
      if (signup) {
        const user = await cloudSignUp(emailValue, passwordValue);
        if (user) toast.success(t("cloud.confirmEmail"));
      } else {
        const nextSession = await cloudSignIn(emailValue, passwordValue);
        acceptSession(nextSession);
        void refreshPolicyCredentials(nextSession).catch(() => undefined);
      }
    } catch { showAccountError(); } finally {
      if (password.current) password.current.value = "";
      setBusy(false);
    }
  };

  const requestPasswordReset = async () => {
    const emailValue = email.current?.value.trim();
    if (!emailValue) { setAuthValidationError("cloud.authEmailRequired"); email.current?.focus(); return; }
    setBusy(true);
    try { await requestCloudPasswordReset(emailValue); toast.success(t("cloud.resetPasswordSent")); }
    catch { showAccountError(); } finally { setBusy(false); }
  };

  const signInWithProvider = async (authenticate: () => Promise<void>) => {
    setBusy(true); setAuthValidationError(null); setOAuthPending(false);
    try { await authenticate(); setOAuthPending(true); }
    catch { showAccountError(); } finally { setBusy(false); }
  };

  const signOut = async () => {
    setBusy(true);
    try { await lockCloudPolicy(); await cloudSignOut(); setSession(null); toast.success(t("cloud.signedOut")); }
    catch { showAccountError(); } finally { setBusy(false); }
  };

  if (!cloudConfigured) return <section className="settings-card"><h4>{t("cloud.accountTitle")}</h4><p>{t("cloud.accountUnavailable")}</p><p className="mt-3 text-xs text-[hsl(var(--muted))]">{t("cloud.localOnly")}</p></section>;

  if (session) return <div className="settings-stack">
    <CloudAccountPanel session={session} />
    <section className="settings-card">
      <h4>{t("cloud.workspaceSummaryTitle")}</h4>
      <p>{workspaceFailed ? t("cloud.workspaceSummaryError") : t("cloud.workspaceSummary", { count: workspaceCount ?? 0 })}</p>
      <Button className="mt-3" size="sm" variant="secondary" onClick={onOpenCloud}>{t("cloud.manageWorkspaces")}</Button>
    </section>
    <section className="settings-card">
      <h4>{t("cloud.sessionTitle")}</h4>
      <p>{t("cloud.signedInAs", { email: session.user.email })}</p>
      <Button className="mt-3" size="sm" variant="secondary" disabled={busy} onClick={() => void signOut()}><LogOut size={14} />{t("cloud.signOut")}</Button>
    </section>
    {persistenceFailed && <p role="alert" className="text-xs text-amber-600">{t("cloud.sessionPersistenceError")}</p>}
  </div>;

  return <div className="settings-card">
    <h4>{t("cloud.signInTitle")}</h4>
    <p>{t("cloud.signInHint")}</p>
    <div className="mt-3 grid gap-2">
      <Button type="button" variant="secondary" disabled={busy || oauthPending} onClick={() => void signInWithProvider(cloudSignInWithGoogle)}><GoogleLogo />{t("cloud.signInWithGoogle")}</Button>
      <Button type="button" variant="secondary" disabled={busy || oauthPending} onClick={() => void signInWithProvider(cloudSignInWithGitHub)}><Github data-brand-icon="github" size={15} aria-hidden="true" />{t("cloud.signInWithGitHub")}</Button>
      {oauthPending && <p role="status" className="text-xs text-[hsl(var(--muted))]">{t("cloud.oauthPending")}</p>}
      <div className="flex items-center gap-3 text-xs text-[hsl(var(--muted))]" aria-hidden="true"><span className="h-px flex-1 bg-[hsl(var(--border))]" /><span>{t("cloud.orUseEmail")}</span><span className="h-px flex-1 bg-[hsl(var(--border))]" /></div>
      <Input ref={email} type="email" autoComplete="email" required placeholder={t("cloud.email")} aria-label={t("cloud.email")} aria-invalid={authValidationError === "cloud.authEmailRequired" || authValidationError === "cloud.authEmailInvalid"} onInput={() => setAuthValidationError(null)} />
      <Input ref={password} type="password" autoComplete="current-password" required minLength={8} maxLength={128} placeholder={t("cloud.password")} aria-label={t("cloud.password")} aria-invalid={authValidationError === "cloud.authPasswordRequired" || authValidationError === "cloud.authPasswordTooShort"} onInput={() => setAuthValidationError(null)} />
      <div className="flex flex-wrap gap-2"><Button type="button" size="sm" disabled={busy} onClick={() => void authenticate(false)}>{t("cloud.signIn")}</Button><Button type="button" size="sm" variant="secondary" disabled={busy} onClick={() => void authenticate(true)}>{t("cloud.signUp")}</Button><Button type="button" size="sm" variant="ghost" disabled={busy} onClick={() => void requestPasswordReset()}>{t("cloud.forgotPassword")}</Button></div>
      {authValidationError && <p role="alert" className="text-xs text-red-500">{t(authValidationError)}</p>}
    </div>
  </div>;
}
