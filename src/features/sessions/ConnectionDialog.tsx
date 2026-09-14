import { zodResolver } from "@hookform/resolvers/zod";
import { ArrowRight, KeyRound, LoaderCircle, LockKeyhole, Server, ShieldCheck, X } from "lucide-react";
import { useCallback, useEffect, useRef, useState, type CSSProperties } from "react";
import { useForm } from "react-hook-form";
import { useTranslation } from "react-i18next";
import { z } from "zod";
import { Button } from "../../components/ui/button";
import { Input } from "../../components/ui/input";
import { appErrorCode } from "../../lib/app-error";
import { cancelHostVerification, credentialStatus, forgetCredential, initializeVault, prepareHostVerification, trustHost, unlockVault, unlockVaultWithPlatform } from "../../lib/tauri/ssh";
import { vaultUnlockAction } from "../../lib/vault-unlock";
import type { ServerProfile } from "../../types/domain";
import type { CredentialInput, CredentialStatus, HostVerification } from "../../types/session";
import { OsLogo } from "../profiles/OsLogo";
import { osLogoDictionary } from "../profiles/os-logo-data";
import { ConnectionAttemptProgress, type ConnectionProgressStage } from "./ConnectionAttemptProgress";
import "./connection-dialog.css";
import { connectionOutcome, type ConnectionAction } from "./connection-outcome";
import { createCredentialInput } from "./credential-input";
import { useConnectionDialogFocus } from "./use-connection-dialog-focus";

const schema = z.object({
  password: z.string(), remember: z.boolean(), masterPassword: z.string(), confirmMasterPassword: z.string(),
});
type Values = z.infer<typeof schema>;
type ConnectionValues = { profileId: string; verificationAttemptId: string; credential: CredentialInput };

