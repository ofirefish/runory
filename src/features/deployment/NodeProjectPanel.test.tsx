// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import i18n from "../../i18n";
import type { DeploymentApp } from "../../types/infrastructure";
import { draftFromNodeApp, emptyNodeDraft, nodeConfigFromDraft, validateNodeDraft, validateNodeStep } from "./node-project-draft";
import { NodeProjectPanel } from "./NodeProjectPanel";

const invoke = vi.hoisted(() => vi.fn(async (command: string) => {
  if (command === "plugin_workspace_entries") return [];
  if (command === "deployment_python_status") return { success: true, output: "active" };
  return { success: true, output: "ok" };
}));
vi.mock("@tauri-apps/api/core", () => ({ invoke }));

let root: Root;
let container: HTMLDivElement;

beforeEach(async () => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  invoke.mockClear();
  container = document.createElement("div");
  document.body.append(container);
  root = createRoot(container);
  await i18n.changeLanguage("zh-CN");
});

afterEach(async () => {
  await act(async () => { root.unmount(); });
  container.remove();
  vi.unstubAllGlobals();
});

const readyDraft = () => ({ ...emptyNodeDraft(), name: "web", projectPath: "/srv/web", setupStep: 4 as const });

function nodeApp(overrides: Partial<DeploymentApp> = {}): DeploymentApp {
  return {
    id: "app-1",
    profileId: "profile-1",
    name: "web",
    repositoryPath: "/srv/web",
    remoteUrl: "",
    branch: "main",
    build: "none",
    restart: { kind: "systemd", service: "runory-node-web.service" },
    projectKind: "node",
    node: nodeConfigFromDraft(readyDraft()),
    setupStep: 4,
    setupStatus: "provisioned",
    sourceKind: "existingPath",
    venvPath: "",
    updatedAtEpochSeconds: 1,
    ...overrides,
  };
}

describe("Node project draft validation", () => {
  it("accepts the default final-step draft", () => {
    expect(validateNodeDraft(readyDraft())).toBeNull();
  });

  it("rejects paths that cannot be written safely into a unit", () => {
    expect(validateNodeStep({ ...readyDraft(), projectPath: "/srv/my web" }, 1)).toBe("deployment.node.validation.pathChars");
    expect(validateNodeStep({ ...readyDraft(), projectPath: "srv/web" }, 1)).toBe("deployment.validation.repositoryPath");
  });

  it("requires a valid node binary and a start target for the chosen mode", () => {
    expect(validateNodeStep({ ...readyDraft(), nodeBin: "/home/a/.nvm/versions/node/v20.1.0/bin/node" }, 3)).toBeNull();
    expect(validateNodeStep({ ...readyDraft(), nodeBin: "nodejs" }, 3)).toBe("deployment.node.validation.nodeBin");
    expect(validateNodeStep({ ...readyDraft(), nodeBin: "/usr/bin/python3" }, 3)).toBe("deployment.node.validation.nodeBin");
    expect(validateNodeStep({ ...readyDraft(), script: "" }, 3)).toBe("deployment.node.validation.script");
    expect(validateNodeStep({ ...readyDraft(), script: "a b" }, 3)).toBe("deployment.node.validation.script");
    expect(validateNodeStep({ ...readyDraft(), buildScript: "a;b" }, 3)).toBe("deployment.node.validation.buildScript");
    const entry = { ...readyDraft(), startMode: "entry" as const };
    expect(validateNodeStep(entry, 3)).toBe("deployment.node.validation.entryFile");
    expect(validateNodeStep({ ...entry, entryFile: "dist/server.js" }, 3)).toBeNull();
    expect(validateNodeStep({ ...entry, entryFile: "../server.js" }, 3)).toBe("deployment.node.validation.entryFile");
    expect(validateNodeStep({ ...entry, entryFile: "server.ts" }, 3)).toBe("deployment.node.validation.entryFile");
  });

  it("validates port, environment, run user and arguments", () => {
    expect(validateNodeDraft({ ...readyDraft(), port: "0" })).toBe("deployment.node.validation.port");
    expect(validateNodeDraft({ ...readyDraft(), nodeEnv: "Production" })).toBe("deployment.node.validation.nodeEnv");
    expect(validateNodeDraft({ ...readyDraft(), runUser: "bad user" })).toBe("deployment.node.validation.runUser");
    expect(validateNodeDraft({ ...readyDraft(), appArgs: "--host=127.0.0.1 --verbose" })).toBeNull();
    expect(validateNodeDraft({ ...readyDraft(), appArgs: "--x=$HOME" })).toBe("deployment.node.validation.appArgs");
  });

  it("round-trips a saved project through the draft", () => {
    const draft = { ...readyDraft(), packageManager: "pnpm" as const, buildScript: " build ", appArgs: " --a   --b ", port: "4000" };
    const config = nodeConfigFromDraft(draft);
    expect(config).toMatchObject({ packageManager: "pnpm", buildScript: "build", appArgs: "--a --b", port: 4000, script: "start", entryFile: "" });
    const restored = draftFromNodeApp(nodeApp({ node: config, setupStatus: "ready" }));
    expect(restored.setupStep).toBe(4);
    expect(nodeConfigFromDraft(restored)).toEqual(config);
  });
});

