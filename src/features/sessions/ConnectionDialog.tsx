import { zodResolver } from "@hookform/resolvers/zod";
import { LockKeyhole, ShieldCheck, X } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";
import { useForm } from "react-hook-form";
import { useTranslation } from "react-i18next";
import { z } from "zod";
import { Button } from "../../components/ui/button";
import { Input } from "../../components/ui/input";
import { cancelHostVerification, credentialStatus, forgetCredential, prepareHostVerification, trustHost, unlockVault } from "../../lib/tauri/ssh";
import type { ServerProfile } from "../../types/domain";
import type { CredentialInput, CredentialStatus, HostVerification } from "../../types/session";
import { createCredentialInput } from "./credential-input";

const schema = z.object({
  password: z.string(), remember: z.boolean(), masterPassword: z.string(), confirmMasterPassword: z.string(),
});
type Values = z.infer<typeof schema>;
type ConnectionValues = { profileId: string; verificationAttemptId: string; credential: CredentialInput };
type ConnectionAction = "connect" | "test";

function errorCode(error: unknown): string {
  return typeof error === "object" && error !== null && "code" in error && typeof error.code === "string" ? error.code : "UNKNOWN";
}

export function ConnectionDialog({ profile, mode, onClose, onConnect, onTest }: {
  profile: ServerProfile;
  mode: "connect" | "reconnect";
  onClose: () => void;
  onConnect: (values: ConnectionValues) => Promise<boolean>;
  onTest: (values: ConnectionValues) => Promise<boolean>;
}) {
  const { t } = useTranslation();
  const [verification, setVerification] = useState<HostVerification | null>(null);
  const [vault, setVault] = useState<CredentialStatus | null>(null);
  const [failure, setFailure] = useState<string | null>(null);
  const [saveWarning, setSaveWarning] = useState(false);
  const [testSucceeded, setTestSucceeded] = useState(false);
  const [busy, setBusy] = useState(false);
  const pendingCredential = useRef<CredentialInput | null>(null);
  const pendingAction = useRef<ConnectionAction>("connect");
  const { register, handleSubmit, getValues, reset, resetField, watch, formState: { errors } } = useForm<Values>({
    resolver: zodResolver(schema), defaultValues: { password: "", remember: false, masterPassword: "", confirmMasterPassword: "" },
  });
  const remember = watch("remember");
  const credentialKind = profile.authMethod === "password" ? "password" : "key-passphrase";

  const refreshVault = useCallback(async () => {
    const status = await credentialStatus(profile.id, credentialKind);
    setVault(status);
    return status;
  }, [credentialKind, profile.id]);
  useEffect(() => { void refreshVault().catch((error) => setFailure(errorCode(error))); }, [refreshVault]);

  const clearSecrets = () => {
    pendingCredential.current = null;
    reset({ password: "", remember: false, masterPassword: "", confirmMasterPassword: "" });
  };
  const runAttempt = async (attemptId: string, credential: CredentialInput, action: ConnectionAction) => {
    try {
      const values = { profileId: profile.id, verificationAttemptId: attemptId, credential };
      const credentialSaved = action === "test" ? await onTest(values) : await onConnect(values);
      clearSecrets();
      setVerification(null);
      if (action === "test") {
        setTestSucceeded(true);
        setSaveWarning(!credentialSaved);
      } else if (credentialSaved) onClose(); else setSaveWarning(true);
    } catch (error) {
      pendingCredential.current = null;
      setVerification(null);
      setFailure(errorCode(error));
    }
  };
  const beginVerification = async (credential: CredentialInput, action: ConnectionAction) => {
    setBusy(true); setFailure(null); setSaveWarning(false); setTestSucceeded(false); pendingCredential.current = credential; pendingAction.current = action;
    try {
      const prepared = await prepareHostVerification(profile.id);
      if (prepared.status === "trusted") await runAttempt(prepared.attemptId, credential, action); else setVerification(prepared);
    } catch (error) {
      pendingCredential.current = null; setFailure(errorCode(error));
    } finally { setBusy(false); }
  };
  const unlock = async () => {
    const masterPassword = getValues("masterPassword");
    if (!masterPassword) { setFailure("VAULT_PASSWORD_REQUIRED"); return null; }
    if (!vault?.vaultInitialized && masterPassword !== getValues("confirmMasterPassword")) { setFailure("VAULT_PASSWORD_MISMATCH"); return null; }
    setBusy(true); setFailure(null);
    try {
      await unlockVault(masterPassword);
      resetField("masterPassword"); resetField("confirmMasterPassword");
      return await refreshVault();
    } catch (error) { setFailure(errorCode(error)); return null; } finally { setBusy(false); }
  };
  const submitEnteredCredential = (action: ConnectionAction) => handleSubmit(async ({ password }) => {
    const credential = createCredentialInput(profile.authMethod, remember, password);
    if (!credential) { setFailure(profile.authMethod === "password" ? "INVALID_PROFILE" : "PASSPHRASE_REQUIRED"); return; }
    if (remember && !vault?.vaultUnlocked && !await unlock()) return;
    await beginVerification(credential, action);
  });
  const runWithStoredCredential = async (action: ConnectionAction) => {
    if (vault?.vaultUnlocked && vault.hasCredential) await beginVerification({ mode: "stored" }, action);
  };
  const forgetStoredPassword = async () => {
    setBusy(true); setFailure(null);
    try { await forgetCredential(profile.id, credentialKind); await refreshVault(); }
    catch (error) { setFailure(errorCode(error)); } finally { setBusy(false); }
  };
  const trustAndConnect = async (rememberHost: boolean) => {
    if (!verification || !pendingCredential.current) return;
    setBusy(true); setFailure(null);
    try { await trustHost(verification.attemptId, rememberHost); await runAttempt(verification.attemptId, pendingCredential.current, pendingAction.current); }
    catch (error) { setFailure(errorCode(error)); } finally { setBusy(false); }
  };
  const cancelVerification = () => {
    if (verification) void cancelHostVerification(verification.attemptId);
    pendingCredential.current = null; setVerification(null);
  };
  const close = () => {
    if (verification) void cancelHostVerification(verification.attemptId);
    clearSecrets(); onClose();
  };

  return <div className="fixed inset-0 z-50 grid place-items-center bg-slate-950/60 p-4" role="dialog" aria-modal="true" aria-labelledby="connection-title"><div className="w-full max-w-lg rounded-xl border bg-[hsl(var(--surface))] p-5 shadow-2xl">
    <div className="mb-5 flex items-center justify-between"><h2 id="connection-title" className="text-lg font-semibold">{verification ? t("connection.fingerprintTitle") : t(mode === "reconnect" ? "connection.reconnectTo" : "connection.connectTo", { name: profile.name })}</h2><Button variant="ghost" size="icon" aria-label={t("a11y.close")} onClick={close}><X size={18} /></Button></div>
    {!verification ? <form className="space-y-4" onSubmit={submitEnteredCredential("connect")}>
      <dl className="rounded-lg border bg-[hsl(var(--background))] p-4 text-sm"><dt className="text-[hsl(var(--muted))]">{t("connection.host")}</dt><dd className="mt-1 font-mono">{profile.host}:{profile.port}</dd><dt className="mt-3 text-[hsl(var(--muted))]">{t("connection.username")}</dt><dd className="mt-1 font-mono">{profile.username}</dd></dl>
      {vault?.vaultInitialized && !vault.vaultUnlocked && <div className="rounded-lg border p-3"><div className="flex items-center gap-2 text-sm font-medium"><LockKeyhole size={16} />{t("connection.vaultLocked")}</div><p className="mt-1 text-xs text-[hsl(var(--muted))]">{t("connection.unlockHint")}</p><label className="mt-3 block text-sm font-medium">{t("connection.vaultPassword")}<Input className="mt-1" type="password" autoComplete="current-password" {...register("masterPassword")} /></label><Button className="mt-3" type="button" variant="secondary" disabled={busy} onClick={() => void unlock()}>{t("connection.unlockVault")}</Button></div>}
      {vault?.vaultUnlocked && vault.hasCredential && <div className="flex flex-wrap gap-2"><Button className="flex-1" type="button" variant="secondary" disabled={busy} onClick={() => void runWithStoredCredential("connect")}><ShieldCheck size={16} />{t(profile.authMethod === "password" ? "connection.useSavedPassword" : "connection.useSavedPassphrase")}</Button><Button type="button" variant="secondary" disabled={busy} onClick={() => void runWithStoredCredential("test")}>{t("connection.testSaved")}</Button><Button type="button" variant="ghost" disabled={busy} onClick={() => void forgetStoredPassword()}>{t("connection.forgetSavedPassword")}</Button></div>}
      <div className="relative flex items-center"><span className="h-px flex-1 bg-[hsl(var(--border))]" /><span className="px-3 text-xs text-[hsl(var(--muted))]">{t(profile.authMethod === "password" ? "connection.orEnterPassword" : "connection.orEnterPassphrase")}</span><span className="h-px flex-1 bg-[hsl(var(--border))]" /></div>
      <label className="block text-sm font-medium">{t(profile.authMethod === "password" ? "connection.password" : "connection.passphrase")}<Input className="mt-1" type="password" autoComplete="off" aria-invalid={Boolean(errors.password)} autoFocus {...register("password")} /></label>
      <label className="flex items-start gap-2 text-sm"><input className="mt-1" type="checkbox" {...register("remember")} /><span><span className="font-medium">{t("connection.rememberSecurely")}</span><span className="mt-0.5 block text-xs text-[hsl(var(--muted))]">{t("connection.rememberHint")}</span></span></label>
      {remember && !vault?.vaultUnlocked && !vault?.vaultInitialized && <div className="rounded-lg border p-3"><p className="text-xs text-[hsl(var(--muted))]">{t("connection.createVaultHint")}</p><label className="mt-3 block text-sm font-medium">{t("connection.vaultPassword")}<Input className="mt-1" type="password" autoComplete="new-password" {...register("masterPassword")} /></label><label className="mt-3 block text-sm font-medium">{t("connection.confirmVaultPassword")}<Input className="mt-1" type="password" autoComplete="new-password" {...register("confirmMasterPassword")} /></label></div>}
      {!remember && <p className="text-xs text-[hsl(var(--muted))]">{t(profile.authMethod === "password" ? "connection.sessionOnlyPassword" : "connection.sessionOnlyPassphrase")}</p>}
      <div className="flex justify-end gap-2"><Button type="button" variant="ghost" onClick={close}>{t("connection.cancel")}</Button><Button type="button" variant="secondary" disabled={busy} onClick={() => void submitEnteredCredential("test")()}>{t("connection.test")}</Button><Button type="submit" disabled={busy}>{t(mode === "reconnect" ? "connection.scanReconnect" : "connection.scan")}</Button></div>
    </form> : <div><p className="mb-4 text-sm text-[hsl(var(--secondary))]">{t("connection.fingerprintHint")}</p><dl className="rounded-lg border bg-[hsl(var(--background))] p-4 text-sm"><dt className="text-[hsl(var(--muted))]">{t("connection.fingerprint")}</dt><dd className="mt-1 break-all font-mono">{verification.fingerprint}</dd><dt className="mt-3 text-[hsl(var(--muted))]">{t("connection.host")}</dt><dd className="mt-1 font-mono">{verification.host}:{verification.port} · {verification.keyType}</dd></dl><div className="mt-5 flex flex-wrap justify-end gap-2"><Button variant="ghost" onClick={cancelVerification}>{t("connection.cancel")}</Button><Button variant="secondary" disabled={busy} onClick={() => void trustAndConnect(false)}>{t("connection.trustOnce")}</Button><Button disabled={busy} onClick={() => void trustAndConnect(true)}>{t("connection.trustRemember")}</Button></div></div>}
    {testSucceeded && <div className="mt-4 rounded-md border border-emerald-500/40 bg-emerald-500/10 p-3 text-sm text-emerald-600"><div className="font-medium">{t("connection.testSuccessTitle")}</div><div className="mt-1 text-xs">{t("connection.testSuccess")}</div></div>}
    {saveWarning && <div className="mt-4 rounded-md border border-amber-500/40 bg-amber-500/10 p-3 text-sm text-amber-600"><div className="font-medium">{t(testSucceeded ? "connection.testSaveWarningTitle" : "connection.saveWarningTitle")}</div><div className="mt-1 text-xs">{t(testSucceeded ? "connection.testSaveWarning" : "connection.saveWarning")}</div>{!testSucceeded && <Button className="mt-3" size="sm" variant="secondary" onClick={onClose}>{t("connection.close")}</Button>}</div>}
    {failure && <div className="mt-4 rounded-md border border-red-500/40 bg-red-500/10 p-3 text-sm text-red-500"><div className="font-medium">{failure === "HOST_KEY_CHANGED" ? t("connection.changedTitle") : t("connection.error")}</div><div className="mt-1 text-xs">{t(`connection.errors.${failure}`, { defaultValue: t("connection.defaultError") })}</div></div>}
  </div></div>;
}
