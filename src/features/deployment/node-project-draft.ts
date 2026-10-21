import type {
  DeploymentApp,
  DeploymentSetupStatus,
  DeploymentSourceKind,
  NodePackageManager,
  NodeProjectConfig,
  NodeStartMode,
} from "../../types/infrastructure";

export type NodeProjectDraft = {
  id?: string;
  name: string;
  projectPath: string;
  setupStep: 1 | 2 | 3 | 4;
  setupStatus: DeploymentSetupStatus;
  sourceKind: DeploymentSourceKind;
  remoteUrl: string;
  branch: string;
  nodeBin: string;
  packageManager: NodePackageManager;
  installDeps: boolean;
  buildScript: string;
  startMode: NodeStartMode;
  script: string;
  entryFile: string;
  appArgs: string;
  port: string;
  nodeEnv: string;
  runUser: string;
  autoStart: boolean;
};

export function emptyNodeDraft(): NodeProjectDraft {
  return {
    name: "",
    projectPath: "",
    setupStep: 1,
    setupStatus: "draft",
    sourceKind: "existingPath",
    remoteUrl: "",
    branch: "master",
    nodeBin: "node",
    packageManager: "npm",
    installDeps: true,
    buildScript: "",
    startMode: "script",
    script: "start",
    entryFile: "",
    appArgs: "",
    port: "3000",
    nodeEnv: "production",
    runUser: "www",
    autoStart: true,
  };
}

export function draftFromNodeApp(app: DeploymentApp): NodeProjectDraft {
  const node = app.node;
  const base = emptyNodeDraft();
  const finished = app.setupStatus === "ready" || app.setupStatus === "provisioned" || app.setupStatus === "failed";
  const step = finished ? 4 : (Math.min(Math.max(app.setupStep ?? 1, 1), 4) as 1 | 2 | 3 | 4);
  return {
    ...base,
    id: app.id,
    name: app.name,
    projectPath: app.repositoryPath,
    setupStep: step,
    setupStatus: app.setupStatus ?? (node ? "ready" : "draft"),
    sourceKind: app.sourceKind ?? (app.remoteUrl ? "gitClone" : "existingPath"),
    remoteUrl: app.remoteUrl ?? "",
    branch: app.branch || "master",
    nodeBin: node?.nodeBin ?? base.nodeBin,
    packageManager: node?.packageManager ?? base.packageManager,
    installDeps: node?.installDeps ?? base.installDeps,
    buildScript: node?.buildScript ?? base.buildScript,
    startMode: node?.startMode ?? base.startMode,
    script: node?.script ?? base.script,
    entryFile: node?.entryFile ?? base.entryFile,
    appArgs: node?.appArgs ?? base.appArgs,
    port: String(node?.port ?? base.port),
    nodeEnv: node?.nodeEnv ?? base.nodeEnv,
    runUser: node?.runUser ?? base.runUser,
    autoStart: node?.autoStart ?? base.autoStart,
  };
}

const collapse = (value: string) => value.trim().split(/\s+/).filter(Boolean).join(" ");

export function normalizeNodeDraft(draft: NodeProjectDraft): NodeProjectDraft {
  return {
    ...draft,
    name: draft.name.trim(),
    projectPath: draft.projectPath.trim(),
    remoteUrl: draft.remoteUrl.trim(),
    branch: draft.branch.trim() || "master",
    nodeBin: draft.nodeBin.trim(),
    buildScript: draft.buildScript.trim(),
    script: draft.script.trim(),
    entryFile: draft.entryFile.trim(),
    appArgs: collapse(draft.appArgs),
    port: draft.port.trim(),
    nodeEnv: draft.nodeEnv.trim(),
    runUser: draft.runUser.trim(),
  };
}

// Paths and options end up in a systemd unit, so the character set mirrors the Rust validator.
const safePath = /^[\w./+@-]+$/;
const optionToken = /^[\w./+@=:,-]+$/;
const scriptName = /^[A-Za-z0-9][\w.:-]{0,63}$/;
const envName = /^[a-z][a-z0-9_-]{0,31}$/;

function validNodeBin(value: string): boolean {
  return value === "node" || (value.startsWith("/") && value.endsWith("/bin/node") && value.length <= 256 && safePath.test(value) && !value.split("/").includes(".."));
}

function validEntry(value: string): boolean {
  return /\.(js|mjs|cjs)$/.test(value) && value.length <= 512 && !value.startsWith("/") && safePath.test(value) && !value.split("/").includes("..");
}

export function validateNodeStep(draft: NodeProjectDraft, step: 1 | 2 | 3 | 4): string | null {
  const next = normalizeNodeDraft(draft);
  if (step === 1) {
    if (!next.name || next.name.length > 128) return "deployment.validation.name";
    if (!next.projectPath.startsWith("/") || next.projectPath.split("/").includes("..")) {
      return "deployment.validation.repositoryPath";
    }
    if (!safePath.test(next.projectPath)) return "deployment.node.validation.pathChars";
    return null;
  }
  if (step === 2) {
    if (next.sourceKind !== "gitClone") return null;
    if (!next.remoteUrl) return "deployment.validation.remoteUrl";
    if (!next.branch) return "deployment.validation.branch";
    return null;
  }
  if (step === 3) {
    if (!validNodeBin(next.nodeBin)) return "deployment.node.validation.nodeBin";
    if (next.buildScript && !scriptName.test(next.buildScript)) return "deployment.node.validation.buildScript";
    if (next.startMode === "script" && !scriptName.test(next.script)) return "deployment.node.validation.script";
    if (next.startMode === "entry" && !validEntry(next.entryFile)) return "deployment.node.validation.entryFile";
    return null;
  }
  return validateNodeDraft({ ...next, setupStep: 4 });
}

export function validateNodeDraft(draft: NodeProjectDraft): string | null {
  const next = normalizeNodeDraft(draft);
  for (const step of [1, 2, 3] as const) {
    const error = validateNodeStep(next, step);
    if (error) return error;
  }
  if (!/^\d+$/.test(next.port) || Number(next.port) < 1 || Number(next.port) > 65535) return "deployment.node.validation.port";
  if (!envName.test(next.nodeEnv)) return "deployment.node.validation.nodeEnv";
  if (!next.runUser || !/^[\w.@:-]+$/.test(next.runUser)) return "deployment.node.validation.runUser";
  const args = next.appArgs ? next.appArgs.split(" ") : [];
  if (args.length > 32 || next.appArgs.length > 1024 || args.some((token) => token.length > 256 || !optionToken.test(token) || token.includes(".."))) {
    return "deployment.node.validation.appArgs";
  }
  return null;
}

export function nodeConfigFromDraft(draft: NodeProjectDraft): NodeProjectConfig {
  const next = normalizeNodeDraft(draft);
  return {
    nodeBin: next.nodeBin || "node",
    packageManager: next.packageManager,
    startMode: next.startMode,
    script: next.startMode === "script" ? next.script : "",
    entryFile: next.startMode === "entry" ? next.entryFile : "",
    appArgs: next.appArgs,
    port: Number(next.port) || 0,
    nodeEnv: next.nodeEnv,
    installDeps: next.installDeps,
    buildScript: next.buildScript,
    runUser: next.runUser,
    autoStart: next.autoStart,
  };
}
