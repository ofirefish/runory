// @vitest-environment jsdom
import { act, useState } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import i18n from "../../i18n";
import { useCatalogStore } from "../../stores/catalog-store";
import { useSessionStore } from "../../stores/session-store";
import { ServerManagement } from "./ServerManagement";

vi.mock("../../features/profiles/ProfileDialog", () => ({ ProfileDialog: ({ initialGroupId }: { initialGroupId?: string | null }) => <div role="dialog" data-group={initialGroupId} /> }));
vi.mock("../../features/groups/GroupDialog", () => ({ GroupDialog: () => <div role="dialog" /> }));
const connect = vi.fn();
const openSync = vi.fn();
const openAuth = vi.fn();
let container: HTMLDivElement;
let root: Root;
const profiles = Array.from({ length: 24 }, (_, index) => ({
  id: `host-${index}`, name: `Server ${index}`, host: `host-${index}.example.test`, username: "root", port: index === 0 ? 2222 : 22,
  groupId: index < 12 ? "production" : null, authMethod: "password" as const, connectionRoute: { type: "direct" as const }, sortOrder: index,
  createdAt: "", updatedAt: "", lastConnectedAt: index < 4 ? `2026-09-0${index + 1}T00:00:00Z` : undefined,
}));
function Fixture() {
  const [query, setQuery] = useState("");
  return <ServerManagement query={query} onQueryChange={setQuery} onConnectProfile={connect} onOpenSync={openSync} onOpenAuth={openAuth} />;
}
async function click(selector: string) {
  const button = container.querySelector<HTMLElement>(selector);
  expect(button).not.toBeNull();
  await act(async () => { button?.click(); });
}
async function search(value: string) {
  const input = container.querySelector<HTMLInputElement>(".server-toolbar input")!;
  await act(async () => {
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")?.set?.call(input, value);
    input.dispatchEvent(new Event("input", { bubbles: true }));
  });
}
beforeEach(async () => {
  vi.clearAllMocks();
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  localStorage.clear();
  await i18n.changeLanguage("en-US");
  useCatalogStore.setState({ profiles, groups: [{ id: "production", name: "Production", sortOrder: 0, collapsed: false, createdAt: "", updatedAt: "" }], selectedProfileId: null, loading: false, errorCode: null,
    reorderProfiles: vi.fn().mockResolvedValue(undefined), reorderGroups: vi.fn().mockResolvedValue(undefined), updateGroup: vi.fn().mockResolvedValue(undefined) });
  useSessionStore.setState({ tabs: [], activeTabId: null });
  container = document.createElement("div"); document.body.append(container); root = createRoot(container);
  await act(async () => { root.render(<Fixture />); });
});
afterEach(async () => {
  await act(async () => { root.unmount(); });
  container.remove(); vi.unstubAllGlobals();
});

