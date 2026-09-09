// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import i18n from "../../i18n";
import { useCatalogStore } from "../../stores/catalog-store";
import { useSessionStore } from "../../stores/session-store";
import { updateProfile } from "../../lib/tauri/catalog";
import type { ServerProfile } from "../../types/domain";
import { ServerManagement } from "./ServerManagement";

vi.mock("../../lib/tauri/catalog", () => ({ updateProfile: vi.fn() }));
vi.mock("../../features/profiles/ProfileDialog", () => ({ ProfileDialog: () => <div role="dialog" /> }));
const profiles: ServerProfile[] = [
  { id: "alpha", name: "Alpha", host: "alpha.test", port: 2222, username: "deploy", groupId: "source", authMethod: "privateKey", keySource: { type: "vault", keyId: "opaque-key" }, connectionRoute: { type: "direct" }, osDistribution: "ubuntu", sortOrder: 2, createdAt: "", updatedAt: "", lastConnectedAt: "2026-09-04" },
  { id: "beta", name: "Beta", host: "beta.test", port: 22, username: "root", groupId: "target", authMethod: "password", connectionRoute: { type: "direct" }, sortOrder: 8, createdAt: "", updatedAt: "" },
];
let container: HTMLDivElement;
let root: Root;
const connect = vi.fn();
const pointTarget = vi.fn();
const section = (id: string) => container.querySelector<HTMLElement>(`section[data-server-group="${id}"]`)!;
async function pointer(type: string, target: EventTarget = window, x = 30, y = 30, pointerId = 1) {
  await act(async () => {
    const event = new Event(type, { bubbles: true, cancelable: true });
    Object.assign(event, { pointerId, isPrimary: true, button: 0, clientX: x, clientY: y });
    target.dispatchEvent(event);
  });
}
async function start(selector = ".host-select") {
  await pointer("pointerdown", container.querySelector(selector)!, 10, 10);
}
async function move(target: HTMLElement | null) {
  pointTarget.mockReturnValue(target);
  await pointer("pointermove");
}
async function render(query = "") {
  await act(async () => { root.render(<ServerManagement query={query} onQueryChange={() => undefined} onConnectProfile={connect} />); });
}
beforeEach(async () => {
  vi.clearAllMocks();
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  Object.defineProperty(document, "elementFromPoint", { configurable: true, value: pointTarget });
  pointTarget.mockReturnValue(null);
  localStorage.clear();
  await i18n.changeLanguage("en-US");
  useCatalogStore.setState({ profiles: [...profiles], groups: ["source", "target", "empty"].map((id, index) => ({ id, name: id, collapsed: id === "target", sortOrder: index, createdAt: "", updatedAt: "" })), selectedProfileId: null, loading: false, errorCode: null });
  useSessionStore.setState({ tabs: [], activeTabId: null });
  vi.mocked(updateProfile).mockImplementation(async (request) => {
    const current = useCatalogStore.getState().profiles;
    const profile = current.find((item) => item.id === request.id)!;
    const sortOrder = profile.groupId === request.groupId ? request.sortOrder : current.filter((item) => item.groupId === request.groupId).reduce((max, item) => Math.max(max, item.sortOrder), -1) + 1;
    return { ...profile, ...request, sortOrder };
  });
  container = document.createElement("div"); document.body.append(container); root = createRoot(container);
  await render();
});
afterEach(async () => {
  await act(async () => { root.unmount(); });
  container.remove(); vi.unstubAllGlobals();
  Reflect.deleteProperty(document, "elementFromPoint");
});

