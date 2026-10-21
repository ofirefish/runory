export type DiskUsage = { mount: string; usedBytes: number; totalBytes: number; usagePercent: number };
export type ServerDashboard = { cpuUsagePercent: number; memoryUsedBytes: number; memoryTotalBytes: number; uptimeSeconds: number; networkReceivedBytes: number; networkTransmittedBytes: number; disks: DiskUsage[] };
export type ProcessInfo = { pid: number; user: string; cpuPercent: number; memoryPercent: number; command: string };
export type ServiceHealth = { name: string; status: "active" | "inactive" | "failed" | "unknown" };
export type ResourceAction = "start" | "stop" | "restart";
export type DockerContainer = {
  id: string;
  name: string;
  image: string;
  state: string;
  status: string;
  ports: string;
  cpuPercent: number;
  memoryBytes: number;
};
export type DockerImage = {
  id: string;
  name: string;
  sizeBytes: number;
  createdAtEpochSeconds: number;
  usedBy: string[];
};
export type DockerOnlineImage = {
  name: string;
  description: string;
  starCount: number;
  isOfficial: boolean;
  isAutomated: boolean;
  tags: string[];
};
export type DockerImageAction =
  | { type: "pull"; reference: string }
  | { type: "remove"; ids: string[] }
  | { type: "prune" }
  | { type: "createContainer"; image: string; name: string; publishPorts?: string[] };
export type DockerNetwork = {
  id: string;
  name: string;
  driver: string;
  ipv4Subnet: string;
  ipv4Gateway: string;
  labels: string;
  createdAtEpochSeconds: number;
};
export type DockerNetworkAction =
  | { type: "create"; name: string; driver: string; subnet?: string; gateway?: string; labels?: string[] }
  | { type: "remove"; ids: string[] }
  | { type: "prune" };
export type DockerVolume = {
  name: string;
  driver: string;
  mountpoint: string;
  scope: string;
  labels: string;
  createdAtEpochSeconds: number;
  usedBy: string[];
};
export type DockerVolumeAction =
  | { type: "create"; name: string; driver?: string; labels?: string[] }
  | { type: "remove"; names: string[] }
  | { type: "prune" };