describe("server catalog", () => {
  it("shows sync status and sends signed-out users to authentication", async () => {
    expect(container.querySelector(".server-sync-status")?.textContent).toContain(i18n.t("serverManagement.syncLabel"));
    await click(`button[title="${i18n.t("serverManagement.openSignIn")}"]`);
    expect(openAuth).toHaveBeenCalledOnce();
    expect(openSync).not.toHaveBeenCalled();
  });

  it("uses all catalog space, renders each host once, and restores layout preference", async () => {
    expect(container.querySelector(".server-catalog-grid")).not.toBeNull();
    expect(container.querySelectorAll(".host-item")).toHaveLength(24);
    expect(container.querySelector(".server-profile-details")).toBeNull();
    expect(container.querySelector(".host-address")?.textContent).toContain(":2222");
    await click(`button[aria-label="${i18n.t("serverManagement.listView")}"]`);
    expect(container.querySelector(".server-catalog-list")).not.toBeNull();
    expect(localStorage.getItem("runory.serversView")).toBe("list");
    await act(async () => { root.unmount(); root = createRoot(container); root.render(<Fixture />); });
    expect(container.querySelector(".server-catalog-list")).not.toBeNull();
  });

  it("marks jump-host profiles in the server catalog", async () => {
    const target = { ...profiles[1], connectionRoute: { type: "jumpHost" as const, profileId: profiles[0].id } };
    await act(async () => { useCatalogStore.setState({ profiles: [profiles[0], target, ...profiles.slice(2)] }); });

    const targetItem = Array.from(container.querySelectorAll(".host-item")).find((item) => item.textContent?.includes(target.name));
    expect(targetItem?.querySelector(".jump-host-indicator")?.getAttribute("aria-label")).toBe(i18n.t("profile.jumpHostIndicatorNamed", { name: profiles[0].name }));
  });

  it("opens and closes details without connecting, and restores keyboard focus", async () => {
    await click(".host-select");
    expect(container.querySelector(".server-profile-details")?.textContent).toContain("host-0.example.test");
    expect(container.querySelector(".server-profile-actions button:nth-child(2)")?.textContent).toBe(i18n.t("common.edit"));
    expect(container.querySelector(".server-profile-actions button:nth-child(3)")?.textContent).toBe(i18n.t("common.delete"));
    expect(connect).not.toHaveBeenCalled();
    await click(`button[aria-label="${i18n.t("serverManagement.closeDetails")}"]`);
    expect(container.querySelector(".server-profile-details")).toBeNull();
    expect(document.activeElement).toBe(container.querySelector(".host-select"));
    await act(async () => { document.activeElement?.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true })); });
    expect(connect).toHaveBeenCalledExactlyOnceWith("host-0");
  });

  it("supports explicit connect and double-click", async () => {
    await click(".server-host-connect");
    expect(connect).toHaveBeenCalledExactlyOnceWith("host-0");
    connect.mockClear();
    await act(async () => { container.querySelector(".host-select")?.dispatchEvent(new MouseEvent("dblclick", { bubbles: true })); });
    expect(connect).toHaveBeenCalledExactlyOnceWith("host-0");
  });

  it("filters all recent hosts by real timestamps and disables manual ordering", async () => {
    await click(".server-filter-switch button:last-child");
    expect(container.querySelectorAll(".host-item")).toHaveLength(4);
    expect(container.querySelector(".host-name")?.textContent).toBe("Server 3");
    expect(container.querySelectorAll(".host-menu [role='menuitem']")).toHaveLength(8);
    await search("host-1");
    expect(container.querySelectorAll(".host-item")).toHaveLength(1);
    expect(container.querySelector(".host-name")?.textContent).toBe("Server 1");
  });

  it("searches collapsed groups without changing persisted collapse state, and clears no-results", async () => {
    await act(async () => { useCatalogStore.setState({ groups: useCatalogStore.getState().groups.map((group) => ({ ...group, collapsed: true })) }); });
    expect(container.querySelectorAll(".host-item")).toHaveLength(12);
    await search("Production");
    expect(container.querySelectorAll(".host-item")).toHaveLength(12);
    expect(container.querySelector(".group-toggle")?.getAttribute("aria-expanded")).toBe("true");
    expect(useCatalogStore.getState().updateGroup).not.toHaveBeenCalled();
    await search("missing-server");
    expect(container.querySelectorAll(".host-item")).toHaveLength(0);
    expect(container.textContent).toContain(i18n.t("serverManagement.noResults"));
    await click(`button[aria-label="${i18n.t("serverManagement.clearSearch")}"]`);
    expect(container.querySelectorAll(".host-item")).toHaveLength(12);
  });

  it("preserves group-scoped creation and reorder payloads", async () => {
    await click(".group-menu summary");
    await click(".group-menu [role='menuitem']");
    expect(container.querySelector("[role='dialog']")?.getAttribute("data-group")).toBe("production");
    await click(".host-menu summary");
    await click(".host-menu [role='menuitem']:nth-child(3)");
    expect(useCatalogStore.getState().reorderProfiles).toHaveBeenCalledWith("production", ["host-1", "host-0", ...profiles.slice(2, 12).map((profile) => profile.id)]);
  });

  it("counts connected hosts once even with multiple terminals and localizes status", async () => {
    await act(async () => {
      useSessionStore.setState({ tabs: ["a", "b"].map((id) => ({ id, profileId: "host-0", sessionId: `session-${id}`, connectionAttemptId: `attempt-${id}`, state: "connected", view: "terminal" })) });
      await i18n.changeLanguage("zh-CN");
    });
    expect(container.querySelector(".server-connected")?.textContent).toBe(i18n.t("serverManagement.connectedCount", { count: 1 }));
    expect(container.querySelector(".server-host-state")?.textContent).toBe(i18n.t("status.connected"));
    expect(container.querySelector(".server-filter-switch")?.textContent).toContain(i18n.t("serverManagement.allServers"));
  });

  it("shows loading, errors, and empty states without placeholder hosts", async () => {
    await act(async () => { useCatalogStore.setState({ loading: true }); });
    expect(container.querySelectorAll(".host-item")).toHaveLength(0);
    expect(container.querySelector(".server-catalog")?.getAttribute("aria-busy")).toBe("true");
    await act(async () => { useCatalogStore.setState({ loading: false, profiles: [], groups: [], errorCode: "UNKNOWN" }); });
    expect(container.querySelector("[role='alert']")?.textContent).toBe(i18n.t("errors.loadFailed"));
    await click(".server-filter-switch button:last-child");
    expect(container.textContent).toContain(i18n.t("serverManagement.noRecent"));
  });
});
