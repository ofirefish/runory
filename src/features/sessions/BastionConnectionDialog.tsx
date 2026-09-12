import { ArrowRight, KeyRound, LoaderCircle, Network, Server, ShieldCheck } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "../../components/ui/button";
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "../../components/ui/dialog";
import { Input } from "../../components/ui/input";
import { appErrorCode } from "../../lib/app-error";
import {
  bastionCancelFlow,
  bastionContinueAuth,
  bastionListAccounts,
  bastionListAssets,
  bastionOpenExternalBrowser,
  bastionPrepareHost,
  bastionSelectAccount,
  bastionSelectAsset,
  bastionStart,
  type AuthChallenge,
  type BastionAccount,
  type BastionAsset,
  type BastionFlowSnapshot,
} from "../../lib/tauri/bastion";
import { cancelHostVerification, credentialStatus, forgetCredential, initializeVault, trustHost, unlockVault, unlockVaultWithPlatform } from "../../lib/tauri/ssh";
import { vaultUnlockAction } from "../../lib/vault-unlock";
import type { ServerProfile } from "../../types/domain";
import type { CredentialInput, CredentialStatus, HostVerification } from "../../types/session";
import "./connection-dialog.css";

type Props = {
  profile: ServerProfile;
  mode: "connect" | "reconnect";
  onClose: () => void;
  onOpenSession: (
    flowId: string,
    provider: string,
    verificationAttemptId?: string,
    sshPasswordCredential?: CredentialInput,
  ) => Promise<boolean>;
};

type Stage = "login" | "mfa" | "sso" | "assets" | "accounts" | "verify" | "opening";

