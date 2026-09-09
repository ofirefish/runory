import { invoke } from "@tauri-apps/api/core";
import type { BuildPreset, CronEntry, CronSchedule, CronTask, DeploymentApp, DeploymentRecord, DockerContainer, DockerDaemonConfig, DockerEngineSettingsView, DockerImage, DockerImageAction, DockerNetwork, DockerNetworkAction, DockerOnlineImage, DockerRegistry, DockerRegistryUpsertInput, DockerVolume, DockerVolumeAction, LogSource, OperationResult, Pm2Process, ProcessInfo, ResourceAction, RestartTarget, ServerDashboard, ServiceHealth } from "../../types/infrastructure";

const request = (sessionId: string) => ({ request: { sessionId } });
export const loadDashboard = (sessionId: string) => invoke<ServerDashboard>("dashboard_overview", request(sessionId));
export const loadProcesses = (sessionId: string) => invoke<ProcessInfo[]>("dashboard_processes", request(sessionId));
export const loadServiceHealth = (sessionId: string, services: string[]) => invoke<ServiceHealth[]>("dashboard_service_health", { request: { sessionId, services } });
export const listDocker = (sessionId: string) => invoke<DockerContainer[]>("docker_list", request(sessionId));
export const actOnDocker = (sessionId: string, container: string, action: ResourceAction) => invoke<OperationResult>("docker_action", { request: { sessionId, container, action } });
export const listDockerImages = (sessionId: string) => invoke<DockerImage[]>("docker_images_list", request(sessionId));
export const searchDockerImages = (sessionId: string, query: string, limit = 100) =>
  invoke<DockerOnlineImage[]>("docker_images_search", { request: { sessionId, query, limit } });
export const actOnDockerImage = (sessionId: string, action: DockerImageAction) => invoke<OperationResult>("docker_image_action", { request: { sessionId, action } });
export const listDockerNetworks = (sessionId: string) => invoke<DockerNetwork[]>("docker_networks_list", request(sessionId));
export const actOnDockerNetwork = (sessionId: string, action: DockerNetworkAction) =>
  invoke<OperationResult>("docker_network_action", { request: { sessionId, action } });
export const listDockerVolumes = (sessionId: string) => invoke<DockerVolume[]>("docker_volumes_list", request(sessionId));
export const actOnDockerVolume = (sessionId: string, action: DockerVolumeAction) =>
  invoke<OperationResult>("docker_volume_action", { request: { sessionId, action } });
export const getDockerSettings = (sessionId: string) =>
  invoke<DockerEngineSettingsView>("docker_settings_get", request(sessionId));
export const applyDockerSettings = (sessionId: string, config: DockerDaemonConfig, restart = true) =>
  invoke<OperationResult>("docker_settings_apply", { request: { sessionId, config, restart } });
export const listDockerRegistries = (sessionId: string) =>
  invoke<DockerRegistry[]>("docker_registries_list", request(sessionId));
export const upsertDockerRegistry = (sessionId: string, input: DockerRegistryUpsertInput) =>
  invoke<DockerRegistry>("docker_registries_upsert", { request: { sessionId, ...input } });
export const deleteDockerRegistries = (sessionId: string, ids: string[]) =>
  invoke<OperationResult>("docker_registries_delete", { request: { sessionId, ids } });
export const listPm2 = (sessionId: string) => invoke<Pm2Process[]>("pm2_list", request(sessionId));
export const actOnPm2 = (sessionId: string, process: string, action: ResourceAction) => invoke<OperationResult>("pm2_action", { request: { sessionId, process, action } });
export const actOnNginx = (sessionId: string, action: "test" | "reload") => invoke<OperationResult>("nginx_action", { request: { sessionId, action } });
export const readLogs = (sessionId: string, source: LogSource, target: string | null, lines: number) => invoke<OperationResult>("logs_read", { request: { sessionId, source, target, lines } });
export const setupGit = (sessionId: string, repositoryPath: string, remoteUrl: string, branch: string) => invoke<OperationResult>("deployment_git_setup", { request: { sessionId, repositoryPath, remoteUrl, branch } });
export const runDeployment = (sessionId: string, repositoryPath: string, branch: string, build: BuildPreset, restart: RestartTarget) => invoke<OperationResult>("deployment_run", { request: { sessionId, repositoryPath, branch, build, restart } });
export const writeEnvironment = (sessionId: string, path: string, entries: { key: string; value: string }[]) => invoke<OperationResult>("deployment_environment_write", { request: { sessionId, path, entries } });
export const inspectSsl = (sessionId: string, domain: string) => invoke<OperationResult>("deployment_ssl_inspect", { request: { sessionId, domain } });
export const issueSsl = (sessionId: string, domain: string, email: string, webroot: string) => invoke<OperationResult>("deployment_ssl_issue", { request: { sessionId, domain, email, webroot } });
export const createBackup = (sessionId: string, sourcePath: string, destinationDirectory: string) => invoke<OperationResult>("deployment_backup", { request: { sessionId, sourcePath, destinationDirectory } });
export const listCron = (sessionId: string) => invoke<CronEntry[]>("deployment_cron_list", request(sessionId));
export const addCron = (sessionId: string, schedule: CronSchedule, task: CronTask) => invoke<CronEntry>("deployment_cron_add", { request: { sessionId, schedule, task } });
export const removeCron = (sessionId: string, cronId: string) => invoke<OperationResult>("deployment_cron_remove", { request: { sessionId, cronId } });
export const listDeploymentHistory = (profileId: string | null) => invoke<DeploymentRecord[]>("deployment_history", { request: { profileId } });
export const listDeploymentApps = (profileId: string) => invoke<DeploymentApp[]>("deployment_apps_list", { request: { profileId } });
export const upsertDeploymentApp = (app: {
  id?: string | null;
  profileId: string;
  name: string;
  repositoryPath: string;
  remoteUrl: string;
  branch: string;
  build: BuildPreset;
  restart: RestartTarget;
}) => invoke<DeploymentApp>("deployment_apps_upsert", { request: app });
export const deleteDeploymentApp = (profileId: string, id: string) => invoke<void>("deployment_apps_delete", { request: { profileId, id } });
