export type SessionState = "idle" | "connecting" | "verifying-host" | "authenticating" | "opening-shell" | "connected" | "disconnected" | "error";

export type TerminalEvent =
  | { event: "output"; data: { bytes: number[] } }
  | { event: "state"; data: { state: SessionState } }
  | { event: "closed"; data: { reason: string | null } };

export type HostVerification = {
  attemptId: string;
  host: string;
  port: number;
  keyType: string;
  fingerprint: string;
  status: "unknown" | "trusted";
};

export type KnownHost = {
  host: string;
  port: number;
  keyType: string;
  fingerprint: string;
  createdAt: string;
  updatedAt: string;
};

export type ConnectRequest = {
  verificationAttemptId: string;
  profileId: string;
  credential: CredentialInput;
  cols: number;
  rows: number;
};

export type TestConnectionRequest = Omit<ConnectRequest, "cols" | "rows">;

export type CredentialInput =
  | { mode: "session-only"; secret: string }
  | { mode: "remember-securely"; secret: string }
  | { mode: "stored" };

export type CredentialKind = "password" | "key-passphrase";

export type CredentialStatus = {
  vaultInitialized: boolean;
  vaultUnlocked: boolean;
  hasCredential: boolean;
  platformUnlockSupported: boolean;
  platformUnlockAvailable: boolean;
  platformUnlockConfigured: boolean;
};

export type PrivateKeyImport = { keyId: string; name: string };

export type ConnectResponse = {
  sessionId: string;
  credentialSaved: boolean;
};

export type TestConnectionResponse = {
  credentialSaved: boolean;
};

export type SftpEntryKind = "directory" | "file" | "symlink" | "other";

export type SftpEntry = {
  name: string;
  path: string;
  kind: SftpEntryKind;
  size: number | null;
  modified: number | null;
  permissions: number | null;
};

export type SftpDirectory = {
  path: string;
  entries: SftpEntry[];
};

export type SftpMetadata = Omit<SftpEntry, "name">;

export type RemoteImagePreview = {
  path: string;
  name: string;
  mimeType: "image/png" | "image/jpeg" | "image/webp" | "image/gif";
  width: number;
  height: number;
  size: number;
  dataBase64: string;
};

export type RemoteTextPreview = {
  path: string;
  name: string;
  encoding: string;
  language: string;
  size: number;
  lineCount: number;
  content: string;
};

export type LocalFileSelection = { grantId: string; name: string; size: number };
export type UploadDirectoryHistoryEntry = { remoteDirectory: string; lastUploadedAtMs: number };
export type TransferDirection = "upload" | "download";
export type TransferState = "queued" | "running" | "completed" | "failed" | "cancelled";
export type TransferJob = {
  id: string;
  sessionId: string;
  direction: TransferDirection;
  name: string;
  remotePath: string;
  totalBytes: number;
  transferredBytes: number;
  state: TransferState;
  errorCode: string | null;
};
export type TransferEvent = { event: "updated"; data: { job: TransferJob } };
