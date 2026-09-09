// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import i18n from "../../i18n";
import * as ssh from "../../lib/tauri/ssh";
import type { ServerProfile } from "../../types/domain";
import { JumpConnectionDialog } from "./JumpConnectionDialog";

vi.mock("../../lib/tauri/ssh", () => ({
  cancelHostVerification: vi.fn().mockResolvedValue(undefined),
  cancelJumpConnection: vi.fn().mockResolvedValue(undefined),
  credentialStatus: vi.fn(),
  initializeVault: vi.fn(),
  prepareHostVerification: vi.fn(),
  prepareJumpConnection: vi.fn(),
  trustHost: vi.fn().mockResolvedValue(undefined),
  unlockVault: vi.fn(),
  unlockVaultWithPlatform: vi.fn(),
}));

const jump: ServerProfile = {
  id: "jump-a",
  name: "Gateway A",
  host: "gateway.example.com",
  port: 22,
  username: "jump-user",
  authMethod: "password",
  connectionRoute: { type: "direct" },
  groupId: null,
  sortOrder: 0,
  createdAt: "",
  updatedAt: "",
};
const target: ServerProfile = {
  ...jump,
  id: "target-b",
  name: "Private B",
  host: "10.0.0.8",
  username: "target-user",
  connectionRoute: { type: "jumpHost", profileId: jump.id },
};
const vault = {
  vaultInitialized: true,
  vaultUnlocked: true,
  hasCredential: false,
  platformUnlockSupported: true,
  platformUnlockAvailable: true,
  platformUnlockConfigured: true,
};
const verification = (attemptId: string, host: string, status: "trusted" | "unknown") => ({
  attemptId,
  host,
  port: 22,
  keyType: "ssh-ed25519",
  fingerprint: `SHA256:${attemptId}`,
  status,
});

let root: Root;
let container: HTMLDivElement;
const onClose = vi.fn();
const onConnect = vi.fn().mockResolvedValue(true);
const onTest = vi.fn().mockResolvedValue(true);

async function flush() {
  await act(async () => { await new Promise((resolve) => setTimeout(resolve, 0)); });
}

async function click(label: string) {
  const button = [...container.querySelectorAll<HTMLButtonElement>("button")]
    .find((candidate) => candidate.textContent === label);
  expect(button).toBeTruthy();
  await act(async () => { button!.click(); });
}

async function type(name: string, value: string) {
  const input = container.querySelector<HTMLInputElement>(`input[name="${name}"]`);
  expect(input).toBeTruthy();
  await act(async () => {
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!.call(input, value);
    input!.dispatchEvent(new Event("input", { bubbles: true }));
  });
}

async function render() {
  await act(async () => {
    root.render(<JumpConnectionDialog profile={target} jumpProfile={jump} mode="connect" onClose={onClose} onConnect={onConnect} onTest={onTest} />);
  });
  await flush();
}

beforeEach(async () => {
  vi.clearAllMocks();
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  await i18n.changeLanguage("en-US");
  vi.mocked(ssh.credentialStatus).mockResolvedValue(vault);
  container = document.createElement("div");
  document.body.append(container);
  root = createRoot(container);
});

afterEach(async () => {
  await act(async () => root.unmount());
  container.remove();
  vi.unstubAllGlobals();
});

describe("jump connection dialog", () => {
  it("verifies A and B separately before submitting B with a one-use jump ticket", async () => {
    vi.mocked(ssh.prepareHostVerification).mockResolvedValue(verification("jump-check", jump.host, "unknown"));
    vi.mocked(ssh.prepareJumpConnection).mockResolvedValue({
      preparationId: "jump-ticket",
      targetVerification: verification("target-check", target.host, "unknown"),
      jumpCredentialSaved: false,
    });
    await render();
    await type("jumpSecret", "jump-secret");
    await type("targetSecret", "target-secret");

    await click(i18n.t("connection.scan"));
    expect(container.textContent).toContain("SHA256:jump-check");
    await click(i18n.t("connection.trustOnce"));
    expect(ssh.prepareJumpConnection).toHaveBeenCalledWith("target-b", "jump-check", {
      mode: "session-only",
      secret: "jump-secret",
    });
    expect(container.textContent).toContain("SHA256:target-check");
    await click(i18n.t("connection.trustOnce"));

    expect(onConnect).toHaveBeenCalledWith({
      profileId: "target-b",
      verificationAttemptId: "target-check",
      credential: { mode: "session-only", secret: "target-secret" },
      jumpPreparationId: "jump-ticket",
    });
    expect(onClose).toHaveBeenCalledOnce();
  });

  it("automatically uses two unlocked stored credentials without exposing a secret", async () => {
    vi.mocked(ssh.credentialStatus).mockResolvedValue({ ...vault, hasCredential: true });
    vi.mocked(ssh.prepareHostVerification).mockResolvedValue(verification("jump-check", jump.host, "trusted"));
    vi.mocked(ssh.prepareJumpConnection).mockResolvedValue({
      preparationId: "jump-ticket",
      targetVerification: verification("target-check", target.host, "trusted"),
      jumpCredentialSaved: false,
    });
    await render();
    await flush();

    expect(ssh.prepareJumpConnection).toHaveBeenCalledWith("target-b", "jump-check", { mode: "stored" });
    expect(onConnect).toHaveBeenCalledWith({
      profileId: "target-b",
      verificationAttemptId: "target-check",
      credential: { mode: "stored" },
      jumpPreparationId: "jump-ticket",
    });
    expect(JSON.stringify(onConnect.mock.calls)).not.toContain("secret");
  });
});
