// @vitest-environment jsdom
import { act, type ComponentProps } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import i18n from "../../i18n";
import { useCatalogStore } from "../../stores/catalog-store";
import { useSessionStore, type SessionTab } from "../../stores/session-store";
import type { TerminalView } from "../../features/terminal/TerminalView";
import { Workspace } from "./Workspace";

const terminalActions = vi.hoisted(() => ({ copy: vi.fn(), paste: vi.fn() }));
vi.mock("../context-panel/ContextPanel", () => ({ ContextPanel: () => null }));
vi.mock("../../features/sessions/ConnectionDialog", () => ({ ConnectionDialog: () => null }));
vi.mock("../../features/profiles/ProfileDialog", () => ({ ProfileDialog: () => null }));
vi.mock("../../features/files/FilesView", () => ({ FilesView: () => <input data-testid="files-state" defaultValue="/var/log" /> }));
vi.mock("../../features/dashboard/DashboardView", () => ({ DashboardView: () => <div data-testid="dashboard" /> }));
vi.mock("../../features/operations/OperationsView", () => ({ OperationsView: () => <div data-testid="operations" /> }));
vi.mock("../../features/deployment/DeploymentView", () => ({ DeploymentView: () => <div data-testid="deployment" /> }));
vi.mock("../../features/terminal/TerminalView", async () => {
  const { forwardRef, useImperativeHandle, useState } = await import("react");
  const { TerminalToolbar } = await import("../../features/terminal/TerminalToolbar");
  return { TerminalView: forwardRef(function TestTerminal({ active, toolbarHost, sessionId }: ComponentProps<typeof TerminalView>, ref) {
    const [searchOpen, setSearchOpen] = useState(false);
    useImperativeHandle(ref, () => ({ write: () => undefined, dimensions: () => ({ cols: 120, rows: 34 }) }));
    return <div data-testid="terminal" data-active={active}>
      <TerminalToolbar toolbarHost={toolbarHost} searchOpen={searchOpen} query="" result={{ resultIndex: -1, resultCount: 0 }} clipboardError={false} onOpenSearch={() => setSearchOpen(true)} onCloseSearch={() => setSearchOpen(false)} onQueryChange={() => undefined} onFindNext={() => undefined} onFindPrevious={() => undefined} onCopy={() => terminalActions.copy(sessionId)} onPaste={() => terminalActions.paste(sessionId)} />
    </div>;
  }) };
});
vi.mock("../../lib/tauri/ssh", () => ({ connectSsh: vi.fn(), disconnectSsh: vi.fn(), reconnectSsh: vi.fn(), testSsh: vi.fn() }));
vi.mock("../../lib/tauri/tunnels", () => ({ sessionTunnelImpact: vi.fn().mockResolvedValue([]) }));

let container: HTMLDivElement;
let tabsHost: HTMLDivElement;
let root: Root;
const tab = (id: string): SessionTab => ({
  id, profileId: "shared-profile", title: id, sessionId: `session-${id}`,
  connectionAttemptId: `attempt-${id}`, state: "connected", view: "terminal",
});
async function render(visible = true) {
  await act(async () => { root.render(<Workspace visible={visible} onSelectServer={() => undefined} titlebarTabsHost={tabsHost} onShowSessions={() => undefined} />); });
  await waitFor(() => container.querySelector('[data-testid="terminal"]') !== null, "terminal");
}
async function waitFor(predicate: () => boolean, label: string) {
  const deadline = Date.now() + 3000;
  while (Date.now() < deadline) {
    if (predicate()) return;
    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 16));
    });
  }
  throw new Error(`Timed out waiting for ${label}`);
}
async function selectSession(id: string) {
  const button = tabsHost.querySelector<HTMLButtonElement>(`.workspace-tab-select[title="${id}"]`)!;
  await act(async () => { button.click(); });
}
async function selectView(labelKey: string) {
  const button = Array.from(container.querySelectorAll<HTMLButtonElement>(".session-workspace.active .dock-tabs button"))
    .find((element) => element.textContent === i18n.t(labelKey));
  expect(button).toBeDefined();
  await act(async () => { button?.click(); });
  await waitFor(() => {
    const current = container.querySelector('.session-workspace.active .dock-tabs [aria-current="page"]')?.textContent;
    return current === i18n.t(labelKey) || container.querySelector(`[data-testid="${labelKey.split(".")[0]}"]`) !== null || labelKey === "terminal.title";
  }, labelKey);
}
const activeView = () => container.querySelector('.session-workspace.active .dock-tabs [aria-current="page"]')?.textContent;

