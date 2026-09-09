import { zodResolver } from "@hookform/resolvers/zod";
import { ArrowRight, KeyRound, LockKeyhole, Network, Server, ShieldCheck, X } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";
import { useForm } from "react-hook-form";
import { useTranslation } from "react-i18next";
import { z } from "zod";
import { Button } from "../../components/ui/button";
import { Input } from "../../components/ui/input";
import { appErrorCode } from "../../lib/app-error";
import {
  cancelHostVerification,
  cancelJumpConnection,
  credentialStatus,
  initializeVault,
  prepareHostVerification,
  prepareJumpConnection,
  trustHost,
  unlockVault,
  unlockVaultWithPlatform,
} from "../../lib/tauri/ssh";
import { vaultUnlockAction } from "../../lib/vault-unlock";
import type { ServerProfile } from "../../types/domain";
import type { CredentialInput, CredentialStatus, HostVerification } from "../../types/session";
import { JumpConnectionProgress, type JumpConnectionProgressStage } from "./JumpConnectionProgress";
import "./connection-dialog.css";
import { connectionOutcome, type ConnectionAction } from "./connection-outcome";
import { createCredentialInput } from "./credential-input";
import { useConnectionDialogFocus } from "./use-connection-dialog-focus";

const schema = z.object({
  jumpSecret: z.string(),
  targetSecret: z.string(),
  rememberJump: z.boolean(),
  rememberTarget: z.boolean(),
  masterPassword: z.string(),
  confirmMasterPassword: z.string(),
});
type Values = z.infer<typeof schema>;
type ConnectionValues = {
  profileId: string;
  verificationAttemptId: string;
  credential: CredentialInput;
  jumpPreparationId?: string;
};
type Leg = "jump" | "target";