describe("server group pointer dragging", () => {
  it("highlights a collapsed group and appends the host through the existing update command", async () => {
    await start(); await move(section("target").querySelector(".group-toggle"));
    expect(section("target").classList.contains("server-group-drop-active")).toBe(true);
    expect(container.querySelector(".host-dragging")).not.toBeNull();
    await pointer("pointerup");
    expect(updateProfile).toHaveBeenCalledExactlyOnceWith({ id: "alpha", name: "Alpha", host: "alpha.test", port: 2222, username: "deploy", authMethod: "privateKey", keySource: { type: "vault", keyId: "opaque-key" }, connectionRoute: { type: "direct" }, groupId: "target", sortOrder: 2 });
    expect(useCatalogStore.getState().profiles.find((profile) => profile.id === "alpha")?.sortOrder).toBe(9);
    expect(useCatalogStore.getState().profiles.find((profile) => profile.id === "alpha")?.groupId).toBe("target");
    expect(container.querySelector(".server-group-drop-tray")).toBeNull();
    expect(container.querySelector(".server-move-notice")).toBeNull();
    expect(container.querySelector(".server-host-drag")).toBeNull();
    expect(connect).not.toHaveBeenCalled();
  });

  it("supports an empty group in compact view", async () => {
    await act(async () => { container.querySelector<HTMLButtonElement>(`[aria-label="${i18n.t("serverManagement.listView")}"]`)?.click(); });
    await start(); await move(section("empty")); await pointer("pointerup");
    expect(updateProfile).toHaveBeenCalledWith(expect.objectContaining({ groupId: "empty" }));
    expect(section("empty").querySelector(".host-name")?.textContent).toBe("Alpha");
  });

  it("exposes hidden and ungrouped targets while searching, and can ungroup a host", async () => {
    await render("Alpha");
    expect(section("target")).toBeNull();
    await start(); await move(null);
    expect(container.querySelectorAll(".server-group-drop-tray [data-server-group]")).toHaveLength(4);
    await move(container.querySelector('.server-group-drop-tray [data-server-group=""]'));
    await pointer("pointerup");
    expect(updateProfile).toHaveBeenCalledWith(expect.objectContaining({ groupId: null }));
    expect(section("").querySelector(".host-name")?.textContent).toBe("Alpha");
  });

  it("provides group targets in recent connections", async () => {
    await act(async () => { container.querySelector<HTMLButtonElement>(".server-filter-switch button:last-child")?.click(); });
    await start(); await move(null);
    await move(container.querySelector('.server-group-drop-tray [data-server-group="empty"]'));
    await pointer("pointerup");
    expect(updateProfile).toHaveBeenCalledWith(expect.objectContaining({ groupId: "empty" }));
  });

  it("does not mutate for a click, outside drop, same group, Escape, or pointer cancellation", async () => {
    await start(); await pointer("pointerup");
    await start(); await move(null); await pointer("pointerup");
    await start(); await move(section("source")); await pointer("pointerup");
    await start(); await move(section("target"));
    await act(async () => { window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape" })); });
    await pointer("pointerup");
    await start(); await move(section("target")); await pointer("pointercancel"); await pointer("pointerup");
    expect(updateProfile).not.toHaveBeenCalled();
    expect(container.querySelector(".server-group-drop-tray")).toBeNull();
  });

  it("ignores external drop events and unrelated pointers", async () => {
    await act(async () => { section("target").dispatchEvent(new Event("drop", { bubbles: true })); });
    await start(); await move(section("target")); await pointer("pointerup", window, 30, 30, 2);
    expect(updateProfile).not.toHaveBeenCalled();
    await pointer("pointercancel");
  });

  it("uses the latest profile metadata and rejects a removed group", async () => {
    await start(); await move(section("target"));
    await act(async () => { useCatalogStore.setState({ profiles: [{ ...profiles[0], host: "changed.test" }, profiles[1]] }); });
    await pointer("pointerup");
    expect(updateProfile).toHaveBeenCalledWith(expect.objectContaining({ host: "changed.test" }));
    vi.mocked(updateProfile).mockClear();
    // Reset the source and remove the target after a drag has started.
    await act(async () => { useCatalogStore.setState({ profiles: [...profiles] }); });
    await start(); await move(section("target"));
    await act(async () => { useCatalogStore.setState({ groups: useCatalogStore.getState().groups.filter((group) => group.id !== "target") }); });
    await pointer("pointerup");
    expect(updateProfile).not.toHaveBeenCalled();
  });

  it("keeps the original group on failure and reports a localized error", async () => {
    vi.mocked(updateProfile).mockRejectedValueOnce(new Error("fixture"));
    await act(async () => { void i18n.changeLanguage("zh-CN"); });
    await start(); await move(section("empty")); await pointer("pointerup");
    expect(useCatalogStore.getState().profiles[0].groupId).toBe("source");
    expect(container.querySelector('[role="alert"]')?.textContent).toBe(i18n.t("serverManagement.moveFailed", { name: "Alpha" }));
    expect(container.querySelector(".host-draggable")).not.toBeNull();
  });

  it("locks further moves until persistence completes", async () => {
    let finish: ((profile: ServerProfile) => void) | undefined;
    vi.mocked(updateProfile).mockImplementationOnce(() => new Promise((resolve) => { finish = resolve; }));
    await start(); await move(section("empty")); await pointer("pointerup");
    expect(container.querySelector(".host-draggable")).toBeNull();
    expect(useCatalogStore.getState().profiles[0].groupId).toBe("source");
    await start(); await move(section("target")); await pointer("pointerup");
    expect(updateProfile).toHaveBeenCalledTimes(1);
    await act(async () => { finish?.({ ...profiles[0], groupId: "empty", sortOrder: 0 }); });
    expect(useCatalogStore.getState().profiles.find((profile) => profile.id === "alpha")?.groupId).toBe("empty");
  });

  it("cancels an in-progress drag when the page unmounts", async () => {
    await start(); await move(section("target"));
    await act(async () => { root.render(null); });
    await pointer("pointerup");
    expect(updateProfile).not.toHaveBeenCalled();
  });

  it("keeps group editing accessible through the existing actions menu", async () => {
    await act(async () => { container.querySelector<HTMLButtonElement>(".host-menu button")?.click(); });
    expect(container.querySelector('[role="dialog"]')).not.toBeNull();
    expect(connect).not.toHaveBeenCalled();
    expect(updateProfile).not.toHaveBeenCalled();
  });

  it.each([".host-item", ".host-name", ".host-address", ".server-host-meta"])("can drag from the card surface %s", async (selector) => {
    await start(selector); await move(section("empty")); await pointer("pointerup");
    expect(updateProfile).toHaveBeenCalledWith(expect.objectContaining({ id: "alpha", groupId: "empty" }));
  });

  it("suppresses post-drag click and double-click, but preserves the next click and keyboard activation", async () => {
    await start(); await move(section("source")); await pointer("pointerup");
    const select = container.querySelector<HTMLButtonElement>(".host-select")!;
    await act(async () => {
      select.dispatchEvent(new MouseEvent("click", { bubbles: true, detail: 1 }));
      select.dispatchEvent(new MouseEvent("dblclick", { bubbles: true, detail: 2 }));
    });
    expect(useCatalogStore.getState().selectedProfileId).toBeNull();
    expect(connect).not.toHaveBeenCalled();
    await start(); await pointer("pointerup");
    await act(async () => { select.dispatchEvent(new MouseEvent("click", { bubbles: true, detail: 1 })); });
    expect(useCatalogStore.getState().selectedProfileId).toBe("alpha");
    await act(async () => { select.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true })); });
    expect(connect).toHaveBeenCalledExactlyOnceWith("alpha");
  });

  it.each([".server-host-connect", ".host-menu summary", ".host-menu button"])("does not start a drag on an action %s", async (selector) => {
    await start(selector); await move(section("empty")); await pointer("pointerup");
    expect(updateProfile).not.toHaveBeenCalled();
    expect(container.querySelector(".server-group-drop-tray")).toBeNull();
  });
});