beforeEach(async () => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  localStorage.clear();
  vi.clearAllMocks();
  await i18n.changeLanguage("en-US");
  useCatalogStore.setState({ profiles: [], groups: [], selectedProfileId: null });
  useSessionStore.setState({ tabs: [tab("one"), tab("two")], activeTabId: "one" });
  container = document.createElement("div");
  tabsHost = document.createElement("div");
  document.body.append(container, tabsHost);
  root = createRoot(container);
});
afterEach(async () => {
  await act(async () => { root.unmount(); });
  container.remove();
  tabsHost.remove();
  localStorage.clear();
  vi.unstubAllGlobals();
});

describe("per-session workspace views", () => {
  it("places terminal actions at the right of the tab strip and binds them to their session", async () => {
    await render();
    const action = (key: string) => container.querySelector<HTMLButtonElement>(`.session-workspace.active .dock-actions button[aria-label="${i18n.t(key)}"]`)!;
    expect(container.querySelector('.session-workspace.active .dock-actions')?.lastElementChild?.querySelectorAll("button")).toHaveLength(3);
    expect(container.querySelector('[data-testid="terminal"] button')).toBeNull();
    await act(async () => { action("terminal.copySelection").click(); action("terminal.paste").click(); });
    expect(terminalActions.copy).toHaveBeenLastCalledWith("session-one");
    expect(terminalActions.paste).toHaveBeenLastCalledWith("session-one");
    await act(async () => { action("terminal.search").click(); });
    expect(container.querySelector('[data-testid="terminal"] input')?.getAttribute("aria-label")).toBe(i18n.t("terminal.searchInput"));
    expect(action("terminal.search").getAttribute("aria-expanded")).toBe("true");
    await selectSession("two");
    expect(action("terminal.search").getAttribute("aria-expanded")).toBe("false");
    await act(async () => { action("terminal.copySelection").click(); action("terminal.paste").click(); });
    expect(terminalActions.copy).toHaveBeenLastCalledWith("session-two");
    expect(terminalActions.paste).toHaveBeenLastCalledWith("session-two");
    await selectSession("one");
    expect(action("terminal.search").getAttribute("aria-expanded")).toBe("true");
  });

  it.each(["files", "dashboard", "operations", "deployment"] as const)("hides terminal actions on %s and restores them on Terminal", async (view) => {
    await render();
    const host = container.querySelector<HTMLElement>('.session-workspace.active .dock-actions')!.lastElementChild as HTMLElement;
    expect(host.hidden).toBe(false);
    await selectView(`${view}.title`);
    expect(host.hidden).toBe(true);
    await selectView("terminal.title");
    expect(host.hidden).toBe(false);
    await render(false);
    expect(host.hidden).toBe(true);
    await render(true);
    expect(host.hidden).toBe(false);
  });

  it.each(["files", "dashboard", "operations", "deployment"] as const)("restores %s independently even for sessions on the same profile", async (view) => {
    await render();
    const terminals = Array.from(container.querySelectorAll('[data-testid="terminal"]'));
    await selectSession("two");
    await selectView(`${view}.title`);
    expect(useSessionStore.getState().tabs.map((session) => session.view)).toEqual(["terminal", view]);
    expect(terminals[1].getAttribute("data-active")).toBe("false");

    await selectSession("one");
    expect(activeView()).toBe(i18n.t("terminal.title"));
    expect(terminals[0].getAttribute("data-active")).toBe("true");
    await selectSession("two");
    expect(activeView()).toBe(i18n.t(`${view}.title`));
    expect(terminals[0].getAttribute("data-active")).toBe("false");
    expect(Array.from(container.querySelectorAll('[data-testid="terminal"]'))).toEqual(terminals);

    await render(false);
    await render(true);
    expect(activeView()).toBe(i18n.t(`${view}.title`));
  });

  it("keeps a background Files view mounted when another session changes views", async () => {
    await render();
    await selectSession("two");
    await selectView("files.title");
    const files = container.querySelector<HTMLInputElement>('[data-testid="files-state"]')!;
    files.value = "/etc/nginx";
    await selectSession("one");
    await selectView("dashboard.title");
    await selectView("terminal.title");
    await selectSession("two");
    expect(container.querySelector('[data-testid="files-state"]')).toBe(files);
    expect(files.value).toBe("/etc/nginx");
    expect(activeView()).toBe(i18n.t("files.title"));
  });

  it("uses session metadata instead of the legacy global view and restores the adjacent session on close", async () => {
    localStorage.setItem("runory.workspaceView", "deployment");
    await render();
    expect(activeView()).toBe(i18n.t("terminal.title"));
    await selectView("files.title");
    await act(async () => { useSessionStore.getState().addTab(tab("three")); });
    expect(activeView()).toBe(i18n.t("terminal.title"));
    await selectSession("two");
    await selectView("dashboard.title");
    await selectSession("one");
    const closeButton = tabsHost.querySelector<HTMLButtonElement>(".workspace-tab-close")!;
    await act(async () => { closeButton.click(); });
    expect(useSessionStore.getState().activeTabId).toBe("two");
    expect(activeView()).toBe(i18n.t("dashboard.title"));
  });
});