export type DockerLogOpts = {
  maxSize?: string | null;
  maxFile?: string | null;
};
export type DockerDaemonConfig = {
  registryMirrors: string[];
  insecureRegistries: string[];
  logDriver: string;
  logOpts: DockerLogOpts;
  liveRestore: boolean;
};
export type DockerEngineInfo = {
  serverVersion: string;
  storageDriver: string;
  loggingDriver: string;
  operatingSystem: string;
  architecture: string;
  ncpu: number;
  memTotalBytes: number;
  dockerRootDir: string;
  liveRestoreEnabled: boolean;
};
export type DockerEngineSettingsView = {
  info: DockerEngineInfo;
  config: DockerDaemonConfig;
  configPath: string;
  configExists: boolean;
  configRawPreserved: boolean;
};
export type DockerRegistry = {
  id: string;
  profileId: string;
  url: string;
  name: string;
  username: string;
  namespace: string;
  remarks: string;
  updatedAtEpochSeconds: number;
};
export type DockerRegistryUpsertInput = {
  id?: string;
  url: string;
  name: string;
  username: string;
  password?: string;
  namespace: string;
  remarks?: string;
};
export type Pm2Process = { id: number; name: string; status: string; cpuPercent: number; memoryBytes: number };
export type LogSource = "system" | "auth" | "nginx-access" | "nginx-error" | "docker" | "pm2" | "service";
export type OperationResult = { success: boolean; output: string };
export type BuildPreset = "none" | "npm" | "pnpm" | "cargo" | "pip";
export type ProjectKind = "php" | "java" | "node" | "go" | "python" | "net" | "reverseProxy" | "html" | "other";
export type PythonStartupMethod = "command" | "uwsgi" | "gunicorn";
export type PythonProtocol = "wsgi" | "asgi";
export type PythonProjectConfig = {
  port: number;
  pythonBin: string;
  startup: PythonStartupMethod;
  entryFile: string;
  protocol: PythonProtocol;
  application: string;
  runUser: string;
  requirementsPath: string;
  startupCommand?: string;
  processes?: number;
  threads?: number;
  autoStart?: boolean;
};
export type PhpRewriteMode = "none" | "frontController";
export type PhpProcessManager = "dynamic" | "static" | "ondemand";
export type PhpProjectConfig = {
  phpBin: string;
  port: number;
  serverName: string;
  runDir: string;
  rewrite: PhpRewriteMode;
  runUser: string;
  processManager: PhpProcessManager;
  maxChildren: number;
  memoryLimitMb: number;
  maxExecutionSeconds: number;
  uploadMaxMb: number;
  openBasedir: boolean;
  composerInstall: boolean;
  autoStart: boolean;
};
export type PhpRuntime = { binary: string; version: string };
export type JavaBuildTool = "none" | "maven" | "gradle";
export type JavaProjectConfig = {
  javaBin: string;
  jarPath: string;
  port: number;
  xmsMb: number;
  xmxMb: number;
  jvmArgs: string;
  appArgs: string;
  runUser: string;
  buildTool: JavaBuildTool;
  autoStart: boolean;
};
export type JavaRuntime = { binary: string; version: string; isDefault: boolean };
export type ProxyHostHeader = "original" | "target";
export type ReverseProxyConfig = {
  port: number;
  serverName: string;
  targetUrl: string;
  hostHeader: ProxyHostHeader;
  websocket: boolean;
  maxBodyMb: number;
  timeoutSeconds: number;
};
export type HtmlProjectConfig = {
  port: number;
  serverName: string;
  rootDir: string;
  indexFiles: string;
  spaFallback: boolean;
  gzip: boolean;
  cacheStatic: boolean;
};
export type GoProjectConfig = {
  executable: string;
  buildEnabled: boolean;
  goBin: string;
  buildPackage: string;
  port: number;
  appArgs: string;
  envVars: string;
  runUser: string;
  autoStart: boolean;
};
export type GoRuntime = { binary: string; version: string; isDefault: boolean };
export type NodePackageManager = "npm" | "yarn" | "pnpm";
export type NodeStartMode = "script" | "entry";
export type NodeProjectConfig = {
  nodeBin: string;
  packageManager: NodePackageManager;
  startMode: NodeStartMode;
  script: string;
  entryFile: string;
  appArgs: string;
  port: number;
  nodeEnv: string;
  installDeps: boolean;
  buildScript: string;
  runUser: string;
  autoStart: boolean;
};
export type NodeRuntime = { binary: string; version: string; isDefault: boolean };
export type JdkVersion = "8" | "11" | "17" | "21" | "25";
export type RestartTarget = { kind: "none" } | { kind: "systemd"; service: string } | { kind: "pm2"; process: string } | { kind: "dockerCompose"; service: string };
export type CronSchedule = "hourly" | "daily" | "weekly";
export type CronTask = { kind: "backup"; sourcePath: string; destinationDirectory: string } | { kind: "serviceRestart"; service: string } | { kind: "gitPull"; repositoryPath: string; branch: string };
export type CronEntry = { id: string; schedule: CronSchedule; taskKind: string };
export type DeploymentRecord = { id: string; profileId: string; operation: string; target: string; startedAtEpochSeconds: number; success: boolean; errorCode: string | null };
export type DeploymentSetupStatus = "draft" | "ready" | "provisioning" | "provisioned" | "failed";
export type DeploymentSourceKind = "existingPath" | "gitClone";
export type PythonRuntime = { binary: string; version: string };
export type DeploymentApp = {
  id: string;
  profileId: string;
  name: string;
  repositoryPath: string;
  remoteUrl: string;
  branch: string;
  build: BuildPreset;
  restart: RestartTarget;
  projectKind?: ProjectKind;
  python?: PythonProjectConfig | null;
  php?: PhpProjectConfig | null;
  java?: JavaProjectConfig | null;
  node?: NodeProjectConfig | null;
  go?: GoProjectConfig | null;
  html?: HtmlProjectConfig | null;
  proxy?: ReverseProxyConfig | null;
  setupStep?: number;
  setupStatus?: DeploymentSetupStatus;
  sourceKind?: DeploymentSourceKind;
  venvPath?: string;
  updatedAtEpochSeconds: number;
};
