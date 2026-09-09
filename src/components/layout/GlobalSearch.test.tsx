// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import i18n from "../../i18n";
import { useCatalogStore } from "../../stores/catalog-store";
import { GlobalSearch } from "./GlobalSearch";

vi.mock("../ui/popover", async (importOriginal) => {
  // jsdom cannot measure viewport collisions; retain real Radix focus/dismissal behavior.
  const actual = await importOriginal<typeof import("../ui/popover")>();
  return { ...actual, PopoverContent: (props: React.ComponentProps<typeof actual.PopoverContent>) => <actual.PopoverContent {...props} avoidCollisions={false} /> };
});

const profiles = ["alpha", "beta"].map((name, index) => ({ id: name, name, host: `10.0.0.${index + 1}`, port: 22, username: index ? "deploy" : "root", groupId: index ? null : "production", authMethod: "password" as const, connectionRoute: { type: "direct" as const }, sortOrder: index, createdAt: "", updatedAt: "", osDistribution: index ? undefined : "ubuntu" as const }));
let container: HTMLDivElement;
let root: Root;
const openProfile = vi.fn();
const input = () => container.querySelector<HTMLInputElement>("input")!;
const options = () => [...document.querySelectorAll<HTMLButtonElement>('[role="option"]')];
async function type(query: string) {
  await act(async () => {
    input().focus();
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")?.set?.call(input(), query);
    input().dispatchEvent(new Event("input", { bubbles: true }));
  });
}
async function key(key: string, options: KeyboardEventInit = {}) {
  await act(async () => input().dispatchEvent(new KeyboardEvent("keydown", { key, bubbles: true, ...options })));
}