export function BastionConnectionDialog({ profile, onClose, onOpenSession }: Props) {
  const { t } = useTranslation();
  const provider =
    profile.connectionRoute.type === "bastion" ? profile.connectionRoute.provider : "bastion";
  const isJumpServer = provider === "jumpserver";
  const isTeleport = provider === "teleport";
  const isBoundary = provider === "boundary";
  const providerLabel = provider;
  const apiBase =
    profile.connectionRoute.type === "bastion" ? profile.connectionRoute.apiBaseUrl?.trim() || "" : "";
  const skipsHostVerify = provider === "mock" || isTeleport || isBoundary;

  const [authMode, setAuthMode] = useState<"accessKey" | "password" | "browserSso" | "token">(
    isJumpServer ? "accessKey" : isTeleport ? "browserSso" : isBoundary ? "token" : "password",
  );
  const [username, setUsername] = useState(profile.username);
  const [password, setPassword] = useState("");
  const [accessKeyId, setAccessKeyId] = useState("");
  const [accessKeySecret, setAccessKeySecret] = useState("");
  const [token, setToken] = useState("");
  const [sshPassword, setSshPassword] = useState("");
  const [rememberToken, setRememberToken] = useState(false);
  const [rememberTargetPassword, setRememberTargetPassword] = useState(false);
  const [useStoredToken, setUseStoredToken] = useState(false);
  const [useStoredTargetPassword, setUseStoredTargetPassword] = useState(false);
  const [tokenVault, setTokenVault] = useState<CredentialStatus | null>(null);
  const [passwordVault, setPasswordVault] = useState<CredentialStatus | null>(null);
  const [masterPassword, setMasterPassword] = useState("");
  const [confirmMasterPassword, setConfirmMasterPassword] = useState("");
  const [challengeCode, setChallengeCode] = useState("");
  const [snap, setSnap] = useState<BastionFlowSnapshot | null>(null);
  const [assets, setAssets] = useState<BastionAsset[]>([]);
  const [accounts, setAccounts] = useState<BastionAccount[]>([]);
  const [assetSearch, setAssetSearch] = useState("");
  const [manualTargetId, setManualTargetId] = useState("");
  const [verification, setVerification] = useState<HostVerification | null>(null);
  const [failure, setFailure] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const flowId = useRef<string | null>(null);
  const pendingConnect = useRef<BastionFlowSnapshot | null>(null);
  const pendingSshPasswordCredential = useRef<CredentialInput | undefined>(undefined);
  const opening = useRef(false);
  const autoOpenedBrowserForFlow = useRef<string | null>(null);

  const refreshVault = useCallback(async () => {
    if (!isBoundary) return null;
    const [tokenStatus, passwordStatus] = await Promise.all([
      credentialStatus(profile.id, "bastion-token"),
      credentialStatus(profile.id, "bastion-target-password"),
    ]);
    setTokenVault(tokenStatus);
    setPasswordVault(passwordStatus);
    if (tokenStatus.vaultUnlocked && tokenStatus.hasCredential) setUseStoredToken(true);
    if (passwordStatus.vaultUnlocked && passwordStatus.hasCredential) setUseStoredTargetPassword(true);
    return tokenStatus;
  }, [isBoundary, profile.id]);

  useEffect(() => {
    void refreshVault().catch((error) => setFailure(appErrorCode(error)));
  }, [refreshVault]);

  const close = useCallback(() => {
    if (busy) return;
    if (verification) void cancelHostVerification(verification.attemptId);
    if (flowId.current) void bastionCancelFlow(flowId.current);
    flowId.current = null;
    onClose();
  }, [busy, onClose, verification]);

  useEffect(
    () => () => {
      if (flowId.current) void bastionCancelFlow(flowId.current);
    },
    [],
  );

  const reportFailure = (error: unknown) => {
    const code = appErrorCode(error);
    console.error("[bastion]", code, error);
    setFailure(code);
  };

  const finishOpen = async (next: BastionFlowSnapshot, verificationAttemptId?: string) => {
    const closed = await onOpenSession(
      next.flowId,
      next.provider,
      verificationAttemptId,
      pendingSshPasswordCredential.current,
    );
    flowId.current = null;
    pendingConnect.current = null;
    pendingSshPasswordCredential.current = undefined;
    setVerification(null);
    if (closed) onClose();
  };

  const openIfReady = async (next: BastionFlowSnapshot) => {
    if (next.state !== "connecting" || opening.current) return;
    opening.current = true;
    setBusy(true);
    setFailure(null);
    try {
      if (skipsHostVerify || next.provider === "mock") {
        await finishOpen(next);
        return;
      }
      const prepared = await bastionPrepareHost(profile.id, next.flowId);
      if (prepared.status === "trusted") {
        await finishOpen(next, prepared.attemptId);
        return;
      }
      pendingConnect.current = next;
      setVerification(prepared);
      opening.current = false;
    } catch (error) {
      reportFailure(error);
      opening.current = false;
    } finally {
      setBusy(false);
    }
  };

  const trustAndConnect = async (remember: boolean) => {
    if (!verification || !pendingConnect.current) return;
    setBusy(true);
    setFailure(null);
    opening.current = true;
    try {
      await trustHost(verification.attemptId, remember);
      await finishOpen(pendingConnect.current, verification.attemptId);
    } catch (error) {
      reportFailure(error);
      opening.current = false;
    } finally {
      setBusy(false);
    }
  };

  const cancelVerification = () => {
    if (verification) void cancelHostVerification(verification.attemptId);
    setVerification(null);
    pendingConnect.current = null;
    opening.current = false;
  };

  const applySnapshot = async (next: BastionFlowSnapshot) => {
    flowId.current = next.flowId;
    setSnap(next);
    if (next.state === "selectingAsset") {
      const page = await bastionListAssets(next.flowId, assetSearch || undefined);
      setAssets(page.items);
    }
    if (next.state === "selectingAccount") {
      setAccounts(await bastionListAccounts(next.flowId));
    }
    await openIfReady(next);
  };

  const unlockOrInitialize = async (status: CredentialStatus | null) => {
    const action = vaultUnlockAction(status);
    if (action === "none") return status;
    setFailure(null);
    try {
      if (action === "initialize-platform") {
        await initializeVault();
      } else if (action === "unlock-platform") {
        await unlockVaultWithPlatform();
      } else {
        if (!masterPassword) {
          setFailure("VAULT_PASSWORD_REQUIRED");
          return null;
        }
        if (!status?.vaultInitialized && masterPassword !== confirmMasterPassword) {
          setFailure("VAULT_PASSWORD_MISMATCH");
          return null;
        }
        await unlockVault(masterPassword);
        setMasterPassword("");
        setConfirmMasterPassword("");
      }
      return await refreshVault();
    } catch (error) {
      reportFailure(error);
      return null;
    }
  };

  const buildTargetPasswordCredential = (): CredentialInput | undefined => {
    if (useStoredTargetPassword && passwordVault?.hasCredential && passwordVault.vaultUnlocked) {
      return { mode: "stored" };
    }
    const secret = sshPassword.trim() || password.trim();
    if (!secret) return undefined;
    return rememberTargetPassword
      ? { mode: "remember-securely", secret }
      : { mode: "session-only", secret };
  };

  const start = async () => {
    setBusy(true);
    setFailure(null);
    try {
      if (authMode === "token") {
        const needsVault =
          rememberToken ||
          rememberTargetPassword ||
          useStoredToken ||
          useStoredTargetPassword;
        if (needsVault) {
          const unlocked = await unlockOrInitialize(tokenVault ?? passwordVault);
          if (!unlocked?.vaultUnlocked) return;
        }
        const tokenCredential: CredentialInput = useStoredToken
          ? { mode: "stored" }
          : rememberToken
            ? { mode: "remember-securely", secret: token.trim() }
            : { mode: "session-only", secret: token.trim() };
        if (!useStoredToken && !token.trim()) {
          setFailure("BASTION_AUTH_FAILED");
          return;
        }
        const targetPasswordCredential = buildTargetPasswordCredential();
        pendingSshPasswordCredential.current = targetPasswordCredential;
        const next = await bastionStart(profile.id, {
          authMode: "token",
          tokenCredential,
          username:
            targetPasswordCredential && username.trim() ? username.trim() : undefined,
        });
        await applySnapshot(next);
        setToken("");
        setSshPassword("");
        await refreshVault();
        return;
      }

      pendingSshPasswordCredential.current = undefined;
      const next =
        authMode === "accessKey"
          ? await bastionStart(profile.id, {
              authMode: "accessKey",
              accessKeyId: accessKeyId.trim(),
              accessKeySecret,
            })
          : authMode === "browserSso"
            ? await bastionStart(profile.id, {
                authMode: "browserSso",
                username: username.trim() || undefined,
              })
            : await bastionStart(profile.id, {
                authMode: "password",
                username: username.trim(),
                password,
              });
      await applySnapshot(next);
      setPassword("");
      setAccessKeySecret("");
    } catch (error) {
      reportFailure(error);
    } finally {
      setBusy(false);
    }
  };

  const completeExternalAuth = async () => {
    if (!snap) return;
    setBusy(true);
    setFailure(null);
    try {
      await applySnapshot(
        await bastionContinueAuth(snap.flowId, {
          type: "externalCompleted",
          id: "external-sso",
        }),
      );
    } catch (error) {
      reportFailure(error);
    } finally {
      setBusy(false);
    }
  };

  const openExternalBrowser = async () => {
    if (!snap) return;
    setFailure(null);
    try {
      await bastionOpenExternalBrowser(snap.flowId);
    } catch (error) {
      reportFailure(error);
    }
  };

  const submitChallenge = async (challenge: AuthChallenge) => {
    if (!snap) return;
    setBusy(true);
    setFailure(null);
    try {
      const response =
        challenge.type === "totp" || challenge.type === "smsCode"
          ? { type: challenge.type, id: challenge.id, code: challengeCode.trim() }
          : challenge.type === "password"
            ? { type: "password" as const, id: challenge.id, password: challengeCode }
            : challenge.type === "text"
              ? { type: "text" as const, id: challenge.id, value: challengeCode }
              : challenge.type === "confirm"
                ? { type: "confirm" as const, id: challenge.id, accepted: true }
                : { type: "cancel" as const, id: challenge.id };
      setChallengeCode("");
      await applySnapshot(await bastionContinueAuth(snap.flowId, response));
    } catch (error) {
      reportFailure(error);
    } finally {
      setBusy(false);
    }
  };

  const pickAsset = async (assetId: string) => {
    if (!snap) return;
    setBusy(true);
    setFailure(null);
    try {
      await applySnapshot(await bastionSelectAsset(snap.flowId, assetId));
    } catch (error) {
      reportFailure(error);
    } finally {
      setBusy(false);
    }
  };

  const pickAccount = async (account: string) => {
    if (!snap) return;
    setBusy(true);
    setFailure(null);
    try {
      await applySnapshot(await bastionSelectAccount(snap.flowId, account));
    } catch (error) {
      reportFailure(error);
    } finally {
      setBusy(false);
    }
  };

  const searchAssets = async () => {
    if (!snap || snap.state !== "selectingAsset") return;
    setBusy(true);
    try {
      setAssets((await bastionListAssets(snap.flowId, assetSearch || undefined)).items);
    } catch (error) {
      reportFailure(error);
    } finally {
      setBusy(false);
    }
  };

  const challenge = snap?.challenge;
  const externalAction = snap?.externalAction;
  const stage: Stage = verification
    ? "verify"
    : !snap || snap.state === "authenticating" || snap.state === "failed"
      ? "login"
      : snap.state === "awaitingUser" && externalAction
        ? "sso"
        : snap.state === "awaitingUser"
          ? "mfa"
          : snap.state === "selectingAsset"
            ? "assets"
            : snap.state === "selectingAccount"
              ? "accounts"
              : "opening";

  useEffect(() => {
    if (
      stage !== "sso" ||
      externalAction?.type !== "openBrowser" ||
      !snap?.flowId ||
      autoOpenedBrowserForFlow.current === snap.flowId
    ) {
      return;
    }
    autoOpenedBrowserForFlow.current = snap.flowId;
    void bastionOpenExternalBrowser(snap.flowId).catch((error) => reportFailure(error));
  }, [stage, externalAction, snap?.flowId]);

  const titleKey =
    stage === "verify"
      ? "connection.fingerprintTitle"
      : stage === "assets"
        ? "bastion.selectAssetTitle"
        : stage === "accounts"
          ? "bastion.selectAccountTitle"
          : stage === "mfa"
            ? "bastion.mfaTitle"
            : stage === "sso"
              ? "bastion.ssoTitle"
              : "bastion.dialogTitle";

  const authModeLabel =
    authMode === "accessKey"
      ? "bastion.authModeAccessKey"
      : authMode === "browserSso"
        ? "bastion.authModeBrowserSso"
        : authMode === "token"
          ? "bastion.authModeToken"
          : "bastion.authModePassword";

  return (
    <Dialog
      open
      onOpenChange={(next) => {
        if (!next) close();
      }}
    >
      <DialogContent
        className="sm:max-w-lg"
        closeDisabled={busy}
        onPointerDownOutside={(event) => {
          if (busy) event.preventDefault();
        }}
        onEscapeKeyDown={(event) => {
          if (busy) event.preventDefault();
        }}
      >
        <DialogHeader>
          <div className="flex items-start gap-3 pr-6">
            <span className="connection-server-icon bastion mt-0.5" aria-hidden="true">
              <Network size={22} strokeWidth={1.5} />
            </span>
            <div className="min-w-0 space-y-1">
              <p className="text-[11px] font-medium text-[hsl(var(--secondary))]">
                {t(titleKey, { name: profile.name })}
              </p>
              <DialogTitle id="bastion-connection-title">{profile.name}</DialogTitle>
              <DialogDescription>
                {t(stage === "opening" ? "bastion.progressFooter" : "bastion.securityFooter")}
              </DialogDescription>
            </div>
          </div>
        </DialogHeader>

        <div className="-mx-4 max-h-[50vh] space-y-4 overflow-y-auto px-4 py-[5px]">
          <dl className="connection-target">
            <div>
              <dt>{t("bastion.provider")}</dt>
              <dd title={providerLabel}>{t(`bastion.providerLabel.${providerLabel}`, { defaultValue: providerLabel })}</dd>
            </div>
            <div>
              <dt>{t("bastion.endpoint")}</dt>
              <dd title={apiBase || `${profile.host}:${profile.port}`}>{apiBase || `${profile.host}:${profile.port}`}</dd>
            </div>
            <div className="connection-target-auth">
              <dt>
                <KeyRound size={13} aria-hidden="true" />
                {t("bastion.authMode")}
              </dt>
              <dd>{t(authModeLabel)}</dd>
            </div>
          </dl>

          {stage === "verify" && verification && (
            <div className="bastion-stage space-y-3">
              <p className="bastion-stage-hint">{t("connection.fingerprintHint")}</p>
              <dl className="bastion-fingerprint">
                <dt>{t("connection.fingerprint")}</dt>
                <dd>{verification.fingerprint}</dd>
                <dt>{t("connection.host")}</dt>
                <dd>
                  {verification.host}:{verification.port} · {verification.keyType}
                </dd>
              </dl>
            </div>
          )}

          {stage === "login" && (
            <form
              id="bastion-login-form"
              className="connection-credential-form space-y-4"
              aria-busy={busy}
              onSubmit={(event) => {
                event.preventDefault();
                void start();
              }}
            >
              {isJumpServer && (
                <div className="bastion-auth-toggle" role="group" aria-label={t("bastion.authMode")}>
                  <button
                    type="button"
                    data-active={authMode === "accessKey"}
                    disabled={busy}
                    onClick={() => setAuthMode("accessKey")}
                  >
                    {t("bastion.authModeAccessKey")}
                  </button>
                  <button
                    type="button"
                    data-active={authMode === "password"}
                    disabled={busy}
                    onClick={() => setAuthMode("password")}
                  >
                    {t("bastion.authModePassword")}
                  </button>
                </div>
              )}

              {isTeleport && (
                <div className="bastion-auth-toggle" role="group" aria-label={t("bastion.authMode")}>
                  <button
                    type="button"
                    data-active={authMode === "browserSso"}
                    disabled={busy}
                    onClick={() => setAuthMode("browserSso")}
                  >
                    {t("bastion.authModeBrowserSso")}
                  </button>
                  <button
                    type="button"
                    data-active={authMode === "password"}
                    disabled={busy}
                    onClick={() => setAuthMode("password")}
                  >
                    {t("bastion.authModePassword")}
                  </button>
                </div>
              )}

              {authMode === "accessKey" ? (
                <>
                  <label className="block text-sm font-medium">
                    {t("bastion.accessKeyId")}
                    <Input
                      className="mt-1"
                      value={accessKeyId}
                      disabled={busy}
                      autoComplete="off"
                      autoFocus
                      onChange={(event) => setAccessKeyId(event.target.value)}
                    />
                  </label>
                  <label className="block text-sm font-medium">
                    {t("bastion.accessKeySecret")}
                    <Input
                      className="mt-1"
                      type="password"
                      value={accessKeySecret}
                      disabled={busy}
                      autoComplete="off"
                      onChange={(event) => setAccessKeySecret(event.target.value)}
                    />
                  </label>
                  <p className="text-xs text-[hsl(var(--muted))]">{t("bastion.accessKeyHint")}</p>
                </>
              ) : authMode === "browserSso" ? (
                <>
                  <label className="block text-sm font-medium">
                    {t("bastion.usernameHint")}
                    <Input
                      className="mt-1"
                      value={username}
                      disabled={busy}
                      autoComplete="username"
                      autoFocus
                      onChange={(event) => setUsername(event.target.value)}
                    />
                  </label>
                  <p className="text-xs text-[hsl(var(--muted))]">{t("bastion.teleportSsoHint")}</p>
                </>
              ) : authMode === "token" ? (
                <>
                  {tokenVault?.vaultUnlocked && tokenVault.hasCredential && (
                    <div className="space-y-2 rounded-lg border p-3">
                      <p className="flex items-center gap-2 text-sm font-medium">
                        <ShieldCheck size={16} />
                        {t("bastion.savedTokenReady")}
                      </p>
                      <div className="flex flex-wrap gap-2">
                        <Button
                          type="button"
                          variant={useStoredToken ? "default" : "secondary"}
                          disabled={busy}
                          onClick={() => {
                            setUseStoredToken(true);
                            setToken("");
                          }}
                        >
                          {t("bastion.useSavedToken")}
                        </Button>
                        <Button
                          type="button"
                          variant="ghost"
                          disabled={busy}
                          onClick={() => {
                            void (async () => {
                              setBusy(true);
                              try {
                                await forgetCredential(profile.id, "bastion-token");
                                setUseStoredToken(false);
                                await refreshVault();
                              } catch (error) {
                                reportFailure(error);
                              } finally {
                                setBusy(false);
                              }
                            })();
                          }}
                        >
                          {t("bastion.forgetSavedToken")}
                        </Button>
                        <Button
                          type="button"
                          variant="ghost"
                          disabled={busy}
                          onClick={() => setUseStoredToken(false)}
                        >
                          {t("bastion.enterAnotherToken")}
                        </Button>
                      </div>
                    </div>
                  )}
                  {!useStoredToken && (
                    <label className="block text-sm font-medium">
                      {t("bastion.boundaryToken")}
                      <Input
                        className="mt-1"
                        type="password"
                        value={token}
                        disabled={busy}
                        autoComplete="off"
                        autoFocus
                        onChange={(event) => {
                          setToken(event.target.value);
                          setUseStoredToken(false);
                        }}
                      />
                    </label>
                  )}
                  <label className="flex items-start gap-2 text-sm">
                    <input
                      className="mt-1"
                      type="checkbox"
                      checked={rememberToken}
                      disabled={busy || useStoredToken}
                      onChange={(event) => setRememberToken(event.target.checked)}
                    />
                    <span>
                      <span className="font-medium">{t("bastion.rememberToken")}</span>
                      <span className="mt-0.5 block text-xs text-[hsl(var(--muted))]">
                        {t("connection.rememberHint")}
                      </span>
                    </span>
                  </label>

                  {passwordVault?.vaultUnlocked && passwordVault.hasCredential && (
                    <div className="space-y-2 rounded-lg border p-3">
                      <p className="flex items-center gap-2 text-sm font-medium">
                        <ShieldCheck size={16} />
                        {t("bastion.savedTargetPasswordReady")}
                      </p>
                      <div className="flex flex-wrap gap-2">
                        <Button
                          type="button"
                          variant={useStoredTargetPassword ? "default" : "secondary"}
                          disabled={busy}
                          onClick={() => {
                            setUseStoredTargetPassword(true);
                            setSshPassword("");
                          }}
                        >
                          {t("bastion.useSavedTargetPassword")}
                        </Button>
                        <Button
                          type="button"
                          variant="ghost"
                          disabled={busy}
                          onClick={() => {
                            void (async () => {
                              setBusy(true);
                              try {
                                await forgetCredential(profile.id, "bastion-target-password");
                                setUseStoredTargetPassword(false);
                                await refreshVault();
                              } catch (error) {
                                reportFailure(error);
                              } finally {
                                setBusy(false);
                              }
                            })();
                          }}
                        >
                          {t("bastion.forgetSavedTargetPassword")}
                        </Button>
                        <Button
                          type="button"
                          variant="ghost"
                          disabled={busy}
                          onClick={() => setUseStoredTargetPassword(false)}
                        >
                          {t("bastion.enterAnotherTargetPassword")}
                        </Button>
                      </div>
                    </div>
                  )}

                  <label className="block text-sm font-medium">
                    {t("bastion.targetSshUsername")}
                    <Input
                      className="mt-1"
                      value={username}
                      disabled={busy}
                      autoComplete="username"
                      onChange={(event) => setUsername(event.target.value)}
                    />
                  </label>
                  {!useStoredTargetPassword && (
                    <label className="block text-sm font-medium">
                      {t("bastion.targetSshPassword")}
                      <Input
                        className="mt-1"
                        type="password"
                        value={sshPassword}
                        disabled={busy}
                        autoComplete="off"
                        onChange={(event) => {
                          setSshPassword(event.target.value);
                          setUseStoredTargetPassword(false);
                        }}
                      />
                    </label>
                  )}
                  <label className="flex items-start gap-2 text-sm">
                    <input
                      className="mt-1"
                      type="checkbox"
                      checked={rememberTargetPassword}
                      disabled={busy || useStoredTargetPassword || !sshPassword.trim()}
                      onChange={(event) => setRememberTargetPassword(event.target.checked)}
                    />
                    <span>
                      <span className="font-medium">{t("bastion.rememberTargetPassword")}</span>
                      <span className="mt-0.5 block text-xs text-[hsl(var(--muted))]">
                        {t("bastion.rememberTargetPasswordHint")}
                      </span>
                    </span>
                  </label>

                  {(rememberToken || rememberTargetPassword) &&
                    !(tokenVault?.vaultUnlocked ?? passwordVault?.vaultUnlocked) &&
                    !(tokenVault?.vaultInitialized ?? passwordVault?.vaultInitialized) &&
                    !(tokenVault?.platformUnlockAvailable ?? passwordVault?.platformUnlockAvailable) && (
                      <div className="rounded-lg border p-3 space-y-3">
                        <p className="text-xs text-[hsl(var(--muted))]">{t("connection.createVaultHint")}</p>
                        <label className="block text-sm font-medium">
                          {t("connection.vaultPassword")}
                          <Input
                            className="mt-1"
                            type="password"
                            autoComplete="new-password"
                            value={masterPassword}
                            disabled={busy}
                            onChange={(event) => setMasterPassword(event.target.value)}
                          />
                        </label>
                        <label className="block text-sm font-medium">
                          {t("connection.confirmVaultPassword")}
                          <Input
                            className="mt-1"
                            type="password"
                            autoComplete="new-password"
                            value={confirmMasterPassword}
                            disabled={busy}
                            onChange={(event) => setConfirmMasterPassword(event.target.value)}
                          />
                        </label>
                      </div>
                    )}
                  {(rememberToken || rememberTargetPassword || useStoredToken || useStoredTargetPassword) &&
                    !(tokenVault?.vaultUnlocked ?? passwordVault?.vaultUnlocked) &&
                    (tokenVault?.vaultInitialized ?? passwordVault?.vaultInitialized) &&
                    !(tokenVault?.platformUnlockConfigured && tokenVault?.platformUnlockAvailable) && (
                      <label className="block text-sm font-medium">
                        {t("connection.vaultPassword")}
                        <Input
                          className="mt-1"
                          type="password"
                          autoComplete="current-password"
                          value={masterPassword}
                          disabled={busy}
                          onChange={(event) => setMasterPassword(event.target.value)}
                        />
                      </label>
                    )}

                  <p className="text-xs text-[hsl(var(--muted))]">{t("bastion.boundaryTokenHint")}</p>
                </>
              ) : (
                <>
                  <label className="block text-sm font-medium">
                    {t("bastion.username")}
                    <Input
                      className="mt-1"
                      value={username}
                      disabled={busy}
                      autoComplete="username"
                      autoFocus
                      onChange={(event) => setUsername(event.target.value)}
                    />
                  </label>
                  <label className="block text-sm font-medium">
                    {t("bastion.password")}
                    <Input
                      className="mt-1"
                      type="password"
                      value={password}
                      disabled={busy}
                      autoComplete="current-password"
                      onChange={(event) => setPassword(event.target.value)}
                    />
                  </label>
                </>
              )}
            </form>
          )}

          {stage === "sso" && externalAction && (
            <div className="bastion-stage space-y-4">
              <p className="bastion-stage-hint">
                {isTeleport ? t("bastion.teleportSsoContinueHint") : t("bastion.ssoHint")}
              </p>
              {externalAction.type === "openBrowser" && (
                <Button
                  type="button"
                  variant="secondary"
                  disabled={busy}
                  onClick={() => {
                    void openExternalBrowser();
                  }}
                >
                  {t(isTeleport ? "bastion.openTshLogin" : "bastion.openBrowserSso")}
                </Button>
              )}
              {externalAction.type === "deviceCode" && (
                <dl className="bastion-fingerprint">
                  <dt>{t("bastion.deviceCode")}</dt>
                  <dd>{externalAction.userCode}</dd>
                  <dt>{t("bastion.verificationUri")}</dt>
                  <dd>{externalAction.verificationUri}</dd>
                </dl>
              )}
            </div>
          )}

          {stage === "mfa" && challenge && (
            <div className="bastion-stage space-y-4">
              <p className="bastion-stage-hint">
                {challenge.message === "teleport.password"
                  ? t("bastion.teleportPasswordChallenge")
                  : challenge.message || t("bastion.mfaRequired")}
              </p>
              {(challenge.type === "totp" ||
                challenge.type === "smsCode" ||
                challenge.type === "password" ||
                challenge.type === "text") && (
                <label className="block text-sm font-medium">
                  {challenge.type === "password"
                    ? t("connection.password")
                    : t("bastion.oneTimePassword")}
                  <Input
                    className="mt-1"
                    type={challenge.type === "password" ? "password" : "text"}
                    value={challengeCode}
                    disabled={busy}
                    autoFocus
                    autoComplete={challenge.type === "password" ? "current-password" : "one-time-code"}
                    onChange={(event) => setChallengeCode(event.target.value)}
                    onKeyDown={(event) => {
                      if (event.key === "Enter") void submitChallenge(challenge);
                    }}
                  />
                </label>
              )}
            </div>
          )}

          {stage === "assets" && (
            <div className="bastion-stage space-y-3">
              <p className="bastion-stage-hint">{t("bastion.selectAssetHint")}</p>
              <div className="flex gap-2">
                <Input
                  className="min-w-0 flex-1"
                  value={assetSearch}
                  disabled={busy}
                  placeholder={t("bastion.searchAssets")}
                  onChange={(event) => setAssetSearch(event.target.value)}
                  onKeyDown={(event) => {
                    if (event.key === "Enter") void searchAssets();
                  }}
                />
                <Button
                  type="button"
                  variant="secondary"
                  className="shrink-0 whitespace-nowrap"
                  disabled={busy}
                  onClick={() => void searchAssets()}
                >
                  {t("bastion.searchAction")}
                </Button>
              </div>
              <ul className="bastion-picker-list">
                {assets.map((asset) => (
                  <li key={asset.remoteId}>
                    <button type="button" disabled={busy} onClick={() => void pickAsset(asset.remoteId)}>
                      <Server size={16} aria-hidden="true" />
                      <span>
                        <strong>{asset.name}</strong>
                        <small>{asset.address || asset.nodePath || asset.remoteId}</small>
                      </span>
                    </button>
                  </li>
                ))}
                {assets.length === 0 && (
                  <li className="bastion-picker-empty">
                    <div className="space-y-2 py-1">
                      <p>{t("bastion.noAssets")}</p>
                      {isBoundary && (
                        <p className="text-xs text-[hsl(var(--muted))]">{t("bastion.boundaryNoAssetsHint")}</p>
                      )}
                    </div>
                  </li>
                )}
              </ul>
              {isBoundary && (
                <div className="flex gap-2">
                  <Input
                    className="min-w-0 flex-1 font-mono text-xs"
                    value={manualTargetId}
                    disabled={busy}
                    placeholder={t("bastion.boundaryTargetIdPlaceholder")}
                    aria-label={t("bastion.boundaryTargetId")}
                    onChange={(event) => setManualTargetId(event.target.value)}
                    onKeyDown={(event) => {
                      if (event.key === "Enter" && manualTargetId.trim()) void pickAsset(manualTargetId.trim());
                    }}
                  />
                  <Button
                    type="button"
                    variant="secondary"
                    className="shrink-0 whitespace-nowrap"
                    disabled={busy || !manualTargetId.trim()}
                    onClick={() => void pickAsset(manualTargetId.trim())}
                  >
                    {t("bastion.useTargetId")}
                  </Button>
                </div>
              )}
            </div>
          )}

          {stage === "accounts" && (
            <div className="bastion-stage space-y-3">
              <p className="bastion-stage-hint">{t("bastion.selectAccountHint")}</p>
              <ul className="bastion-picker-list">
                {accounts.map((account) => (
                  <li key={account.remoteId ?? account.username}>
                    <button type="button" disabled={busy} onClick={() => void pickAccount(account.username)}>
                      <ShieldCheck size={16} aria-hidden="true" />
                      <span>
                        <strong>{account.displayName ?? account.username}</strong>
                        <small>
                          {account.username}
                          {account.secretManagedByBastion ? ` · ${t("bastion.managedSecret")}` : ""}
                        </small>
                      </span>
                    </button>
                  </li>
                ))}
              </ul>
            </div>
          )}

          {stage === "opening" && (
            <p className="connection-vault-progress" role="status">
              <LoaderCircle size={14} aria-hidden="true" />
              {t("bastion.openingSession")}
            </p>
          )}

          {failure && (
            <div role="alert" className="rounded-md border border-red-500/40 bg-red-500/10 p-3 text-sm text-red-500">
              <div className="font-medium">{t("connection.error")}</div>
              <div className="mt-1 text-xs">
                {t(`connection.errors.${failure}`, {
                  cli: isBoundary ? "boundary" : isTeleport ? "tsh" : "tsh / boundary",
                  defaultValue: t(`errors.${failure}`, {
                    cli: isBoundary ? "boundary" : isTeleport ? "tsh" : "tsh / boundary",
                    defaultValue: t("bastion.genericError"),
                  }),
                })}
              </div>
            </div>
          )}
        </div>

        <DialogFooter className="sm:justify-between">
          {stage === "verify" && verification ? (
            <>
              <Button variant="ghost" disabled={busy} onClick={cancelVerification}>
                {t("connection.cancel")}
              </Button>
              <div className="flex flex-col-reverse gap-2 sm:flex-row">
                <Button variant="secondary" disabled={busy} onClick={() => void trustAndConnect(false)}>
                  {t("connection.trustOnce")}
                </Button>
                <Button disabled={busy} onClick={() => void trustAndConnect(true)}>
                  {t("connection.trustRemember")}
                </Button>
              </div>
            </>
          ) : stage === "login" ? (
            <>
              <DialogClose asChild>
                <Button type="button" variant="outline" disabled={busy}>
                  {t("connection.cancel")}
                </Button>
              </DialogClose>
              <Button
                type="submit"
                form="bastion-login-form"
                disabled={
                  busy ||
                  (authMode === "accessKey"
                    ? !accessKeyId.trim() || !accessKeySecret
                    : authMode === "browserSso"
                      ? false
                      : authMode === "token"
                        ? !(useStoredToken || token.trim())
                        : !username.trim() || !password)
                }
              >
                {busy ? <LoaderCircle size={14} className="animate-spin" /> : null}
                {t("bastion.signIn")}
                {!busy && <ArrowRight size={14} aria-hidden="true" />}
              </Button>
            </>
          ) : stage === "sso" ? (
            <>
              <DialogClose asChild>
                <Button type="button" variant="outline" disabled={busy}>
                  {t("connection.cancel")}
                </Button>
              </DialogClose>
              <Button disabled={busy} onClick={() => void completeExternalAuth()}>
                {busy ? <LoaderCircle size={14} className="animate-spin" /> : <ShieldCheck size={14} />}
                {t("bastion.ssoContinue")}
              </Button>
            </>
          ) : stage === "mfa" && challenge ? (
            <>
              <DialogClose asChild>
                <Button type="button" variant="outline" disabled={busy}>
                  {t("connection.cancel")}
                </Button>
              </DialogClose>
              <Button
                disabled={
                  busy ||
                  ((challenge.type === "totp" ||
                    challenge.type === "smsCode" ||
                    challenge.type === "password") &&
                    !challengeCode.trim())
                }
                onClick={() => void submitChallenge(challenge)}
              >
                {busy ? <LoaderCircle size={14} className="animate-spin" /> : <ShieldCheck size={14} />}
                {t("bastion.verify")}
              </Button>
            </>
          ) : (
            <DialogClose asChild>
              <Button type="button" variant="outline" disabled={busy || stage === "opening"}>
                {t("connection.cancel")}
              </Button>
            </DialogClose>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