export function JumpConnectionDialog({ profile, jumpProfile, mode, onClose, onConnect, onTest }: {
  profile: ServerProfile;
  jumpProfile: ServerProfile;
  mode: "connect" | "reconnect";
  onClose: () => void;
  onConnect: (values: ConnectionValues) => Promise<boolean>;
  onTest: (values: ConnectionValues) => Promise<boolean>;
}) {
  const { t } = useTranslation();
  const [statuses, setStatuses] = useState<Record<Leg, CredentialStatus> | null>(null);
  const [useStored, setUseStored] = useState<Record<Leg, boolean>>({ jump: false, target: false });
  const [verification, setVerification] = useState<{ leg: Leg; value: HostVerification } | null>(null);
  const [failure, setFailure] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [progressStage, setProgressStage] = useState<JumpConnectionProgressStage | null>(null);
  const [saveWarning, setSaveWarning] = useState(false);
  const [testSucceeded, setTestSucceeded] = useState(false);
  const pendingCredentials = useRef<Record<Leg, CredentialInput> | null>(null);
  const pendingAction = useRef<ConnectionAction>("connect");
  const preparationId = useRef<string | null>(null);
  const jumpSaveFailed = useRef(false);
  const autoAttempted = useRef(false);
  const { register, handleSubmit, getValues, reset, resetField, watch } = useForm<Values>({
    resolver: zodResolver(schema),
    defaultValues: { jumpSecret: "", targetSecret: "", rememberJump: false, rememberTarget: false, masterPassword: "", confirmMasterPassword: "" },
  });
  const rememberJump = watch("rememberJump");
  const rememberTarget = watch("rememberTarget");

  const refreshStatuses = useCallback(async () => {
    const [jump, target] = await Promise.all([
      credentialStatus(jumpProfile.id, jumpProfile.authMethod === "password" ? "password" : "key-passphrase"),
      credentialStatus(profile.id, profile.authMethod === "password" ? "password" : "key-passphrase"),
    ]);
    const next = { jump, target };
    setStatuses(next);
    setUseStored({ jump: jump.vaultUnlocked && jump.hasCredential, target: target.vaultUnlocked && target.hasCredential });
    return next;
  }, [jumpProfile.authMethod, jumpProfile.id, profile.authMethod, profile.id]);

  useEffect(() => { void refreshStatuses().catch((error) => setFailure(appErrorCode(error))); }, [refreshStatuses]);

  const clearAttempt = useCallback(() => {
    if (verification) void cancelHostVerification(verification.value.attemptId);
    if (preparationId.current) void cancelJumpConnection(preparationId.current);
    preparationId.current = null;
    pendingCredentials.current = null;
    setVerification(null);
  }, [verification]);

  const unlockVaultForCredentials = async () => {
    const status = statuses?.target ?? null;
    const action = vaultUnlockAction(status);
    if (action === "none") return true;
    setBusy(true); setFailure(null);
    try {
      if (action === "unlock-platform") await unlockVaultWithPlatform();
      else if (action === "initialize-platform") await initializeVault();
      else {
        const password = getValues("masterPassword");
        if (!password) { setFailure("VAULT_PASSWORD_REQUIRED"); return false; }
        if (!status?.vaultInitialized && password !== getValues("confirmMasterPassword")) { setFailure("VAULT_PASSWORD_MISMATCH"); return false; }
        await unlockVault(password);
      }
      resetField("masterPassword"); resetField("confirmMasterPassword");
      await refreshStatuses();
      return true;
    } catch (error) { setFailure(appErrorCode(error)); return false; }
    finally { setBusy(false); }
  };

  const finalize = useCallback(async (targetAttemptId: string) => {
    const credentials = pendingCredentials.current;
    const ticket = preparationId.current;
    if (!credentials || !ticket) return;
    setProgressStage("connect");
    setBusy(true); setFailure(null);
    try {
      const values = { profileId: profile.id, verificationAttemptId: targetAttemptId, credential: credentials.target, jumpPreparationId: ticket };
      const credentialSaved = pendingAction.current === "test" ? await onTest(values) : await onConnect(values);
      preparationId.current = null;
      const outcome = connectionOutcome(pendingAction.current, credentials.target.mode, credentialSaved);
      const warning = outcome.saveWarning || jumpSaveFailed.current;
      setTestSucceeded(outcome.testSucceeded);
      setSaveWarning(warning);
      pendingCredentials.current = null;
      reset();
      if (pendingAction.current === "connect" && !warning) onClose();
    } catch (error) {
      clearAttempt();
      setFailure(appErrorCode(error));
    } finally { setBusy(false); setProgressStage(null); }
  }, [clearAttempt, onClose, onConnect, onTest, profile.id, reset]);

  const prepareTarget = useCallback(async (jumpAttemptId: string) => {
    const credentials = pendingCredentials.current;
    if (!credentials) return;
    setProgressStage("route");
    setBusy(true); setFailure(null);
    try {
      const prepared = await prepareJumpConnection(profile.id, jumpAttemptId, credentials.jump);
      preparationId.current = prepared.preparationId;
      jumpSaveFailed.current = credentials.jump.mode === "remember-securely" && !prepared.jumpCredentialSaved;
      if (prepared.targetVerification.status === "trusted") await finalize(prepared.targetVerification.attemptId);
      else setVerification({ leg: "target", value: prepared.targetVerification });
    } catch (error) {
      clearAttempt();
      if (credentials.jump.mode === "stored") setUseStored((current) => ({ ...current, jump: false }));
      setFailure(appErrorCode(error));
    } finally { setBusy(false); setProgressStage(null); }
  }, [clearAttempt, finalize, profile.id]);

  const begin = useCallback(async (credentials: Record<Leg, CredentialInput>, action: ConnectionAction) => {
    clearAttempt();
    pendingCredentials.current = credentials;
    pendingAction.current = action;
    jumpSaveFailed.current = false;
    setProgressStage("jump");
    setBusy(true); setFailure(null); setSaveWarning(false); setTestSucceeded(false);
    try {
      const prepared = await prepareHostVerification(jumpProfile.id);
      if (prepared.status === "trusted") await prepareTarget(prepared.attemptId);
      else setVerification({ leg: "jump", value: prepared });
    } catch (error) {
      pendingCredentials.current = null;
      setFailure(appErrorCode(error));
    } finally { setBusy(false); setProgressStage(null); }
  }, [clearAttempt, jumpProfile.id, prepareTarget]);

  const submit = (action: ConnectionAction) => handleSubmit(async (values) => {
    if ((values.rememberJump || values.rememberTarget) && !statuses?.target.vaultUnlocked && !await unlockVaultForCredentials()) return;
    const jump = useStored.jump ? { mode: "stored" as const } : createCredentialInput(jumpProfile.authMethod, values.rememberJump, values.jumpSecret);
    const target = useStored.target ? { mode: "stored" as const } : createCredentialInput(profile.authMethod, values.rememberTarget, values.targetSecret);
    if (!jump || !target) { setFailure("INVALID_PROFILE"); return; }
    await begin({ jump, target }, action);
  });

  useEffect(() => {
    if (!statuses || autoAttempted.current || !useStored.jump || !useStored.target) return;
    autoAttempted.current = true;
    void begin({ jump: { mode: "stored" }, target: { mode: "stored" } }, "connect");
  }, [begin, statuses, useStored.jump, useStored.target]);

  const trust = async (remember: boolean) => {
    if (!verification) return;
    setProgressStage(verification.leg === "jump" ? "route" : "connect");
    setBusy(true); setFailure(null);
    try {
      await trustHost(verification.value.attemptId, remember);
      const current = verification;
      setVerification(null);
      if (current.leg === "jump") await prepareTarget(current.value.attemptId);
      else await finalize(current.value.attemptId);
    } catch (error) { setFailure(appErrorCode(error)); }
    finally { setBusy(false); setProgressStage(null); }
  };

  const close = () => { if (busy) return; clearAttempt(); reset(); onClose(); };
  const dialogFocus = useConnectionDialogFocus(busy, close);
  const connecting = busy && progressStage !== null;
  const vault = statuses?.target;
  const credentialField = (leg: Leg, current: ServerProfile) => {
    const stored = statuses?.[leg].hasCredential && statuses[leg].vaultUnlocked;
    const field = leg === "jump" ? "jumpSecret" : "targetSecret";
    const rememberField = leg === "jump" ? "rememberJump" : "rememberTarget";
    return <section className="jump-credential-card" data-leg={leg}>
      <header className="jump-credential-header">
        <span aria-hidden="true">{leg === "jump" ? <Network size={17} /> : <Server size={17} />}</span>
        <div><small>{t(leg === "jump" ? "connection.jumpProgress.jumpHost" : "connection.jumpProgress.targetHost")}</small><strong>{current.name}</strong></div>
        <span className="jump-auth-method"><KeyRound size={12} />{t(current.authMethod === "password" ? "connection.password" : "profile.privateKey")}</span>
      </header>
      <p className="jump-credential-endpoint">{current.username}@{current.host}:{current.port}</p>
      {stored && useStored[leg] ? <div className="jump-stored-credential"><span><ShieldCheck size={15} />{t("connection.savedCredentialSelected")}</span><Button type="button" size="sm" variant="ghost" onClick={() => setUseStored((value) => ({ ...value, [leg]: false }))}>{t(current.authMethod === "password" ? "connection.useAnotherPassword" : "connection.useAnotherPassphrase")}</Button></div> : <div className="jump-manual-credential"><label>{t(current.authMethod === "password" ? "connection.password" : "connection.passphrase")}<Input type="password" autoComplete="off" {...register(field)} /></label><label className="jump-remember"><input type="checkbox" {...register(rememberField)} /><span><strong>{t("connection.rememberSecurely")}</strong><small>{t("connection.rememberHint")}</small></span></label>{stored && <Button type="button" size="sm" variant="ghost" onClick={() => setUseStored((value) => ({ ...value, [leg]: true }))}>{t("connection.useSavedCredential")}</Button>}</div>}
    </section>;
  };

  return <div {...dialogFocus} tabIndex={-1} className="connection-modal" role="dialog" aria-modal="true" aria-labelledby="jump-connection-title">
    <div className="connection-card jump-connection-card">
      <header className="connection-card-header">
        <span className="connection-server-icon jump" aria-hidden="true"><Network size={22} strokeWidth={1.5} /></span>
        <div className="connection-card-title"><p>{verification && !connecting ? t(verification.leg === "jump" ? "connection.jumpFingerprintTitle" : "connection.targetFingerprintTitle") : t(mode === "reconnect" ? "connection.jumpProgress.reconnect" : "connection.jumpProgress.title")}</p><h2 id="jump-connection-title">{profile.name}</h2></div>
        <Button variant="ghost" size="icon" disabled={busy} aria-label={t("a11y.close")} onClick={close}><X size={17} /></Button>
      </header>
      <div className="connection-card-body">
        <div className="jump-route-summary" aria-label={t("connection.jumpProgress.routeSummary") }>
          <div><small>{t("connection.jumpProgress.jumpHost")}</small><strong>{jumpProfile.name}</strong><span>{jumpProfile.host}:{jumpProfile.port}</span></div>
          <span className="jump-route-arrow" aria-hidden="true"><i /><ArrowRight size={14} /></span>
          <div><small>{t("connection.jumpProgress.targetHost")}</small><strong>{profile.name}</strong><span>{profile.host}:{profile.port}</span></div>
        </div>
        {connecting && <JumpConnectionProgress stage={progressStage} testing={pendingAction.current === "test"} />}
        <div hidden={connecting}>
          {verification ? <div className="jump-fingerprint-panel"><span className="jump-verification-badge"><ShieldCheck size={14} />{t(verification.leg === "jump" ? "connection.jumpProgress.verifyingJump" : "connection.jumpProgress.verifyingTarget")}</span><p>{t("connection.fingerprintHint")}</p><dl><dt>{t("connection.fingerprint")}</dt><dd>{verification.value.fingerprint}</dd><dt>{t("connection.host")}</dt><dd>{verification.value.host}:{verification.value.port} · {verification.value.keyType}</dd></dl><div className="connection-form-actions"><Button variant="ghost" disabled={busy} onClick={() => { clearAttempt(); }}>{t("connection.cancel")}</Button><Button variant="secondary" disabled={busy} onClick={() => void trust(false)}>{t("connection.trustOnce")}</Button><Button disabled={busy} onClick={() => void trust(true)}>{t("connection.trustRemember")}</Button></div></div> : <form className="jump-credential-form" aria-busy={busy} onSubmit={submit("connect")}>
            <div className="jump-credential-grid">{credentialField("jump", jumpProfile)}{credentialField("target", profile)}</div>
            {vault?.vaultInitialized && !vault.vaultUnlocked && <div className="jump-vault-card"><p><LockKeyhole size={16} />{t("connection.vaultLocked")}</p><label>{t("connection.vaultPassword")}<Input type="password" autoComplete="current-password" {...register("masterPassword")} /></label><Button type="button" variant="secondary" disabled={busy} onClick={() => void unlockVaultForCredentials()}>{t("connection.unlockVault")}</Button></div>}
            {(rememberJump || rememberTarget) && !vault?.vaultInitialized && !vault?.platformUnlockAvailable && <div className="jump-vault-card"><p>{t("connection.createVaultHint")}</p><label>{t("connection.vaultPassword")}<Input type="password" autoComplete="new-password" {...register("masterPassword")} /></label><label>{t("connection.confirmVaultPassword")}<Input type="password" autoComplete="new-password" {...register("confirmMasterPassword")} /></label></div>}
            <div className="connection-form-actions"><Button type="button" variant="ghost" disabled={busy} onClick={close}>{t("connection.cancel")}</Button><Button type="button" variant="secondary" disabled={busy} onClick={() => void submit("test")()}>{t("connection.test")}</Button><Button type="submit" disabled={busy}>{t(mode === "reconnect" ? "connection.scanReconnect" : "connection.scan")}<ArrowRight size={14} aria-hidden="true" /></Button></div>
          </form>}
        </div>
        {testSucceeded && <div className="mt-4 rounded-md border border-emerald-500/40 bg-emerald-500/10 p-3 text-sm text-emerald-600"><div className="font-medium">{t("connection.testSuccessTitle")}</div><div className="mt-1 text-xs">{t("connection.jumpTestSuccess")}</div></div>}
        {saveWarning && <div className="mt-4 rounded-md border border-amber-500/40 bg-amber-500/10 p-3 text-sm text-amber-600"><div className="font-medium">{t("connection.saveWarningTitle")}</div><div className="mt-1 text-xs">{t("connection.jumpSaveWarning")}</div></div>}
        {failure && <div role="alert" className="mt-4 rounded-md border border-red-500/40 bg-red-500/10 p-3 text-sm text-red-500"><div className="font-medium">{failure.includes("HOST_KEY_CHANGED") ? t("connection.changedTitle") : t("connection.error")}</div><div className="mt-1 text-xs">{t(`connection.errors.${failure}`, { defaultValue: t(`errors.${failure}`, { defaultValue: t("connection.defaultError") }) })}</div></div>}
      </div>
      <footer className="connection-card-footer"><LockKeyhole size={13} aria-hidden="true" /><span>{t(connecting ? pendingAction.current === "test" ? "connection.jumpProgress.testFooter" : "connection.jumpProgress.footer" : "connection.jumpProgress.security")}</span></footer>
    </div>
  </div>;
}
