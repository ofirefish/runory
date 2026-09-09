// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { GlobalTopBar } from "./GlobalTopBar";

const { toggleMaximize } = vi.hoisted(() => ({ toggleMaximize: vi.fn() }));

vi.mock("@tauri-apps/api/core", () => ({ isTauri: () => true }));
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: () => ({ toggleMaximize }) }));
vi.mock("react-i18next", () => ({ useTranslation: () => ({ t: (key: string) => key }) }));
vi.mock("../ui/button", () => ({ Button: ({ children, ...props }: React.ButtonHTMLAttributes<HTMLButtonElement>) => <button {...props}>{children}</button> }));
vi.mock("./GlobalSearch", () => ({ GlobalSearch: () => <input /> }));
vi.mock("./WindowControls", () => ({ WindowControls: () => <div data-no-drag /> }));
vi.mock("../../features/tunnels/TitlebarTunnels", () => ({ TitlebarTunnels: () => <div data-no-drag /> }));

let container: HTMLDivElement;
let root: Root;

beforeEach(async () => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  toggleMaximize.mockReset();
  container = document.createElement("div");
  document.body.append(container);
  root = createRoot(container);
  await act(async () => root.render(<GlobalTopBar onOpenProfile={() => undefined} tabsHostRef={() => undefined} onOpenNavigation={() => undefined} onOpenTunnels={() => undefined} onOpenPricing={() => undefined} />));
});

afterEach(async () => {
  await act(async () => root.unmount());
  container.remove();
  vi.unstubAllGlobals();
});

describe("global titlebar", () => {
  it("delegates drag-region double-clicks to Tauri's native titlebar handling", async () => {
    const titlebar = container.querySelector<HTMLElement>(".global-topbar")!;
    expect(titlebar.hasAttribute("data-tauri-drag-region")).toBe(true);

    await act(async () => titlebar.dispatchEvent(new MouseEvent("dblclick", { bubbles: true })));

    expect(toggleMaximize).not.toHaveBeenCalled();
  });
});
