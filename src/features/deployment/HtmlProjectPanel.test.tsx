// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import i18n from "../../i18n";
import type { DeploymentApp } from "../../types/infrastructure";
import { draftFromHtmlApp, emptyHtmlDraft, htmlConfigFromDraft, validateHtmlDraft, validateHtmlStep } from "./html-project-draft";
import { HtmlProjectPanel } from "./HtmlProjectPanel";

const invoke = vi.hoisted(() => vi.fn(async (command: string) => {
  if (command === "plugin_workspace_entries") return [];
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

const readyDraft = () => ({ ...emptyHtmlDraft(), name: "docs", projectPath: "/srv/docs", setupStep: 4 as const });

function htmlApp(overrides: Partial<DeploymentApp> = {}): DeploymentApp {
  return {
    id: "app-1",
    profileId: "profile-1",
    name: "docs",
    repositoryPath: "/srv/docs",
    remoteUrl: "",
    branch: "main",
    build: "none",
    restart: { kind: "none" },
    projectKind: "html",
    html: htmlConfigFromDraft(readyDraft()),
    setupStep: 4,
    setupStatus: "provisioned",
    sourceKind: "existingPath",
    venvPath: "",
    updatedAtEpochSeconds: 1,
    ...overrides,
  };
}

describe("HTML project draft validation", () => {
  it("accepts the default final-step draft", () => {
    expect(validateHtmlDraft(readyDraft())).toBeNull();
  });

  it("rejects paths and web roots that cannot be written safely into nginx", () => {
    expect(validateHtmlStep({ ...readyDraft(), projectPath: "/srv/my docs" }, 1)).toBe("deployment.html.validation.pathChars");
    expect(validateHtmlStep({ ...readyDraft(), projectPath: "srv/docs" }, 1)).toBe("deployment.validation.repositoryPath");
    expect(validateHtmlStep({ ...readyDraft(), rootDir: "../dist" }, 3)).toBe("deployment.html.validation.rootDir");
    expect(validateHtmlStep({ ...readyDraft(), rootDir: "dist; autoindex on" }, 3)).toBe("deployment.html.validation.rootDir");
    expect(validateHtmlStep({ ...readyDraft(), rootDir: "/dist/" }, 3)).toBeNull();
  });

  it("validates default documents, port and domain names", () => {
    expect(validateHtmlStep({ ...readyDraft(), indexFiles: "" }, 3)).toBe("deployment.html.validation.indexFiles");
    expect(validateHtmlStep({ ...readyDraft(), indexFiles: "dir/index.html" }, 3)).toBe("deployment.html.validation.indexFiles");
    expect(validateHtmlDraft({ ...readyDraft(), port: "0" })).toBe("deployment.html.validation.port");
    expect(validateHtmlDraft({ ...readyDraft(), serverName: "example.com *.example.com localhost" })).toBeNull();
    expect(validateHtmlDraft({ ...readyDraft(), serverName: "a.com; root /;" })).toBe("deployment.html.validation.serverName");
  });

  it("round-trips a saved project through the draft", () => {
    const draft = { ...readyDraft(), rootDir: "/dist/", port: "8080", serverName: " a.com   b.com ", spaFallback: true, gzip: false };
    const config = htmlConfigFromDraft(draft);
    expect(config).toMatchObject({ rootDir: "dist", port: 8080, serverName: "a.com b.com", spaFallback: true, gzip: false, cacheStatic: true });
    const restored = draftFromHtmlApp(htmlApp({ html: config, setupStatus: "ready" }));
    expect(restored.setupStep).toBe(4);
    expect(htmlConfigFromDraft(restored)).toEqual(config);
  });
});

describe("HtmlProjectPanel", () => {
  it("renders the empty state with a single add action", async () => {
    await act(async () => {
      root.render(<HtmlProjectPanel sessionId="session-1" profileId="profile-1" apps={[]} onChanged={async () => undefined} />);
    });
    expect(container.textContent).toContain(i18n.t("deployment.html.emptyTitle"));
    expect(container.querySelectorAll("button").length).toBe(1);
  });

  it("disables deploy while disconnected", async () => {
    await act(async () => {
      root.render(<HtmlProjectPanel sessionId={null} profileId="profile-1" apps={[htmlApp()]} onChanged={async () => undefined} />);
    });
    const deploy = Array.from(container.querySelectorAll("button")).find((button) => button.textContent?.includes(i18n.t("deployment.python.deploy")));
    expect(deploy?.hasAttribute("disabled")).toBe(true);
  });

  it("asks for confirmation before deploying and records the result", async () => {
    await act(async () => {
      root.render(<HtmlProjectPanel sessionId="session-1" profileId="profile-1" apps={[htmlApp()]} onChanged={async () => undefined} />);
    });
    expect(container.textContent).toContain(i18n.t("deployment.html.status.provisioned"));
    const deploy = Array.from(container.querySelectorAll("button")).find((button) => button.textContent?.includes(i18n.t("deployment.python.deploy")));
    await act(async () => { deploy?.click(); });
    expect(invoke).not.toHaveBeenCalledWith("deployment_html_provision", expect.anything());
    expect(document.body.textContent).toContain(i18n.t("deployment.html.provisionConfirm", { name: "docs" }));

    const confirm = Array.from(document.querySelectorAll("button")).find((button) => button.textContent === i18n.t("deployment.confirm"));
    await act(async () => { confirm?.click(); });
    expect(invoke).toHaveBeenCalledWith("deployment_html_provision", {
      request: { sessionId: "session-1", name: "docs", projectPath: "/srv/docs", html: htmlConfigFromDraft(readyDraft()) },
    });
    const statuses = invoke.mock.calls
      .filter((call) => call[0] === "deployment_apps_upsert")
      .map((call) => (call as unknown as [string, { request: { setupStatus: string } }])[1].request.setupStatus);
    expect(statuses).toEqual(["provisioning", "provisioned"]);
  });

  it("does not offer deploy-only actions for unfinished drafts", async () => {
    await act(async () => {
      root.render(<HtmlProjectPanel sessionId="session-1" profileId="profile-1" apps={[htmlApp({ setupStatus: "draft", setupStep: 2, html: null })]} onChanged={async () => undefined} />);
    });
    expect(container.textContent).toContain(i18n.t("deployment.python.continue"));
    expect(container.textContent).not.toContain(i18n.t("deployment.python.logs"));
  });
});
