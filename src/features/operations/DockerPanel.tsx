import { Box, CloudDownload, HardDrive, Layers, Network, Package, Settings } from "lucide-react";
import { useState, type KeyboardEvent, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "../../components/ui/button";
import type {
  DockerContainer,
  DockerDaemonConfig,
  DockerEngineSettingsView,
  DockerImage,
  DockerImageAction,
  DockerNetwork,
  DockerNetworkAction,
  DockerRegistry,
  DockerRegistryUpsertInput,
  DockerVolume,
  DockerVolumeAction,
} from "../../types/infrastructure";
import { formatFileSize } from "../files/sftp-format";
import { DockerImagesPanel } from "./DockerImagesPanel";
import { DockerNetworksPanel } from "./DockerNetworksPanel";
import { DockerOnlineImagesPanel } from "./DockerOnlineImagesPanel";
import { DockerRegistriesPanel } from "./DockerRegistriesPanel";
import { DockerSettingsPanel } from "./DockerSettingsPanel";
import { DockerVolumesPanel } from "./DockerVolumesPanel";

export type DockerTab = "containers" | "images" | "onlineImages" | "networks" | "volumes" | "registry" | "settings";

const dockerTabs: { id: DockerTab; icon: typeof Box }[] = [
  { id: "containers", icon: Box },
  { id: "images", icon: Layers },
  { id: "onlineImages", icon: CloudDownload },
  { id: "networks", icon: Network },
  { id: "volumes", icon: HardDrive },
  { id: "registry", icon: Package },
  { id: "settings", icon: Settings },
];

function formatCpuPercent(value: number): string {
  if (!Number.isFinite(value)) return "—";
  return `${value.toFixed(value >= 10 ? 1 : 2)}%`;
}

export function DockerPanel({
  sessionId,
  containers,
  images,
  networks = [],
  volumes = [],
  registries = [],
  imagesBusy = false,
  networksBusy = false,
  volumesBusy = false,
  registriesBusy = false,
  settings,
  settingsBusy = false,
  unsupportedMessage = null,
  onTabChange,
  onImagesTab,
  onNetworksTab,
  onVolumesTab,
  onRegistriesTab,
  onSettingsTab,
  onReloadSettings,
  onApplySettings,
  onImageAction,
  onNetworkAction,
  onVolumeAction,
  onRegistryUpsert,
  onRegistryDelete,
  actionButtons,
}: {
  sessionId: string;
  containers: DockerContainer[];
  images: DockerImage[];
  networks?: DockerNetwork[];
  volumes?: DockerVolume[];
  registries?: DockerRegistry[];
  imagesBusy?: boolean;
  networksBusy?: boolean;
  volumesBusy?: boolean;
  registriesBusy?: boolean;
  settings: DockerEngineSettingsView | null;
  settingsBusy?: boolean;
  unsupportedMessage?: string | null;
  onTabChange?: (tab: DockerTab) => void;
  onImagesTab?: () => void;
  onNetworksTab?: () => void;
  onVolumesTab?: () => void;
  onRegistriesTab?: () => void;
  onSettingsTab?: () => void;
  onReloadSettings: () => Promise<void>;
  onApplySettings: (config: DockerDaemonConfig) => Promise<void>;
  onImageAction: (action: DockerImageAction) => Promise<void>;
  onNetworkAction?: (action: DockerNetworkAction) => Promise<void>;
  onVolumeAction?: (action: DockerVolumeAction) => Promise<void>;
  onRegistryUpsert?: (input: DockerRegistryUpsertInput) => Promise<void>;
  onRegistryDelete?: (ids: string[]) => Promise<void>;
  actionButtons: (kind: "docker", target: string, label: string, state: string) => ReactNode;
}) {
  const { t, i18n } = useTranslation();
  const [tab, setTab] = useState<DockerTab>("containers");

  const selectTab = (next: DockerTab) => {
    setTab(next);
    onTabChange?.(next);
    if (next === "images" || next === "onlineImages") onImagesTab?.();
    if (next === "networks") onNetworksTab?.();
    if (next === "volumes") onVolumesTab?.();
    if (next === "registry") onRegistriesTab?.();
    if (next === "settings") onSettingsTab?.();
  };

  const onKeyDown = (event: KeyboardEvent<HTMLElement>) => {
    if (event.key !== "ArrowUp" && event.key !== "ArrowDown" && event.key !== "Home" && event.key !== "End") return;
    const current = dockerTabs.findIndex((item) => item.id === tab);
    if (current < 0) return;
    event.preventDefault();
    const nextIndex = event.key === "Home" ? 0
      : event.key === "End" ? dockerTabs.length - 1
        : (current + (event.key === "ArrowDown" ? 1 : dockerTabs.length - 1)) % dockerTabs.length;
    const next = dockerTabs[nextIndex].id;
    selectTab(next);
    document.getElementById(`docker-tab-${next}`)?.focus();
  };

  return (
    <div className="flex min-h-0 min-w-0 flex-1">
      <nav
        className="w-44 shrink-0 space-y-1 overflow-auto border-r p-2"
        role="tablist"
        aria-label={t("operations.dockerTabs")}
        aria-orientation="vertical"
        onKeyDown={onKeyDown}
      >
        {dockerTabs.map((item) => (
          <Button
            key={item.id}
            id={`docker-tab-${item.id}`}
            type="button"
            role="tab"
            variant={tab === item.id ? "secondary" : "ghost"}
            className="w-full justify-start"
            aria-selected={tab === item.id}
            aria-controls={`docker-panel-${item.id}`}
            tabIndex={tab === item.id ? 0 : -1}
            onClick={() => selectTab(item.id)}
          >
            <item.icon size={15} />
            {t(`operations.dockerTab.${item.id}`)}
          </Button>
        ))}
      </nav>
      <div
        id={`docker-panel-${tab}`}
        role="tabpanel"
        aria-labelledby={`docker-tab-${tab}`}
        className="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden p-4"
      >
        {unsupportedMessage ? (
          <div className="mb-3 shrink-0 rounded-lg border border-red-500/30 bg-red-500/10 p-3 text-sm text-red-500">
            {unsupportedMessage}
          </div>
        ) : null}
        {tab === "containers" ? (
          <div className="min-h-0 flex-1 overflow-auto rounded-lg border">
            <table className="w-full text-left text-sm">
              <thead className="sticky top-0 bg-[hsl(var(--elevated))] text-xs">
                <tr>
                  <th className="px-3 py-2">{t("operations.name")}</th>
                  <th className="px-3 py-2">{t("operations.image")}</th>
                  <th className="px-3 py-2">{t("operations.status")}</th>
                  <th className="px-3 py-2">{t("operations.ports")}</th>
                  <th className="px-3 py-2">{t("operations.cpu")}</th>
                  <th className="px-3 py-2">{t("operations.memory")}</th>
                  <th className="w-28 px-3 py-2">{t("operations.actions")}</th>
                </tr>
              </thead>
              <tbody>
                {containers.map((item) => (
                  <tr key={item.id} className="border-t">
                    <td className="px-3 py-2">{item.name}</td>
                    <td className="px-3 py-2 font-mono text-xs">{item.image}</td>
                    <td className="px-3 py-2">{item.status}</td>
                    <td className="max-w-56 truncate px-3 py-2 font-mono text-xs" title={item.ports || undefined}>
                      {item.ports || "—"}
                    </td>
                    <td className="px-3 py-2 tabular-nums">{item.state === "running" ? formatCpuPercent(item.cpuPercent) : "—"}</td>
                    <td className="px-3 py-2 tabular-nums">
                      {item.state === "running" ? formatFileSize(item.memoryBytes, i18n.language) : "—"}
                    </td>
                    <td className="px-3 py-2">{actionButtons("docker", item.id, item.name, item.state)}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        ) : tab === "images" ? (
          <DockerImagesPanel images={images} busy={imagesBusy} onAction={onImageAction} />
        ) : tab === "onlineImages" ? (
          <DockerOnlineImagesPanel
            sessionId={sessionId}
            images={images}
            busy={imagesBusy}
            onAction={onImageAction}
            onEnsureLocalImages={onImagesTab}
          />
        ) : tab === "networks" ? (
          <DockerNetworksPanel
            networks={networks}
            busy={networksBusy}
            onAction={onNetworkAction ?? (async () => undefined)}
            onOpenSettings={() => selectTab("settings")}
          />
        ) : tab === "volumes" ? (
          <DockerVolumesPanel
            volumes={volumes}
            busy={volumesBusy}
            onAction={onVolumeAction ?? (async () => undefined)}
            onOpenSettings={() => selectTab("settings")}
          />
        ) : tab === "registry" ? (
          <DockerRegistriesPanel
            registries={registries}
            busy={registriesBusy}
            onUpsert={onRegistryUpsert ?? (async () => undefined)}
            onDelete={onRegistryDelete ?? (async () => undefined)}
            onOpenSettings={() => selectTab("settings")}
          />
        ) : tab === "settings" ? (
          <div className="min-h-0 flex-1 overflow-auto">
            <DockerSettingsPanel
              settings={settings}
              busy={settingsBusy}
              onReload={onReloadSettings}
              onApply={onApplySettings}
            />
          </div>
        ) : (
          <div className="grid h-full min-h-40 place-items-center rounded-lg border border-dashed p-8 text-center">
            <div className="max-w-sm">
              <p className="text-sm font-medium">{t(`operations.dockerTab.${tab}`)}</p>
              <p className="mt-2 text-xs text-[hsl(var(--muted))]">{t("operations.dockerTabUnavailable")}</p>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