describe("NodeProjectPanel", () => {
  it("renders the empty state with a single add action", async () => {
    await act(async () => {
      root.render(<NodeProjectPanel sessionId="session-1" profileId="profile-1" apps={[]} onChanged={async () => undefined} />);
    });
    expect(container.textContent).toContain(i18n.t("deployment.node.emptyTitle"));
    expect(container.querySelectorAll("button").length).toBe(1);
  });

  it("disables deploy while disconnected", async () => {
    await act(async () => {
      root.render(<NodeProjectPanel sessionId={null} profileId="profile-1" apps={[nodeApp()]} onChanged={async () => undefined} />);
    });
    const deploy = Array.from(container.querySelectorAll("button")).find((button) => button.textContent?.includes(i18n.t("deployment.python.deploy")));
    expect(deploy?.hasAttribute("disabled")).toBe(true);
    expect(invoke).not.toHaveBeenCalledWith("deployment_python_status", expect.anything());
  });

  it("shows the start command, live state and confirms before controlling the service", async () => {
    await act(async () => {
      root.render(<NodeProjectPanel sessionId="session-1" profileId="profile-1" apps={[nodeApp()]} onChanged={async () => undefined} />);
    });
    expect(container.textContent).toContain("npm run start");
    expect(invoke).toHaveBeenCalledWith("deployment_python_status", { request: { sessionId: "session-1", service: "runory-node-web.service" } });
    expect(container.textContent).toContain(i18n.t("deployment.python.status.active"));

    const restart = Array.from(container.querySelectorAll("button")).find((button) => button.textContent === i18n.t("deployment.python.action.restart"));
    await act(async () => { restart?.click(); });
    expect(invoke).not.toHaveBeenCalledWith("deployment_node_control", expect.anything());
    expect(document.body.textContent).toContain(i18n.t("deployment.node.controlConfirm", { name: "web", action: i18n.t("deployment.python.action.restart") }));

    const confirm = Array.from(document.querySelectorAll("button")).find((button) => button.textContent === i18n.t("deployment.confirm"));
    await act(async () => { confirm?.click(); });
    expect(invoke).toHaveBeenCalledWith("deployment_node_control", { request: { sessionId: "session-1", appId: "app-1", action: "restart" } });
  });

  it("does not offer service controls for unfinished drafts", async () => {
    await act(async () => {
      root.render(<NodeProjectPanel sessionId="session-1" profileId="profile-1" apps={[nodeApp({ setupStatus: "draft", setupStep: 2, node: null, restart: { kind: "none" } })]} onChanged={async () => undefined} />);
    });
    expect(container.textContent).toContain(i18n.t("deployment.python.continue"));
    expect(container.textContent).not.toContain(i18n.t("deployment.python.logs"));
  });
});
