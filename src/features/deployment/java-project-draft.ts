import type {
  DeploymentApp,
  DeploymentSetupStatus,
  DeploymentSourceKind,
  JavaBuildTool,
  JavaProjectConfig,
} from "../../types/infrastructure";

export type JavaProjectDraft = {
  id?: string;
  name: string;
  projectPath: string;
  setupStep: 1 | 2 | 3 | 4;
  setupStatus: DeploymentSetupStatus;
  sourceKind: DeploymentSourceKind;
  remoteUrl: string;
  branch: string;
  javaBin: string;
  jarPath: string;
  buildTool: JavaBuildTool;
  port: string;
  xmsMb: string;
  xmxMb: string;
  jvmArgs: string;
  appArgs: string;
  runUser: string;
  autoStart: boolean;
};

export function emptyJavaDraft(): JavaProjectDraft {
  return {
    name: "",
    projectPath: "",
    setupStep: 1,
    setupStatus: "draft",
    sourceKind: "existingPath",
    remoteUrl: "",
    branch: "master",
    javaBin: "java",
    jarPath: "",
    buildTool: "none",
    port: "8080",
    xmsMb: "256",
    xmxMb: "512",
    jvmArgs: "",
    appArgs: "",
    runUser: "www",
    autoStart: true,
  };
}

export function draftFromJavaApp(app: DeploymentApp): JavaProjectDraft {
  const java = app.java;
  const base = emptyJavaDraft();
  const finished = app.setupStatus === "ready" || app.setupStatus === "provisioned" || app.setupStatus === "failed";
  const step = finished ? 4 : (Math.min(Math.max(app.setupStep ?? 1, 1), 4) as 1 | 2 | 3 | 4);
  return {
    ...base,
    id: app.id,
    name: app.name,
    projectPath: app.repositoryPath,
    setupStep: step,
    setupStatus: app.setupStatus ?? (java ? "ready" : "draft"),
    sourceKind: app.sourceKind ?? (app.remoteUrl ? "gitClone" : "existingPath"),
    remoteUrl: app.remoteUrl ?? "",
    branch: app.branch || "master",
    javaBin: java?.javaBin ?? base.javaBin,
    jarPath: java?.jarPath ?? base.jarPath,
    buildTool: java?.buildTool ?? base.buildTool,
    port: String(java?.port ?? base.port),
    xmsMb: String(java?.xmsMb ?? base.xmsMb),
    xmxMb: String(java?.xmxMb ?? base.xmxMb),
    jvmArgs: java?.jvmArgs ?? base.jvmArgs,
    appArgs: java?.appArgs ?? base.appArgs,
    runUser: java?.runUser ?? base.runUser,
    autoStart: java?.autoStart ?? base.autoStart,
  };
}

const collapse = (value: string) => value.trim().split(/\s+/).filter(Boolean).join(" ");

export function normalizeJavaDraft(draft: JavaProjectDraft): JavaProjectDraft {
  return {
    ...draft,
    name: draft.name.trim(),
    projectPath: draft.projectPath.trim(),
    remoteUrl: draft.remoteUrl.trim(),
    branch: draft.branch.trim() || "master",
    javaBin: draft.javaBin.trim(),
    jarPath: draft.jarPath.trim(),
    port: draft.port.trim(),
    xmsMb: draft.xmsMb.trim(),
    xmxMb: draft.xmxMb.trim(),
    jvmArgs: collapse(draft.jvmArgs),
    appArgs: collapse(draft.appArgs),
    runUser: draft.runUser.trim(),
  };
}

// Paths and options end up in a systemd unit, so the character set mirrors the Rust validator.
const safePath = /^[\w./+@-]+$/;
const optionToken = /^[\w./+@=:,-]+$/;
const reservedJvmOption = /^-(xmx|xms|agent|xrun|xdebug|dserver\.port)/i;

function inRange(value: string, min: number, max: number): boolean {
  return /^\d+$/.test(value) && Number(value) >= min && Number(value) <= max;
}

function validJavaBin(value: string): boolean {
  return value === "java" || (value.startsWith("/") && value.endsWith("/bin/java") && value.length <= 256 && safePath.test(value) && !value.split("/").includes(".."));
}

function validJarPath(value: string): boolean {
  if (!value.endsWith(".jar") || value.length > 512 || !safePath.test(value)) return false;
  return !value.split("/").includes("..");
}

function tokens(value: string): string[] {
  return value ? value.split(" ") : [];
}

export function validateJavaStep(draft: JavaProjectDraft, step: 1 | 2 | 3 | 4): string | null {
  const next = normalizeJavaDraft(draft);
  if (step === 1) {
    if (!next.name || next.name.length > 128) return "deployment.validation.name";
    if (!next.projectPath.startsWith("/") || next.projectPath.split("/").includes("..")) {
      return "deployment.validation.repositoryPath";
    }
    if (!safePath.test(next.projectPath)) return "deployment.java.validation.pathChars";
    return null;
  }
  if (step === 2) {
    if (next.sourceKind !== "gitClone") return null;
    if (!next.remoteUrl) return "deployment.validation.remoteUrl";
    if (!next.branch) return "deployment.validation.branch";
    return null;
  }
  if (step === 3) {
    if (!validJavaBin(next.javaBin)) return "deployment.java.validation.javaBin";
    if (!validJarPath(next.jarPath)) return "deployment.java.validation.jarPath";
    return null;
  }
  return validateJavaDraft({ ...next, setupStep: 4 });
}

export function validateJavaDraft(draft: JavaProjectDraft): string | null {
  const next = normalizeJavaDraft(draft);
  for (const step of [1, 2, 3] as const) {
    const error = validateJavaStep(next, step);
    if (error) return error;
  }
  if (!inRange(next.port, 1, 65535)) return "deployment.java.validation.port";
  if (!next.runUser || !/^[\w.@:-]+$/.test(next.runUser)) return "deployment.java.validation.runUser";
  if (!inRange(next.xmsMb, 16, 262144) || !inRange(next.xmxMb, 16, 262144) || Number(next.xmsMb) > Number(next.xmxMb)) {
    return "deployment.java.validation.heap";
  }
  const jvm = tokens(next.jvmArgs);
  if (jvm.length > 32 || next.jvmArgs.length > 1024 || jvm.some((token) => token.length > 256 || !token.startsWith("-") || !optionToken.test(token) || token.includes("..") || reservedJvmOption.test(token))) {
    return "deployment.java.validation.jvmArgs";
  }
  const app = tokens(next.appArgs);
  if (app.length > 32 || next.appArgs.length > 1024 || app.some((token) => token.length > 256 || !optionToken.test(token) || token.includes(".."))) {
    return "deployment.java.validation.appArgs";
  }
  return null;
}

export function javaConfigFromDraft(draft: JavaProjectDraft): JavaProjectConfig {
  const next = normalizeJavaDraft(draft);
  return {
    javaBin: next.javaBin || "java",
    jarPath: next.jarPath,
    port: Number(next.port) || 0,
    xmsMb: Number(next.xmsMb),
    xmxMb: Number(next.xmxMb),
    jvmArgs: next.jvmArgs,
    appArgs: next.appArgs,
    runUser: next.runUser,
    buildTool: next.buildTool,
    autoStart: next.autoStart,
  };
}
