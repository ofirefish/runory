// @vitest-environment jsdom

import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { AuthWebPage } from "./AuthWebPage";

const auth = vi.hoisted(() => ({
  consume: vi.fn<() => Promise<unknown>>(),
  updatePassword: vi.fn<(password: string) => Promise<void>>(),
}));

vi.mock("../../lib/supabase/cloud", () => ({
  consumeCloudAuthRedirect: auth.consume,
  updateCloudPassword: auth.updatePassword,
}));

vi.mock("react-i18next", () => ({ useTranslation: () => ({ t: (key: string) => key }) }));

describe("auth web page", () => {
  let container: HTMLDivElement;
  let root: Root;

  beforeEach(() => {
    vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
    container = document.createElement("div");
    document.body.append(container);
    root = createRoot(container);
    auth.consume.mockReset().mockResolvedValue({});
    auth.updatePassword.mockReset().mockResolvedValue();
  });

  afterEach(() => {
    act(() => root.unmount());
    container.remove();
    vi.unstubAllGlobals();
  });

  it("confirms an auth redirect without exposing tokens", async () => {
    await act(async () => { root.render(<AuthWebPage mode="confirm" />); });
    expect(auth.consume).toHaveBeenCalledTimes(1);
    expect(container.textContent).toContain("cloud.webConfirmSuccess");
  });

  it("updates a recovered account password", async () => {
    await act(async () => { root.render(<AuthWebPage mode="reset" />); });
    const inputs = container.querySelectorAll<HTMLInputElement>('input[type="password"]');
    inputs[0].value = "secure-pass-123";
    inputs[1].value = "secure-pass-123";
    const button = container.querySelector<HTMLButtonElement>("button");
    await act(async () => { button?.click(); });
    expect(auth.updatePassword).toHaveBeenCalledWith("secure-pass-123");
    expect(container.textContent).toContain("cloud.webResetSuccess");
  });

  it("keeps the reset form available when passwords do not match", async () => {
    await act(async () => { root.render(<AuthWebPage mode="reset" />); });
    const inputs = container.querySelectorAll<HTMLInputElement>('input[type="password"]');
    inputs[0].value = "secure-pass-123";
    inputs[1].value = "different-pass";
    await act(async () => { container.querySelector<HTMLButtonElement>("button")?.click(); });
    expect(auth.updatePassword).not.toHaveBeenCalled();
    expect(container.querySelectorAll('input[type="password"]')).toHaveLength(2);
    expect(container.textContent).toContain("cloud.webPasswordError");
  });
});
