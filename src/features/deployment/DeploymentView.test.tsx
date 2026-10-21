// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import i18n from "../../i18n";
import { DeploymentView } from "./DeploymentView";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

const apps = [
  { id: "p1", profileId: "test-profile", name: "Shop", repositoryPath: "/srv/shop", remoteUrl: "", branch: "main", build: "none", restart: { kind: "none" }, projectKind: "php", setupStep: 4, setupStatus: "ready", sourceKind: "existingPath", venvPath: "", updatedAtEpochSeconds: 1 },
  { id: "g1", profileId: "test-profile", name: "Edge", repositoryPath: "/etc/nginx/conf.d/runory-proxy-edge.conf", remoteUrl: "", branch: "main", build: "none", restart: { kind: "none" }, projectKind: "reverseProxy", setupStep: 3, setupStatus: "ready", sourceKind: "existingPath", venvPath: "", updatedAtEpochSeconds: 1, proxy: { port: 80, serverName: "", targetUrl: "http://127.0.0.1:3000", hostHeader: "original", websocket: true, maxBodyMb: 50, timeoutSeconds: 60 } },
  { id: "o1", profileId: "test-profile", name: "Legacy", repositoryPath: "/srv/legacy", remoteUrl: "", branch: "main", build: "none", restart: { kind: "none" }, projectKind: "other", setupStep: 4, setupStatus: "ready", sourceKind: "existingPath", venvPath: "", updatedAtEpochSeconds: 1 },
];

let root: Root;
let container: HTMLDivElement;

beforeEach(async () => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  container = document.createElement("div");
  document.body.append(container);
  root = createRoot(container);
  vi.mocked(invoke).mockReset().mockImplementation(async (command) => {
    if (command === "deployment_apps_list") return apps;
    if (command === "plugin_workspace_entries") return [];
    return { success: true, output: "active" };
  });
});

afterEach(async () => {
  await act(async () => { root.unmount(); });
  container.remove();
  vi.unstubAllGlobals();
});

describe.each(["en-US", "zh-CN"])("Deployment project tabs (%s)", (language) => {
  beforeEach(async () => {
    await i18n.changeLanguage(language);
    await act(async () => { root.render(<DeploymentView sessionId="test-session" profileId="test-profile" />); });
    await act(async () => { await Promise.resolve(); });
  });

  const tablist = () => container.querySelector(`[role="tablist"][aria-label="${i18n.t("deployment.projectKinds")}"]`)!;
  const tab = (kind: string) => tablist().querySelector<HTMLButtonElement>(`#deployment-project-${kind}`);

  it("offers one tab per project type and no Net or Other tabs", () => {
    const kinds = ["php", "java", "node", "go", "python", "reverseProxy", "html"] as const;
    expect(Array.from(tablist().querySelectorAll('[role="tab"]')).map((item) => item.id)).toEqual(kinds.map((kind) => `deployment-project-${kind}`));
    for (const kind of kinds) expect(tab(kind)?.textContent).toContain(i18n.t(`deployment.project.${kind}`));
    expect(tab("net")).toBeNull();
    expect(tab("other")).toBeNull();
  });

  it("starts on PHP and only lists apps of the selected type", async () => {
    expect(tab("php")?.getAttribute("aria-selected")).toBe("true");
    expect(container.textContent).toContain("Shop");
    expect(container.textContent).not.toContain("Edge");
    await act(async () => { tab("reverseProxy")!.click(); });
    expect(tab("reverseProxy")?.getAttribute("aria-selected")).toBe("true");
    expect(tab("php")?.getAttribute("aria-selected")).toBe("false");
    expect(container.textContent).toContain("Edge");
    expect(container.textContent).not.toContain("Shop");
    expect(container.textContent).not.toContain("Legacy");
  });

  it("shows the matching empty state for every project type", async () => {
    const empty: Record<string, string> = {
      java: "deployment.java.emptyTitle",
      node: "deployment.node.emptyTitle",
      go: "deployment.go.emptyTitle",
      python: "deployment.python.emptyTitle",
      html: "deployment.html.emptyTitle",
    };
    for (const [kind, key] of Object.entries(empty)) {
      await act(async () => { tab(kind)!.click(); });
      expect(container.textContent, kind).toContain(i18n.t(key));
    }
  });
});
