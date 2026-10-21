// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import i18n from "../../i18n";
import type { DeploymentApp } from "../../types/infrastructure";
import { draftFromGoApp, emptyGoDraft, goConfigFromDraft, validateGoDraft, validateGoStep } from "./go-project-draft";
import { GoProjectPanel } from "./GoProjectPanel";

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

const readyDraft = () => ({ ...emptyGoDraft(), name: "api", projectPath: "/srv/api", executable: "bin/app", setupStep: 4 as const });

function goApp(overrides: Partial<DeploymentApp> = {}): DeploymentApp {
  return {
    id: "app-1",
    profileId: "profile-1",
    name: "api",
    repositoryPath: "/srv/api",
    remoteUrl: "",
    branch: "main",
    build: "none",
    restart: { kind: "systemd", service: "runory-go-api.service" },
    projectKind: "go",
    go: goConfigFromDraft(readyDraft()),
    setupStep: 4,
    setupStatus: "provisioned",
    sourceKind: "existingPath",
    venvPath: "",
    updatedAtEpochSeconds: 1,
    ...overrides,
  };
}

describe("Go project draft validation", () => {
  it("accepts the default final-step draft once an executable is chosen", () => {
    expect(validateGoDraft(readyDraft())).toBeNull();
    expect(validateGoStep({ ...readyDraft(), executable: "" }, 3)).toBe("deployment.go.validation.executable");
  });

  it("rejects paths and executables that cannot be written safely into a unit", () => {
    expect(validateGoStep({ ...readyDraft(), projectPath: "/srv/my api" }, 1)).toBe("deployment.go.validation.pathChars");
    expect(validateGoStep({ ...readyDraft(), projectPath: "srv/api" }, 1)).toBe("deployment.validation.repositoryPath");
    expect(validateGoStep({ ...readyDraft(), executable: "bin/" }, 3)).toBe("deployment.go.validation.executable");
    expect(validateGoStep({ ...readyDraft(), executable: "../app" }, 3)).toBe("deployment.go.validation.executable");
    expect(validateGoStep({ ...readyDraft(), executable: "-app" }, 3)).toBe("deployment.go.validation.executable");
    expect(validateGoStep({ ...readyDraft(), executable: "/opt/app/server" }, 3)).toBeNull();
  });

  it("validates the Go toolchain and build package", () => {
    expect(validateGoStep({ ...readyDraft(), goBin: "/usr/local/go/bin/go" }, 3)).toBeNull();
    expect(validateGoStep({ ...readyDraft(), goBin: "gofmt" }, 3)).toBe("deployment.go.validation.goBin");
    expect(validateGoStep({ ...readyDraft(), buildPackage: "./cmd/server" }, 3)).toBeNull();
    expect(validateGoStep({ ...readyDraft(), buildPackage: "-o" }, 3)).toBe("deployment.go.validation.buildPackage");
    expect(validateGoStep({ ...readyDraft(), buildPackage: "/abs" }, 3)).toBe("deployment.go.validation.buildPackage");
  });

  it("validates port, arguments and environment variables", () => {
    expect(validateGoDraft({ ...readyDraft(), port: "0" })).toBe("deployment.go.validation.port");
    expect(validateGoDraft({ ...readyDraft(), runUser: "bad user" })).toBe("deployment.go.validation.runUser");
    expect(validateGoDraft({ ...readyDraft(), appArgs: "--x=a|b" })).toBe("deployment.go.validation.appArgs");
    expect(validateGoDraft({ ...readyDraft(), envVars: "GIN_MODE=release APP_ENV=prod" })).toBeNull();
    for (const bad of ["NOEQUALS", "PORT=1", "PATH=/x", "LD_PRELOAD=/x.so", "A=$HOME", "1A=x"]) {
      expect(validateGoDraft({ ...readyDraft(), envVars: bad }), bad).toBe("deployment.go.validation.envVars");
    }
  });

  it("round-trips a saved project through the draft", () => {
    const draft = { ...readyDraft(), envVars: " A=1   B=2 ", appArgs: " --a   --b ", port: "9000", buildEnabled: false };
    const config = goConfigFromDraft(draft);
    expect(config).toMatchObject({ envVars: "A=1 B=2", appArgs: "--a --b", port: 9000, buildEnabled: false, goBin: "go", buildPackage: "." });
    const restored = draftFromGoApp(goApp({ go: config, setupStatus: "ready" }));
    expect(restored.setupStep).toBe(4);
    expect(goConfigFromDraft(restored)).toEqual(config);
  });
});

describe("GoProjectPanel", () => {
  it("renders the empty state with a single add action", async () => {
    await act(async () => {
      root.render(<GoProjectPanel sessionId="session-1" profileId="profile-1" apps={[]} onChanged={async () => undefined} />);
    });
    expect(container.textContent).toContain(i18n.t("deployment.go.emptyTitle"));
    expect(container.querySelectorAll("button").length).toBe(1);
  });

  it("disables deploy while disconnected", async () => {
    await act(async () => {
      root.render(<GoProjectPanel sessionId={null} profileId="profile-1" apps={[goApp()]} onChanged={async () => undefined} />);
    });
    const deploy = Array.from(container.querySelectorAll("button")).find((button) => button.textContent?.includes(i18n.t("deployment.python.deploy")));
    expect(deploy?.hasAttribute("disabled")).toBe(true);
    expect(invoke).not.toHaveBeenCalledWith("deployment_python_status", expect.anything());
  });

  it("shows the live state and confirms before controlling the service", async () => {
    await act(async () => {
      root.render(<GoProjectPanel sessionId="session-1" profileId="profile-1" apps={[goApp()]} onChanged={async () => undefined} />);
    });
    expect(container.textContent).toContain("bin/app");
    expect(invoke).toHaveBeenCalledWith("deployment_python_status", { request: { sessionId: "session-1", service: "runory-go-api.service" } });
    expect(container.textContent).toContain(i18n.t("deployment.python.status.active"));

    const restart = Array.from(container.querySelectorAll("button")).find((button) => button.textContent === i18n.t("deployment.python.action.restart"));
    await act(async () => { restart?.click(); });
    expect(invoke).not.toHaveBeenCalledWith("deployment_go_control", expect.anything());
    expect(document.body.textContent).toContain(i18n.t("deployment.go.controlConfirm", { name: "api", action: i18n.t("deployment.python.action.restart") }));

    const confirm = Array.from(document.querySelectorAll("button")).find((button) => button.textContent === i18n.t("deployment.confirm"));
    await act(async () => { confirm?.click(); });
    expect(invoke).toHaveBeenCalledWith("deployment_go_control", { request: { sessionId: "session-1", appId: "app-1", action: "restart" } });
  });

  it("does not offer service controls for unfinished drafts", async () => {
    await act(async () => {
      root.render(<GoProjectPanel sessionId="session-1" profileId="profile-1" apps={[goApp({ setupStatus: "draft", setupStep: 2, go: null, restart: { kind: "none" } })]} onChanged={async () => undefined} />);
    });
    expect(container.textContent).toContain(i18n.t("deployment.python.continue"));
    expect(container.textContent).not.toContain(i18n.t("deployment.python.logs"));
  });
});
