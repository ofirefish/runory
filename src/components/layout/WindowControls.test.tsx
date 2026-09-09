// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { WindowControls } from "./WindowControls";

vi.mock("@tauri-apps/api/core", () => ({ isTauri: () => false }));
vi.mock("react-i18next", () => ({ useTranslation: () => ({ t: (key: string) => key }) }));
vi.mock("../../stores/settings-store", () => ({
  useSettingsStore: (selector: (state: { theme: string; setTheme: () => void }) => unknown) =>
    selector({ theme: "light", setTheme: vi.fn() }),
}));

let container: HTMLDivElement;
let root: Root;

beforeEach(() => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  Object.defineProperty(window, "matchMedia", {
    writable: true,
    value: vi.fn().mockImplementation((query: string) => ({
      matches: false,
      media: query,
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
    })),
  });
  container = document.createElement("div");
  document.body.append(container);
  root = createRoot(container);
});

afterEach(async () => {
  await act(async () => root.unmount());
  container.remove();
  vi.unstubAllGlobals();
});

describe("WindowControls pricing entry", () => {
  it("places the pricing button immediately before the theme toggle and opens pricing", async () => {
    const onOpenPricing = vi.fn();
    await act(async () => root.render(<WindowControls onOpenPricing={onOpenPricing} />));

    const pricing = container.querySelector<HTMLButtonElement>(".window-pricing-button");
    const theme = container.querySelector<HTMLButtonElement>(".window-theme-toggle");
    expect(pricing).not.toBeNull();
    expect(theme).not.toBeNull();
    expect(pricing?.nextElementSibling).toBe(theme);
    expect(pricing?.getAttribute("aria-label")).toBe("window.openPricing");

    await act(async () => { pricing?.click(); });
    expect(onOpenPricing).toHaveBeenCalledOnce();
  });
});