export function ConnectionDialog({ profile, mode, onClose, onConnect, onTest }: {
  profile: ServerProfile;
  mode: "connect" | "reconnect" | "tunnel";
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
  const [progressStage, setProgressStage] = useState<ConnectionProgressStage | null>(null);
  const [showManualCredential, setShowManualCredential] = useState(false);
  const pendingCredential = useRef<CredentialInput | null>(null);
  const pendingAction = useRef<ConnectionAction>("connect");
  const autoStoredAttempted = useRef(false);
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
  useEffect(() => {
    void refreshVault()
      .then((status) => {
        if (!status.vaultUnlocked || !status.hasCredential) setShowManualCredential(true);
      })
      .catch((error) => {
        setShowManualCredential(true);
        setFailure(appErrorCode(error));
      });
  }, [refreshVault]);

  const clearSecrets = useCallback(() => {
    pendingCredential.current = null;
    reset({ password: "", remember: false, masterPassword: "", confirmMasterPassword: "" });
  }, [reset]);
  const runAttempt = useCallback(async (attemptId: string, credential: CredentialInput, action: ConnectionAction) => {
    setProgressStage("connect");
    try {
      const values = { profileId: profile.id, verificationAttemptId: attemptId, credential };
      const credentialSaved = action === "test" ? await onTest(values) : await onConnect(values);
      const outcome = connectionOutcome(action, credential.mode, credentialSaved);
      clearSecrets();
      setVerification(null);
      setTestSucceeded(outcome.testSucceeded);
      setSaveWarning(outcome.saveWarning);
      if (outcome.closeDialog) onClose();
    } catch (error) {
      pendingCredential.current = null;
      setVerification(null);
      if (credential.mode === "stored") setShowManualCredential(true);
      setFailure(appErrorCode(error));
    }
  }, [clearSecrets, onClose, onConnect, onTest, profile.id]);
  const beginVerification = useCallback(async (credential: CredentialInput, action: ConnectionAction) => {
    setProgressStage("verify");
    setBusy(true); setFailure(null); setSaveWarning(false); setTestSucceeded(false); pendingCredential.current = credential; pendingAction.current = action;
    try {
      const prepared = await prepareHostVerification(profile.id);
      if (prepared.status === "trusted") await runAttempt(prepared.attemptId, credential, action); else setVerification(prepared);
    } catch (error) {
      pendingCredential.current = null; setFailure(appErrorCode(error));
    } finally { setBusy(false); setProgressStage(null); }
  }, [profile.id, runAttempt]);
  useEffect(() => {
    if (!vault?.vaultUnlocked || !vault.hasCredential || autoStoredAttempted.current) return;
    autoStoredAttempted.current = true;
    void beginVerification({ mode: "stored" }, "connect");
  }, [beginVerification, vault?.hasCredential, vault?.vaultUnlocked]);
  const unlockWithPassword = async () => {
    const masterPassword = getValues("masterPassword");
    if (!masterPassword) { setFailure("VAULT_PASSWORD_REQUIRED"); return null; }
    if (!vault?.vaultInitialized && masterPassword !== getValues("confirmMasterPassword")) { setFailure("VAULT_PASSWORD_MISMATCH"); return null; }
    setBusy(true); setFailure(null);
    try {
      await unlockVault(masterPassword);
      resetField("masterPassword"); resetField("confirmMasterPassword");
      return await refreshVault();
    } catch (error) { setFailure(appErrorCode(error)); return null; } finally { setBusy(false); }
  };
  const unlockOrInitialize = async () => {
    const action = vaultUnlockAction(vault);
    if (action === "none") return vault;
    if (action === "unlock-password") return unlockWithPassword();
    setBusy(true); setFailure(null);
    try {
      if (action === "unlock-platform") await unlockVaultWithPlatform(); else await initializeVault();
      return await refreshVault();
    } catch (error) { setFailure(appErrorCode(error)); return null; } finally { setBusy(false); }
  };
  const submitEnteredCredential = (action: ConnectionAction) => handleSubmit(async ({ password }) => {
    const credential = createCredentialInput(profile.authMethod, remember, password);
    if (!credential) { setFailure(profile.authMethod === "password" ? "INVALID_PROFILE" : "PASSPHRASE_REQUIRED"); return; }
    if (remember && !vault?.vaultUnlocked && !await unlockOrInitialize()) return;
    await beginVerification(credential, action);
  });
  const runWithStoredCredential = async (action: ConnectionAction) => {
    if (vault?.vaultUnlocked && vault.hasCredential) {
      setShowManualCredential(false);
      await beginVerification({ mode: "stored" }, action);
    }
  };
  const forgetStoredPassword = async () => {
    setBusy(true); setFailure(null);
    try { await forgetCredential(profile.id, credentialKind); await refreshVault(); setShowManualCredential(true); }
    catch (error) { setFailure(appErrorCode(error)); } finally { setBusy(false); }
  };
  const trustAndConnect = async (rememberHost: boolean) => {
    if (!verification || !pendingCredential.current) return;
    setBusy(true); setFailure(null);
    setProgressStage("verify");
    try { await trustHost(verification.attemptId, rememberHost); await runAttempt(verification.attemptId, pendingCredential.current, pendingAction.current); }
    catch (error) { setFailure(appErrorCode(error)); } finally { setBusy(false); setProgressStage(null); }
  };
  const cancelVerification = () => {
    if (verification) void cancelHostVerification(verification.attemptId);
    pendingCredential.current = null; setVerification(null);
  };
  const close = () => {
    if (busy) return;
    if (verification) void cancelHostVerification(verification.attemptId);
    clearSecrets(); onClose();
  };

  const connecting = busy && progressStage !== null;
  const dialogFocus = useConnectionDialogFocus(busy, close);
  const endpoint = `${profile.host.includes(":") && !profile.host.startsWith("[") ? `[${profile.host}]` : profile.host}:${profile.port}`;
  const osLogo = profile.osDistribution ? osLogoDictionary[profile.osDistribution] : null;

  return <div {...dialogFocus} tabIndex={-1} className="connection-modal" role="dialog" aria-modal="true" aria-labelledby="connection-title">
    <div className="connection-card">
      <header className="connection-card-header">
        <span
          className={osLogo ? "connection-server-icon has-os-logo" : "connection-server-icon"}
          style={osLogo ? { "--os-logo-color": osLogo.color } as CSSProperties : undefined}
          aria-hidden="true"
        >
          {profile.osDistribution && osLogo
            ? <OsLogo plain distribution={profile.osDistribution} state="idle" statusLabel={osLogo.label} />
            : <Server size={22} strokeWidth={1.5} />}
        </span>
        <div className="connection-card-title">
          <p>{t(verification && !connecting ? "connection.fingerprintTitle" : mode === "tunnel" ? "tunnels.connectTo" : "connection.title", { name: profile.name })}</p>
          <h2 id="connection-title">{profile.name}</h2>
        </div>
        <Button variant="ghost" size="icon" aria-label={t("a11y.close")} disabled={busy} onClick={close}><X size={17} /></Button>
      </header>
      <div className="connection-card-body">
        <dl className="connection-target">
          <div><dt>{t("connection.host")}</dt><dd title={endpoint}>{endpoint}</dd></div>
          <div><dt>{t("connection.username")}</dt><dd title={profile.username}>{profile.username}</dd></div>
          <div className="connection-target-auth"><dt><KeyRound size={13} aria-hidden="true" />{t("profile.authMethod")}</dt><dd>{t(profile.authMethod === "password" ? "connection.password" : "profile.privateKey")}</dd></div>
        </dl>
        {mode === "tunnel" && <p className="mb-4 text-xs text-[hsl(var(--secondary))]">{t("tunnels.backgroundHint")}</p>}
        {connecting && <ConnectionAttemptProgress stage={progressStage} testing={pendingAction.current === "test"} osDistribution={profile.osDistribution} />}
        <div hidden={connecting}>
    {!verification ? <form className="connection-credential-form space-y-4" aria-busy={busy} onSubmit={submitEnteredCredential("connect")}>
      {vault?.vaultInitialized && !vault.vaultUnlocked && vault.platformUnlockConfigured && vault.platformUnlockAvailable && <div className="rounded-lg border p-3"><div className="flex items-center gap-2 text-sm font-medium"><LockKeyhole size={16} />{t("connection.vaultLocked")}</div><p className="mt-1 text-xs text-[hsl(var(--muted))]">{t("connection.platformUnlockHint")}</p><Button className="mt-3" type="button" variant="secondary" disabled={busy} onClick={() => void unlockOrInitialize()}>{t("connection.unlockVault")}</Button></div>}
      {vault?.vaultInitialized && !vault.vaultUnlocked && (!vault.platformUnlockConfigured || !vault.platformUnlockAvailable) && <div className="rounded-lg border p-3"><div className="flex items-center gap-2 text-sm font-medium"><LockKeyhole size={16} />{t(vault.platformUnlockAvailable ? "connection.vaultMigrationRequired" : "connection.vaultLocked")}</div><p className="mt-1 text-xs text-[hsl(var(--muted))]">{t(vault.platformUnlockAvailable ? "connection.vaultMigrationHint" : "connection.unlockHint")}</p><label className="mt-3 block text-sm font-medium">{t("connection.vaultPassword")}<Input className="mt-1" type="password" autoComplete="current-password" {...register("masterPassword")} /></label><Button className="mt-3" type="button" variant="secondary" disabled={busy} onClick={() => void unlockWithPassword()}>{t(vault.platformUnlockAvailable ? "connection.migrateVault" : "connection.unlockVault")}</Button></div>}
      {vault === null && <p className="flex items-center gap-2 text-xs text-[hsl(var(--muted))]" role="status"><LoaderCircle className="animate-spin" size={14} />{t("connection.checkingSavedCredential")}</p>}
      {vault?.vaultUnlocked && vault.hasCredential && <div className="space-y-3 rounded-lg border p-3"><p className="flex items-center gap-2 text-sm font-medium"><ShieldCheck size={16} />{t(profile.authMethod === "password" ? "connection.savedPasswordReady" : "connection.savedPassphraseReady")}</p><div className="flex flex-wrap gap-2"><Button className="flex-1" type="button" variant="secondary" disabled={busy} onClick={() => void runWithStoredCredential("connect")}>{t(profile.authMethod === "password" ? "connection.useSavedPassword" : "connection.useSavedPassphrase")}</Button><Button type="button" variant="secondary" disabled={busy} onClick={() => void runWithStoredCredential("test")}>{t("connection.testSaved")}</Button><Button type="button" variant="ghost" disabled={busy} onClick={() => void forgetStoredPassword()}>{t("connection.forgetSavedPassword")}</Button><Button type="button" variant="ghost" disabled={busy} onClick={() => setShowManualCredential((visible) => !visible)}>{t(profile.authMethod === "password" ? "connection.useAnotherPassword" : "connection.useAnotherPassphrase")}</Button></div></div>}
      {showManualCredential && <>
        {vault?.hasCredential && <div className="relative flex items-center"><span className="h-px flex-1 bg-[hsl(var(--border))]" /><span className="px-3 text-xs text-[hsl(var(--muted))]">{t(profile.authMethod === "password" ? "connection.orEnterPassword" : "connection.orEnterPassphrase")}</span><span className="h-px flex-1 bg-[hsl(var(--border))]" /></div>}
        <label className="block text-sm font-medium">{t(profile.authMethod === "password" ? "connection.password" : "connection.passphrase")}<Input className="mt-1" type="password" autoComplete="off" aria-invalid={Boolean(errors.password)} autoFocus {...register("password")} /></label>
        <label className="flex items-start gap-2 text-sm"><input className="mt-1" type="checkbox" {...register("remember")} /><span><span className="font-medium">{t("connection.rememberSecurely")}</span><span className="mt-0.5 block text-xs text-[hsl(var(--muted))]">{t("connection.rememberHint")}</span></span></label>
        {remember && !vault?.vaultUnlocked && !vault?.vaultInitialized && vault?.platformUnlockAvailable && <div className="rounded-lg border p-3"><p className="text-xs text-[hsl(var(--muted))]">{t("connection.createPlatformVaultHint")}</p></div>}
        {remember && !vault?.vaultUnlocked && !vault?.vaultInitialized && !vault?.platformUnlockAvailable && <div className="rounded-lg border p-3"><p className="text-xs text-[hsl(var(--muted))]">{t("connection.createVaultHint")}</p><label className="mt-3 block text-sm font-medium">{t("connection.vaultPassword")}<Input className="mt-1" type="password" autoComplete="new-password" {...register("masterPassword")} /></label><label className="mt-3 block text-sm font-medium">{t("connection.confirmVaultPassword")}<Input className="mt-1" type="password" autoComplete="new-password" {...register("confirmMasterPassword")} /></label></div>}
        {!remember && <p className="text-xs text-[hsl(var(--muted))]">{t(profile.authMethod === "password" ? "connection.sessionOnlyPassword" : "connection.sessionOnlyPassphrase")}</p>}
      </>}
      <div className="connection-form-actions"><Button type="button" variant="ghost" disabled={busy} onClick={close}>{t("connection.cancel")}</Button>{showManualCredential && <><Button type="button" variant="secondary" disabled={busy} onClick={() => void submitEnteredCredential("test")()}>{t("connection.test")}</Button><Button type="submit" disabled={busy}>{t(mode === "tunnel" ? "tunnels.authenticateStart" : mode === "reconnect" ? "connection.scanReconnect" : "connection.scan")}<ArrowRight size={14} aria-hidden="true" /></Button></>}</div>
    </form> : <div><p className="mb-4 text-sm text-[hsl(var(--secondary))]">{t("connection.fingerprintHint")}</p><dl className="rounded-lg border bg-[hsl(var(--background))] p-4 text-sm"><dt className="text-[hsl(var(--muted))]">{t("connection.fingerprint")}</dt><dd className="mt-1 break-all font-mono">{verification.fingerprint}</dd><dt className="mt-3 text-[hsl(var(--muted))]">{t("connection.host")}</dt><dd className="mt-1 font-mono">{verification.host}:{verification.port} · {verification.keyType}</dd></dl><div className="mt-5 flex flex-wrap justify-end gap-2"><Button variant="ghost" disabled={busy} onClick={cancelVerification}>{t("connection.cancel")}</Button><Button variant="secondary" disabled={busy} onClick={() => void trustAndConnect(false)}>{t("connection.trustOnce")}</Button><Button disabled={busy} onClick={() => void trustAndConnect(true)}>{t("connection.trustRemember")}</Button></div></div>}
        </div>
        {busy && !connecting && <p className="connection-vault-progress" role="status"><LoaderCircle size={14} aria-hidden="true" />{t("connection.progress.vault")}</p>}
    {testSucceeded && <div className="mt-4 rounded-md border border-emerald-500/40 bg-emerald-500/10 p-3 text-sm text-emerald-600"><div className="font-medium">{t("connection.testSuccessTitle")}</div><div className="mt-1 text-xs">{t("connection.testSuccess")}</div></div>}
    {saveWarning && <div className="mt-4 rounded-md border border-amber-500/40 bg-amber-500/10 p-3 text-sm text-amber-600"><div className="font-medium">{t(testSucceeded ? "connection.testSaveWarningTitle" : "connection.saveWarningTitle")}</div><div className="mt-1 text-xs">{t(testSucceeded ? "connection.testSaveWarning" : "connection.saveWarning")}</div>{!testSucceeded && <Button className="mt-3" size="sm" variant="secondary" onClick={onClose}>{t("connection.close")}</Button>}</div>}
    {failure && <div role="alert" className="mt-4 rounded-md border border-red-500/40 bg-red-500/10 p-3 text-sm text-red-500"><div className="font-medium">{failure === "HOST_KEY_CHANGED" ? t("connection.changedTitle") : t("connection.error")}</div><div className="mt-1 text-xs">{t(`connection.errors.${failure}`, { defaultValue: t(`errors.${failure}`, { defaultValue: t("connection.defaultError") }) })}</div></div>}
      </div>
      <footer className="connection-card-footer"><LockKeyhole size={13} aria-hidden="true" /><span>{t(connecting ? pendingAction.current === "test" ? "connection.progress.testFooter" : mode === "tunnel" ? "connection.progress.tunnelFooter" : "connection.progress.footer" : "connection.progress.security")}</span></footer>
    </div>
  </div>;
}