beforeEach(async () => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  vi.stubGlobal("ResizeObserver", class { observe() {} unobserve() {} disconnect() {} });
  // jsdom has no layout; provide a nonzero anchor for Radix's positioning loop.
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockReturnValue({ x: 0, y: 0, top: 0, left: 0, right: 280, bottom: 32, width: 280, height: 32, toJSON() {} });
  vi.spyOn(HTMLElement.prototype, "offsetWidth", "get").mockReturnValue(280);
  vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockReturnValue(32);
  const layoutStyle = document.createElement("div").style;
  layoutStyle.cssText = "position: static; display: block; visibility: visible; direction: ltr; width: 280px; height: 32px; transform: none; filter: none; perspective: none;";
  vi.spyOn(window, "getComputedStyle").mockReturnValue(layoutStyle);
  openProfile.mockReset();
  await i18n.changeLanguage("en-US");
  useCatalogStore.setState({ profiles, groups: [{ id: "production", name: "Production", sortOrder: 0, collapsed: false, createdAt: "", updatedAt: "" }], loading: false, errorCode: null });
  container = document.createElement("div");
  document.body.append(container);
  root = createRoot(container);
  await act(async () => root.render(<GlobalSearch onOpenProfile={openProfile} />));
});
afterEach(async () => {
  await act(async () => root.unmount());
  container.remove();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

describe("independent titlebar search", () => {
  it.each([{ ctrlKey: true }, { metaKey: true }])("captures the search shortcut before terminal input (%j)", async (modifiers) => {
    const terminalInput = document.createElement("textarea");
    container.append(terminalInput);
    const terminalKey = vi.fn((event: KeyboardEvent) => {
      event.preventDefault();
      event.stopPropagation();
    });
    terminalInput.addEventListener("keydown", terminalKey, true);
    const event = new KeyboardEvent("keydown", { key: "k", bubbles: true, cancelable: true, ...modifiers });
    await act(async () => {
      terminalInput.focus();
      terminalInput.dispatchEvent(event);
    });
    expect(document.activeElement).toBe(input());
    expect(input().getAttribute("aria-expanded")).toBe("true");
    expect(terminalKey).not.toHaveBeenCalled();
    expect(event.defaultPrevented).toBe(true);
  });

  it("does not intercept other terminal keys or steal focus from a modal", async () => {
    const terminalInput = document.createElement("textarea");
    container.append(terminalInput);
    const terminalKey = vi.fn();
    terminalInput.addEventListener("keydown", terminalKey);
    await act(async () => {
      terminalInput.focus();
      for (const options of [{ key: "c", ctrlKey: true }, { key: "k" }, { key: "k", ctrlKey: true, altKey: true }, { key: "k", ctrlKey: true, shiftKey: true }, { key: "k", ctrlKey: true, isComposing: true }]) {
        terminalInput.dispatchEvent(new KeyboardEvent("keydown", { bubbles: true, cancelable: true, ...options }));
      }
    });
    expect(terminalKey).toHaveBeenCalledTimes(5);
    expect(document.activeElement).toBe(terminalInput);
    const modal = document.createElement("div");
    modal.setAttribute("role", "dialog");
    modal.setAttribute("aria-modal", "true");
    modal.append(terminalInput);
    container.append(modal);
    await act(async () => {
      terminalInput.focus();
      terminalInput.dispatchEvent(new KeyboardEvent("keydown", { key: "k", ctrlKey: true, bubbles: true, cancelable: true }));
    });
    expect(document.activeElement).toBe(terminalInput);
    expect(input().getAttribute("aria-expanded")).toBe("false");
    expect(terminalKey).toHaveBeenCalledTimes(6);
  });

  it.each([[" ALPHA ", "alpha"], ["10.0.0.2", "beta"], ["DEPLOY", "beta"], ["production", "alpha"]])("matches %s and opens the selected target", async (query, id) => {
    await type(query);
    expect(options()).toHaveLength(1);
    expect(options()[0].textContent).toContain(id);
    expect(options()[0].textContent).toContain(id === "alpha" ? "Production" : i18n.t("sidebar.ungrouped"));
    await act(async () => options()[0].click());
    expect(openProfile).toHaveBeenCalledWith(id);
    expect(input().value).toBe("");
    expect(input().getAttribute("aria-expanded")).toBe("false");
  });

  it("shows the group and connection on one line with the host distribution logo", async () => {
    await type("alpha");
    const option = options()[0];
    const title = option.querySelector(".global-search-result-title")!;
    expect(title.textContent).toBe("Production/alpha");
    expect(title.querySelector("small")?.textContent).toBe("Production");
    expect(title.querySelector("strong")?.textContent).toBe("alpha");
    expect(option.querySelector(".host-os-logo.plain")?.getAttribute("aria-label")).toContain("Ubuntu");
  });

  it("supports shortcuts, keyboard selection, escape, and IME composition", async () => {
    await key("k", { ctrlKey: true });
    expect(document.activeElement).toBe(input());
    expect(document.querySelector('[role="status"]')?.textContent).toBe(i18n.t("shell.searchHint"));
    await type("10.0.0");
    expect(document.activeElement).toBe(input());
    expect(options()[0].getAttribute("aria-selected")).toBe("true");
    await key("ArrowUp");
    expect(options()[1].getAttribute("aria-selected")).toBe("true");
    expect(input().getAttribute("aria-activedescendant")).toBe(options()[1].id);
    await key("ArrowDown");
    expect(options()[0].getAttribute("aria-selected")).toBe("true");
    await key("ArrowDown");
    await key("Enter", { isComposing: true });
    expect(openProfile).not.toHaveBeenCalled();
    await key("Escape");
    expect(options()).toHaveLength(0);
    await key("Enter");
    expect(openProfile).not.toHaveBeenCalled();
    await key("k", { metaKey: true });
    await key("Enter");
    expect(openProfile).toHaveBeenCalledWith("beta");
  }, 15000);

  it("closes when focus moves outside and reopens on click", async () => {
    await type("alpha");
    const outside = document.createElement("button");
    container.append(outside);
    await act(async () => outside.focus());
    expect(options()).toHaveLength(0);
    await act(async () => input().click());
    expect(options()).toHaveLength(1);
  }, 15000);

  it("handles empty results and live catalog updates without opening stale targets", async () => {
    await type("missing");
    expect(options()).toHaveLength(0);
    expect(document.querySelector('[role="status"]')?.textContent).toBe(i18n.t("shell.searchNoResults"));
    await key("ArrowDown");
    await key("Enter");
    expect(openProfile).not.toHaveBeenCalled();
    await type("10.0.0");
    await key("ArrowDown");
    await act(async () => useCatalogStore.setState({ profiles: [profiles[0]] }));
    expect(options()).toHaveLength(1);
    await key("Enter");
    expect(openProfile).toHaveBeenCalledWith("alpha");
  });

  it("localizes empty, loading, and unavailable states", async () => {
    await act(async () => i18n.changeLanguage("zh-CN"));
    await type("missing");
    expect(document.querySelector('[role="status"]')?.textContent).toBe(i18n.t("shell.searchNoResults"));
    await act(async () => useCatalogStore.setState({ loading: true }));
    expect(document.querySelector('[role="status"]')?.textContent).toBe(i18n.t("common.loading"));
    await act(async () => useCatalogStore.setState({ loading: false, errorCode: "UNKNOWN" }));
    expect(document.querySelector('[role="status"]')?.textContent).toBe(i18n.t("shell.searchUnavailable"));
  });
});
