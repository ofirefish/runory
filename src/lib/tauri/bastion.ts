import { Channel, invoke } from "@tauri-apps/api/core";
import type { ConnectResponse, CredentialInput, HostVerification, TerminalEvent } from "../../types/session";

export type BastionFlowUiState =
  | "authenticating"
  | "awaitingUser"
  | "authenticated"
  | "discoveringAssets"
  | "selectingAsset"
  | "selectingAccount"
  | "discoveringAccounts"
  | "connecting"
  | "connected"
  | "failed"
  | "cancelled";

export type AuthChallenge =
  | { type: "password"; id: string; message: string }
  | { type: "totp"; id: string; message: string }
  | { type: "smsCode"; id: string; message: string; maskedTarget?: string }
  | { type: "confirm"; id: string; message: string }
  | { type: "choice"; id: string; message: string; choices: { id: string; label: string }[] }
  | { type: "text"; id: string; message: string; secret: boolean };

export type ExternalAuthAction =
  | { type: "openBrowser"; url: string; callbackUri?: string }
  | { type: "deviceCode"; verificationUri: string; userCode: string; expiresInSecs: number }
  | { type: "qrCode"; payload: string };

export type AuthChallengeResponse =
  | { type: "password"; id: string; password: string }
  | { type: "totp"; id: string; code: string }
  | { type: "smsCode"; id: string; code: string }
  | { type: "confirm"; id: string; accepted: boolean }
  | { type: "choice"; id: string; choiceId: string }
  | { type: "text"; id: string; value: string }
  | { type: "externalCompleted"; id: string }
  | { type: "cancel"; id: string };

export type BastionFlowSnapshot = {
  flowId: string;
  profileId: string;
  bastionId: string;
  provider: string;
  state: BastionFlowUiState;
  challenge?: AuthChallenge;
  externalAction?: ExternalAuthAction;
  selectedAssetId?: string;
  selectedAccount?: string;
  sessionMetadata?: {
    sessionId?: string;
    provider: string;
    assetId: string;
    account: string;
    recording: boolean;
    commandAudit: boolean;
    fileAudit: boolean;
    startedAt: number;
  };
  errorCode?: string;
};

export type BastionAsset = {
  provider: string;
  remoteId: string;
  name: string;
  address?: string;
  platform?: string;
  nodePath?: string;
};

export type BastionAccount = {
  remoteId?: string;
  username: string;
  displayName?: string;
  privileged: boolean;
  secretManagedByBastion: boolean;
};

export type AssetPage = {
  items: BastionAsset[];
  page: number;
  pageSize: number;
  hasMore: boolean;
};

export type BastionStartRequest =
  | { authMode?: "password"; username: string; password: string }
  | {
      authMode: "accessKey";
      accessKeyId?: string;
      accessKeySecret?: string;
      accessKeyCredential?: CredentialInput;
    }
  | { authMode: "browserSso"; username?: string }
  | {
      authMode: "token";
      token?: string;
      tokenCredential?: CredentialInput;
      username?: string;
    };

export const bastionStart = (profileId: string, request: BastionStartRequest) => {
  if (request.authMode === "accessKey") {
    return invoke<BastionFlowSnapshot>("bastion_start", {
      request: {
        profileId,
        authMode: "accessKey",
        accessKeyId: request.accessKeyId ?? "",
        accessKeySecret: request.accessKeySecret ?? "",
        accessKeyCredential: request.accessKeyCredential,
      },
    });
  }
  if (request.authMode === "browserSso") {
    return invoke<BastionFlowSnapshot>("bastion_start", {
      request: {
        profileId,
        authMode: "browserSso",
        username: request.username ?? "",
      },
    });
  }
  if (request.authMode === "token") {
    return invoke<BastionFlowSnapshot>("bastion_start", {
      request: {
        profileId,
        authMode: "token",
        token: request.token ?? "",
        tokenCredential: request.tokenCredential,
        username: request.username ?? "",
      },
    });
  }
  return invoke<BastionFlowSnapshot>("bastion_start", {
    request: {
      profileId,
      authMode: "password",
      username: request.username,
      password: request.password,
    },
  });
};

export const bastionContinueAuth = (flowId: string, response: AuthChallengeResponse) =>
  invoke<BastionFlowSnapshot>("bastion_continue_auth", { request: { flowId, response } });

export const bastionOpenExternalBrowser = (flowId: string) =>
  invoke<void>("bastion_open_external_browser", { request: { flowId } });

export const bastionListAssets = (flowId: string, search?: string, page = 0, pageSize = 20) =>
  invoke<AssetPage>("bastion_list_assets", { request: { flowId, search, page, pageSize } });

export const bastionSelectAsset = (flowId: string, assetId: string) =>
  invoke<BastionFlowSnapshot>("bastion_select_asset", { request: { flowId, assetId } });

export const bastionListAccounts = (flowId: string) =>
  invoke<BastionAccount[]>("bastion_list_accounts", { request: { flowId } });

export const bastionSelectAccount = (flowId: string, account: string) =>
  invoke<BastionFlowSnapshot>("bastion_select_account", { request: { flowId, account } });

export const bastionCancelFlow = (flowId: string) =>
  invoke<void>("bastion_cancel_flow", { request: { flowId } });

export const bastionPrepareHost = (profileId: string, flowId: string) =>
  invoke<HostVerification>("bastion_prepare_host", {
    request: { profileId, flowId },
  });

export const bastionConnectFlow = (
  flowId: string,
  profileId: string,
  cols: number,
  rows: number,
  onEvent: (event: TerminalEvent) => void,
  verificationAttemptId?: string,
  sshPasswordCredential?: CredentialInput,
) => {
  const channel = new Channel<TerminalEvent>();
  channel.onmessage = onEvent;
  return invoke<ConnectResponse>("bastion_connect_flow", {
    request: {
      flowId,
      profileId,
      cols,
      rows,
      verificationAttemptId,
      sshPasswordCredential,
    },
    onEvent: channel,
  });
};
