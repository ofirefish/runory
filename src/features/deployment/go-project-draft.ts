import type {
  DeploymentApp,
  DeploymentSetupStatus,
  DeploymentSourceKind,
  GoProjectConfig,
} from "../../types/infrastructure";

export type GoProjectDraft = {
  id?: string;
  name: string;
  projectPath: string;
  setupStep: 1 | 2 | 3 | 4;
  setupStatus: DeploymentSetupStatus;
  sourceKind: DeploymentSourceKind;
  remoteUrl: string;
  branch: string;
  buildEnabled: boolean;
  goBin: string;
  buildPackage: string;
  executable: string;
  port: string;
  appArgs: string;
  envVars: string;
  runUser: string;
  autoStart: boolean;
};

export function emptyGoDraft(): GoProjectDraft {
  return {
    name: "",
    projectPath: "",
    setupStep: 1,
    setupStatus: "draft",
    sourceKind: "existingPath",
    remoteUrl: "",
    branch: "master",
    buildEnabled: true,
    goBin: "go",
    buildPackage: ".",
    executable: "",
    port: "8080",
    appArgs: "",
    envVars: "",
    runUser: "www",
    autoStart: true,
  };
}

export function draftFromGoApp(app: DeploymentApp): GoProjectDraft {
  const go = app.go;
  const base = emptyGoDraft();
  const finished = app.setupStatus === "ready" || app.setupStatus === "provisioned" || app.setupStatus === "failed";
  const step = finished ? 4 : (Math.min(Math.max(app.setupStep ?? 1, 1), 4) as 1 | 2 | 3 | 4);
  return {
    ...base,
    id: app.id,
    name: app.name,
    projectPath: app.repositoryPath,
    setupStep: step,
    setupStatus: app.setupStatus ?? (go ? "ready" : "draft"),
    sourceKind: app.sourceKind ?? (app.remoteUrl ? "gitClone" : "existingPath"),
    remoteUrl: app.remoteUrl ?? "",
    branch: app.branch || "master",
    buildEnabled: go?.buildEnabled ?? base.buildEnabled,
    goBin: go?.goBin ?? base.goBin,
    buildPackage: go?.buildPackage ?? base.buildPackage,
    executable: go?.executable ?? base.executable,
    port: String(go?.port ?? base.port),
    appArgs: go?.appArgs ?? base.appArgs,
    envVars: go?.envVars ?? base.envVars,
    runUser: go?.runUser ?? base.runUser,
    autoStart: go?.autoStart ?? base.autoStart,
  };
}

const collapse = (value: string) => value.trim().split(/\s+/).filter(Boolean).join(" ");

export function normalizeGoDraft(draft: GoProjectDraft): GoProjectDraft {
  return {
    ...draft,
    name: draft.name.trim(),
    projectPath: draft.projectPath.trim(),
    remoteUrl: draft.remoteUrl.trim(),
    branch: draft.branch.trim() || "master",
    goBin: draft.goBin.trim() || "go",
    buildPackage: draft.buildPackage.trim() || ".",
    executable: draft.executable.trim(),
    port: draft.port.trim(),
    appArgs: collapse(draft.appArgs),
    envVars: collapse(draft.envVars),
    runUser: draft.runUser.trim(),
  };
}

// Paths and options end up in a systemd unit, so the character set mirrors the Rust validator.
const safePath = /^[\w./+@-]+$/;
const optionToken = /^[\w./+@=:,-]+$/;
const envKey = /^[A-Za-z_][A-Za-z0-9_]{0,63}$/;

function validGoBin(value: string): boolean {
  return value === "go" || (value.startsWith("/") && value.endsWith("/bin/go") && value.length <= 256 && safePath.test(value) && !value.split("/").includes(".."));
}

function validExecutable(value: string): boolean {
  if (!value || value.length > 512 || value.endsWith("/") || !safePath.test(value) || value.split("/").includes("..")) return false;
  return !value.startsWith("-");
}

function validPackage(value: string): boolean {
  if (value === ".") return true;
  return value.length <= 256 && !value.startsWith("-") && !value.startsWith("/") && safePath.test(value) && !value.split("/").includes("..");
}

function validEnvVars(value: string): boolean {
  const pairs = value ? value.split(" ") : [];
  if (pairs.length > 32 || value.length > 2048) return false;
  return pairs.every((pair) => {
    const index = pair.indexOf("=");
    if (index < 1) return false;
    const key = pair.slice(0, index);
    const val = pair.slice(index + 1);
    return envKey.test(key) && key !== "PORT" && key !== "PATH" && !key.startsWith("LD_") && val.length <= 256 && (val === "" || optionToken.test(val));
  });
}

export function validateGoStep(draft: GoProjectDraft, step: 1 | 2 | 3 | 4): string | null {
  const next = normalizeGoDraft(draft);
  if (step === 1) {
    if (!next.name || next.name.length > 128) return "deployment.validation.name";
    if (!next.projectPath.startsWith("/") || next.projectPath.split("/").includes("..")) {
      return "deployment.validation.repositoryPath";
    }
    if (!safePath.test(next.projectPath)) return "deployment.go.validation.pathChars";
    return null;
  }
  if (step === 2) {
    if (next.sourceKind !== "gitClone") return null;
    if (!next.remoteUrl) return "deployment.validation.remoteUrl";
    if (!next.branch) return "deployment.validation.branch";
    return null;
  }
  if (step === 3) {
    if (!validExecutable(next.executable)) return "deployment.go.validation.executable";
    if (!validGoBin(next.goBin)) return "deployment.go.validation.goBin";
    if (!validPackage(next.buildPackage)) return "deployment.go.validation.buildPackage";
    return null;
  }
  return validateGoDraft({ ...next, setupStep: 4 });
}

export function validateGoDraft(draft: GoProjectDraft): string | null {
  const next = normalizeGoDraft(draft);
  for (const step of [1, 2, 3] as const) {
    const error = validateGoStep(next, step);
    if (error) return error;
  }
  if (!/^\d+$/.test(next.port) || Number(next.port) < 1 || Number(next.port) > 65535) return "deployment.go.validation.port";
  if (!next.runUser || !/^[\w.@:-]+$/.test(next.runUser)) return "deployment.go.validation.runUser";
  const args = next.appArgs ? next.appArgs.split(" ") : [];
  if (args.length > 32 || next.appArgs.length > 1024 || args.some((token) => token.length > 256 || !optionToken.test(token) || token.includes(".."))) {
    return "deployment.go.validation.appArgs";
  }
  if (!validEnvVars(next.envVars)) return "deployment.go.validation.envVars";
  return null;
}

export function goConfigFromDraft(draft: GoProjectDraft): GoProjectConfig {
  const next = normalizeGoDraft(draft);
  return {
    executable: next.executable,
    buildEnabled: next.buildEnabled,
    goBin: next.goBin,
    buildPackage: next.buildPackage,
    port: Number(next.port) || 0,
    appArgs: next.appArgs,
    envVars: next.envVars,
    runUser: next.runUser,
    autoStart: next.autoStart,
  };
}
