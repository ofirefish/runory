// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import i18n from "../../i18n";
import * as ssh from "../../lib/tauri/ssh";
import type { ServerProfile } from "../../types/domain";
import type { HostVerification } from "../../types/session";
import { ConnectionDialog } from "./ConnectionDialog";

vi.mock("../../lib/tauri/ssh", () => ({
  credentialStatus: vi.fn(), prepareHostVerification: vi.fn(),
  trustHost: vi.fn().mockResolvedValue(undefined), cancelHostVerification: vi.fn(),
  forgetCredential: vi.fn(), initializeVault: vi.fn(), unlockVault: vi.fn(), unlockVaultWithPlatform: vi.fn(),
}));

const profile: ServerProfile = {
  id: "preview-host", name: "Production · Singapore", host: "192.0.2.24", port: 22, username: "deploy",
  authMethod: "password", connectionRoute: { type: "direct" }, groupId: null, sortOrder: 0, createdAt: "", updatedAt: "",
};
const verified: HostVerification = { attemptId: "attempt", host: profile.host, port: 22, keyType: "ssh-ed25519", fingerprint: "SHA256:fixture", status: "trusted" };
const vault = { vaultInitialized: true, vaultUnlocked: true, hasCredential: true, platformUnlockSupported: true, platformUnlockAvailable: true, platformUnlockConfigured: true };
function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason: unknown) => void;
  const promise = new Promise<T>((res, rej) => { resolve = res; reject = rej; });
  return { promise, resolve, reject };
}
let root: Root;
let container: HTMLDivElement;
const onClose = vi.fn();
const onConnect = vi.fn<() => Promise<boolean>>();
const onTest = vi.fn<() => Promise<boolean>>();
async function render() {
  await act(async () => { root.render(<ConnectionDialog profile={profile} mode="connect" onClose={onClose} onConnect={onConnect} onTest={onTest} />); });
}
async function click(key: string) {
  const button = [...container.querySelectorAll<HTMLButtonElement>("button")].find((element) => element.textContent === i18n.t(key));
  expect(button).toBeTruthy();
  await act(async () => { button!.click(); });
}

beforeEach(async () => {
  vi.clearAllMocks();
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  await i18n.changeLanguage("en-US");
  vi.mocked(ssh.credentialStatus).mockResolvedValue(vault);
  vi.mocked(ssh.prepareHostVerification).mockResolvedValue(verified);
  onConnect.mockResolvedValue(false);
  onTest.mockResolvedValue(false);
  container = document.createElement("div");
  document.body.append(container);
  root = createRoot(container);
});
afterEach(async () => {
  await act(async () => { root.unmount(); });
  container.remove();
  vi.unstubAllGlobals();
});

describe("connection dialog progress", () => {
  it("keeps keyboard focus in the dialog and allows Escape before connecting", async () => {
    vi.mocked(ssh.credentialStatus).mockResolvedValue({ ...vault, hasCredential: false });
    await render();
    const controls = [...container.querySelectorAll<HTMLButtonElement>("button")];
    const last = controls.at(-1)!;
    last.focus();
    await act(async () => { last.dispatchEvent(new KeyboardEvent("keydown", { key: "Tab", bubbles: true, cancelable: true })); });
    expect(document.activeElement).toBe(controls[0]);
    await act(async () => { controls[0].dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true })); });
    expect(onClose).toHaveBeenCalledOnce();
  });
  it("advances only after real host verification and keeps credential actions hidden while connecting", async () => {
    const verification = deferred<HostVerification>();
    const connection = deferred<boolean>();
    vi.mocked(ssh.prepareHostVerification).mockReturnValue(verification.promise);
    onConnect.mockReturnValue(connection.promise);
    await render();
    expect(container.querySelector('[aria-current="step"]')?.textContent).toContain(i18n.t("connection.progress.verify"));
    expect(container.querySelector("form")?.closest("[hidden]")).toBeTruthy();
    expect(onConnect).not.toHaveBeenCalled();
    await act(async () => { verification.resolve(verified); });
    expect(container.querySelector('[aria-current="step"]')?.textContent).toContain(i18n.t("connection.progress.connect"));
    expect(container.querySelector('[data-state="complete"]')?.textContent).toContain(i18n.t("connection.progress.verified"));
    expect(onClose).not.toHaveBeenCalled();
    expect(onConnect).toHaveBeenCalledWith({ profileId: profile.id, verificationAttemptId: "attempt", credential: { mode: "stored" } });
    await act(async () => { connection.resolve(false); });
    expect(onClose).toHaveBeenCalledOnce();
  });

  it("waits for explicit trust and preserves trust-once approval", async () => {
    vi.mocked(ssh.prepareHostVerification).mockResolvedValue({ ...verified, status: "unknown" });
    await render();
    expect(container.querySelector(".connection-attempt")).toBeNull();
    expect(container.textContent).toContain(verified.fingerprint);
    expect(onConnect).not.toHaveBeenCalled();
    await click("connection.trustOnce");
    expect(ssh.trustHost).toHaveBeenCalledWith("attempt", false);
    expect(onConnect).toHaveBeenCalledOnce();
  });

  it("cancels an unknown host without connecting", async () => {
    vi.mocked(ssh.prepareHostVerification).mockResolvedValue({ ...verified, status: "unknown" });
    await render();
    await click("connection.cancel");
    expect(ssh.cancelHostVerification).toHaveBeenCalledWith("attempt");
    expect(onConnect).not.toHaveBeenCalled();
  });

  it("restores credential entry with a localized error after authentication fails", async () => {
    onConnect.mockRejectedValue({ code: "AUTH_FAILED" });
    await render();
    expect(container.querySelector(".connection-attempt")).toBeNull();
    expect(container.querySelector("form")?.closest("[hidden]")).toBeNull();
    expect(container.querySelector('input[name="password"]')).toBeTruthy();
    expect(container.querySelector('[role="alert"]')?.textContent).toContain(i18n.t("connection.errors.AUTH_FAILED"));
    expect(onClose).not.toHaveBeenCalled();
  });

  it("blocks changed host keys before authentication", async () => {
    vi.mocked(ssh.prepareHostVerification).mockRejectedValue({ code: "HOST_KEY_CHANGED" });
    await render();
    expect(container.querySelector('[role="alert"]')?.textContent).toContain(i18n.t("connection.changedTitle"));
    expect(onConnect).not.toHaveBeenCalled();
    expect(ssh.trustHost).not.toHaveBeenCalled();
  });

  it("uses distinct test progress and keeps the test result open", async () => {
    const test = deferred<boolean>();
    onTest.mockReturnValue(test.promise);
    await render();
    onClose.mockClear();
    await click("connection.testSaved");
    expect(container.querySelector('[role="status"]')?.textContent).toContain(i18n.t("connection.progress.testing"));
    expect(container.querySelector(".connection-card-footer")?.textContent).toContain(i18n.t("connection.progress.testFooter"));
    await act(async () => { test.resolve(false); });
    expect(container.textContent).toContain(i18n.t("connection.testSuccessTitle"));
    expect(onClose).not.toHaveBeenCalled();
  });
});
