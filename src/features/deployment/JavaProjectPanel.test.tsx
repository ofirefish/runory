// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import i18n from "../../i18n";
import type { DeploymentApp } from "../../types/infrastructure";
import { draftFromJavaApp, emptyJavaDraft, javaConfigFromDraft, validateJavaDraft, validateJavaStep } from "./java-project-draft";
import { JavaProjectPanel } from "./JavaProjectPanel";

const invoke = vi.hoisted(() => vi.fn(async (command: string) => {
  if (command === "plugin_workspace_entries") return [];
  if (command === "deployment_python_status") return { success: true, output: "active" };
  if (command === "deployment_list_java_runtimes") return [
    { binary: "/usr/lib/jvm/java-17/bin/java", version: "17.0.9", isDefault: true },
    { binary: "/usr/lib/jvm/java-21/bin/java", version: "21.0.1", isDefault: false },
  ];
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

const readyDraft = () => ({ ...emptyJavaDraft(), name: "orders", projectPath: "/srv/orders", jarPath: "target/app.jar", setupStep: 4 as const });

function javaApp(overrides: Partial<DeploymentApp> = {}): DeploymentApp {
  return {
    id: "app-1",
    profileId: "profile-1",
    name: "orders",
    repositoryPath: "/srv/orders",
    remoteUrl: "",
    branch: "main",
    build: "none",
    restart: { kind: "systemd", service: "runory-java-orders.service" },
    projectKind: "java",
    java: javaConfigFromDraft(readyDraft()),
    setupStep: 4,
    setupStatus: "provisioned",
    sourceKind: "existingPath",
    venvPath: "",
    updatedAtEpochSeconds: 1,
    ...overrides,
  };
}

describe("Java project draft validation", () => {
  it("accepts the default final-step draft once a jar is chosen", () => {
    expect(validateJavaDraft(readyDraft())).toBeNull();
    expect(validateJavaStep({ ...readyDraft(), jarPath: "" }, 3)).toBe("deployment.java.validation.jarPath");
  });

  it("rejects paths and jar names that cannot be written safely into a unit", () => {
    expect(validateJavaStep({ ...readyDraft(), projectPath: "/srv/my app" }, 1)).toBe("deployment.java.validation.pathChars");
    expect(validateJavaStep({ ...readyDraft(), projectPath: "srv/orders" }, 1)).toBe("deployment.validation.repositoryPath");
    expect(validateJavaStep({ ...readyDraft(), jarPath: "app.war" }, 3)).toBe("deployment.java.validation.jarPath");
    expect(validateJavaStep({ ...readyDraft(), jarPath: "../app.jar" }, 3)).toBe("deployment.java.validation.jarPath");
    expect(validateJavaStep({ ...readyDraft(), jarPath: "/opt/app/app.jar" }, 3)).toBeNull();
  });

  it("accepts java on PATH or an absolute JDK path only", () => {
    expect(validateJavaStep({ ...readyDraft(), javaBin: "/usr/lib/jvm/jdk-21/bin/java" }, 3)).toBeNull();
    expect(validateJavaStep({ ...readyDraft(), javaBin: "javac" }, 3)).toBe("deployment.java.validation.javaBin");
    expect(validateJavaStep({ ...readyDraft(), javaBin: "/usr/bin/python3" }, 3)).toBe("deployment.java.validation.javaBin");
  });

  it("validates port, heap and option strings", () => {
    expect(validateJavaDraft({ ...readyDraft(), port: "0" })).toBe("deployment.java.validation.port");
    expect(validateJavaDraft({ ...readyDraft(), xmsMb: "1024", xmxMb: "512" })).toBe("deployment.java.validation.heap");
    expect(validateJavaDraft({ ...readyDraft(), xmxMb: "8" })).toBe("deployment.java.validation.heap");
    expect(validateJavaDraft({ ...readyDraft(), jvmArgs: "-XX:+UseG1GC -Duser.timezone=Asia/Shanghai" })).toBeNull();
    expect(validateJavaDraft({ ...readyDraft(), jvmArgs: "-Xmx2g" })).toBe("deployment.java.validation.jvmArgs");
    expect(validateJavaDraft({ ...readyDraft(), jvmArgs: "-agentlib:jdwp=transport=dt_socket" })).toBe("deployment.java.validation.jvmArgs");
    expect(validateJavaDraft({ ...readyDraft(), jvmArgs: "-Dx=$(id)" })).toBe("deployment.java.validation.jvmArgs");
    expect(validateJavaDraft({ ...readyDraft(), appArgs: "--a=b|c" })).toBe("deployment.java.validation.appArgs");
    expect(validateJavaDraft({ ...readyDraft(), runUser: "bad user" })).toBe("deployment.java.validation.runUser");
  });

  it("round-trips a saved project through the draft", () => {
    const draft = { ...readyDraft(), javaBin: "/usr/lib/jvm/jdk-17/bin/java", jvmArgs: " -XX:+UseG1GC   -Xss512k ", port: "9000", buildTool: "maven" as const };
    const config = javaConfigFromDraft(draft);
    expect(config).toMatchObject({ jvmArgs: "-XX:+UseG1GC -Xss512k", port: 9000, buildTool: "maven", xmsMb: 256, xmxMb: 512 });
    const restored = draftFromJavaApp(javaApp({ java: config, setupStatus: "ready" }));
    expect(restored.setupStep).toBe(4);
    expect(javaConfigFromDraft(restored)).toEqual(config);
  });
});

describe("JavaProjectPanel", () => {
  it("renders the empty state with add and JDK management actions", async () => {
    await act(async () => {
      root.render(<JavaProjectPanel sessionId="session-1" profileId="profile-1" apps={[]} onChanged={async () => undefined} />);
    });
    expect(container.textContent).toContain(i18n.t("deployment.java.emptyTitle"));
    expect(container.querySelectorAll("button").length).toBe(2);
  });

  it("disables deploy while disconnected", async () => {
    await act(async () => {
      root.render(<JavaProjectPanel sessionId={null} profileId="profile-1" apps={[javaApp()]} onChanged={async () => undefined} />);
    });
    const deploy = Array.from(container.querySelectorAll("button")).find((button) => button.textContent?.includes(i18n.t("deployment.python.deploy")));
    expect(deploy?.hasAttribute("disabled")).toBe(true);
    expect(invoke).not.toHaveBeenCalledWith("deployment_python_status", expect.anything());
  });

  it("shows the live service state and asks for confirmation before controlling the service", async () => {
    await act(async () => {
      root.render(<JavaProjectPanel sessionId="session-1" profileId="profile-1" apps={[javaApp()]} onChanged={async () => undefined} />);
    });
    expect(invoke).toHaveBeenCalledWith("deployment_python_status", { request: { sessionId: "session-1", service: "runory-java-orders.service" } });
    expect(container.textContent).toContain(i18n.t("deployment.python.status.active"));

    const restart = Array.from(container.querySelectorAll("button")).find((button) => button.textContent === i18n.t("deployment.python.action.restart"));
    await act(async () => { restart?.click(); });
    expect(invoke).not.toHaveBeenCalledWith("deployment_java_control", expect.anything());
    expect(document.body.textContent).toContain(i18n.t("deployment.java.controlConfirm", { name: "orders", action: i18n.t("deployment.python.action.restart") }));

    const confirm = Array.from(document.querySelectorAll("button")).find((button) => button.textContent === i18n.t("deployment.confirm"));
    await act(async () => { confirm?.click(); });
    expect(invoke).toHaveBeenCalledWith("deployment_java_control", { request: { sessionId: "session-1", appId: "app-1", action: "restart" } });
  });

  it("does not offer service controls for unfinished drafts", async () => {
    await act(async () => {
      root.render(<JavaProjectPanel sessionId="session-1" profileId="profile-1" apps={[javaApp({ setupStatus: "draft", setupStep: 2, java: null, restart: { kind: "none" } })]} onChanged={async () => undefined} />);
    });
    expect(container.textContent).toContain(i18n.t("deployment.python.continue"));
    expect(container.textContent).not.toContain(i18n.t("deployment.python.logs"));
  });
});

describe("JavaJdkManager", () => {
  const openManager = async () => {
    await act(async () => {
      root.render(<JavaProjectPanel sessionId="session-1" profileId="profile-1" apps={[]} onChanged={async () => undefined} />);
    });
    const manage = Array.from(container.querySelectorAll("button")).find((button) => button.textContent?.includes(i18n.t("deployment.java.jdk.manage")));
    await act(async () => { manage?.click(); });
  };
  const clickText = async (text: string) => {
    const button = Array.from(document.querySelectorAll("button")).find((item) => item.textContent === text);
    await act(async () => { button?.click(); });
  };

  it("lists detected JDKs and marks the system default", async () => {
    await openManager();
    expect(invoke).toHaveBeenCalledWith("deployment_list_java_runtimes", { request: { sessionId: "session-1" } });
    expect(document.body.textContent).toContain("17.0.9");
    expect(document.body.textContent).toContain(i18n.t("deployment.java.jdk.default"));
    expect(document.body.textContent).toContain("/usr/lib/jvm/java-21/bin/java");
  });

  it("requires confirmation before installing a JDK", async () => {
    await openManager();
    await clickText(i18n.t("deployment.java.jdk.install"));
    expect(invoke).not.toHaveBeenCalledWith("deployment_java_install_jdk", expect.anything());
    expect(document.body.textContent).toContain(i18n.t("deployment.java.jdk.installConfirm", { version: "17" }));
    await clickText(i18n.t("deployment.confirm"));
    expect(invoke).toHaveBeenCalledWith("deployment_java_install_jdk", { request: { sessionId: "session-1", version: "17" } });
  });

  it("switches the default only for a listed JDK after confirmation", async () => {
    await openManager();
    await clickText(i18n.t("deployment.java.jdk.setDefault"));
    expect(invoke).not.toHaveBeenCalledWith("deployment_java_set_default_jdk", expect.anything());
    await clickText(i18n.t("deployment.confirm"));
    expect(invoke).toHaveBeenCalledWith("deployment_java_set_default_jdk", { request: { sessionId: "session-1", binary: "/usr/lib/jvm/java-21/bin/java" } });
  });
});
