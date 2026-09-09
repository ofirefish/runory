// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import i18n from "../../i18n";
import type { DockerEngineSettingsView } from "../../types/infrastructure";
import { DockerPanel } from "./DockerPanel";
import { matchLocalImages } from "./DockerOnlineImagesPanel";

vi.mock("../../lib/tauri/infrastructure", () => ({
  searchDockerImages: vi.fn(async () => [
    {
      name: "nginx",
      description: "Official build of Nginx.",
      starCount: 19712,
      isOfficial: true,
      isAutomated: false,
      tags: ["latest", "1.25", "1.25-alpine"],
    },
    {
      name: "bitnami/redis",
      description: "Redis by Bitnami",
      starCount: 100,
      isOfficial: false,
      isAutomated: false,
      tags: ["latest", "7.2"],
    },
  ]),
}));

let container: HTMLDivElement;
let root: Root;

const sampleContainer = {
  id: "c1",
  name: "redis",
  image: "redis:7",
  state: "running",
  status: "Up",
  ports: "0.0.0.0:6379->6379/tcp",
  cpuPercent: 1.25,
  memoryBytes: 1_572_864,
};

const sampleImage = {
  id: "sha256:0cf1d6af5ca7abcdef",
  name: "nginx:latest",
  sizeBytes: 160_914_473,
  createdAtEpochSeconds: 1_774_419_134,
  usedBy: ["nginx"],
};

const sampleSettings: DockerEngineSettingsView = {
  info: {
    serverVersion: "24.0.7",
    storageDriver: "overlay2",
    loggingDriver: "json-file",
    operatingSystem: "Ubuntu 22.04",
    architecture: "x86_64",
    ncpu: 4,
    memTotalBytes: 8_312_487_936,
    dockerRootDir: "/var/lib/docker",
    liveRestoreEnabled: true,
  },
  config: {
    registryMirrors: ["https://mirror.example"],
    insecureRegistries: ["registry.local:5000"],
    logDriver: "json-file",
    logOpts: { maxSize: "10m", maxFile: "3" },
    liveRestore: true,
  },
  configPath: "/etc/docker/daemon.json",
  configExists: true,
  configRawPreserved: true,
};

const sampleNetwork = {
  id: "netabcdef012345",
  name: "docker_default",
  driver: "bridge",
  ipv4Subnet: "172.25.0.0/16",
  ipv4Gateway: "172.25.0.1",
  labels: "com.docker.compose.network:default",
  createdAtEpochSeconds: 1_723_228_332,
};

const sampleVolume = {
  name: "b47dde889551a55d",
  driver: "local",
  mountpoint: "/var/lib/docker/volumes/b47dde889551a55d/_data",
  scope: "local",
  labels: "com.docker.volume.anonymous:",
  createdAtEpochSeconds: 1_723_228_332,
  usedBy: ["redis"],
};

const sampleRegistry = {
  id: "11111111-1111-1111-1111-111111111111",
  profileId: "22222222-2222-2222-2222-222222222222",
  url: "docker.io",
  name: "docker-official",
  username: "runory",
  namespace: "library",
  remarks: "hub",
  updatedAtEpochSeconds: 1_723_228_332,
};

const panelProps = {
  sessionId: "session-1",
  settings: sampleSettings as DockerEngineSettingsView | null,
  networks: [] as typeof sampleNetwork[],
  volumes: [] as typeof sampleVolume[],
  registries: [] as typeof sampleRegistry[],
  onReloadSettings: async () => undefined,
  onApplySettings: async () => undefined,
  onImageAction: async () => undefined,
  onNetworkAction: async () => undefined,
  onVolumeAction: async () => undefined,
  onRegistryUpsert: async () => undefined,
  onRegistryDelete: async () => undefined,
  actionButtons: () => null,
};

beforeEach(() => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  container = document.createElement("div");
  document.body.append(container);
  root = createRoot(container);
});

afterEach(async () => {
  await act(async () => { root.unmount(); });
  container.remove();
  vi.unstubAllGlobals();
});

describe("DockerPanel", () => {
  it.each(["en-US", "zh-CN"] as const)("renders vertical docker tabs in %s", async (language) => {
    await i18n.changeLanguage(language);
    await act(async () => {
      root.render(
        <DockerPanel
          {...panelProps}
          containers={[sampleContainer]}
          images={[]}
        />,
      );
    });

    const tablist = container.querySelector(`[role="tablist"][aria-label="${i18n.t("operations.dockerTabs")}"]`);
    expect(tablist).not.toBeNull();
    expect(tablist?.getAttribute("aria-orientation")).toBe("vertical");

    const tabs = [
      "containers",
      "images",
      "onlineImages",
      "networks",
      "volumes",
      "registry",
      "settings",
    ] as const;
    for (const id of tabs) {
      const tab = container.querySelector(`#docker-tab-${id}`);
      expect(tab?.textContent).toContain(i18n.t(`operations.dockerTab.${id}`));
    }

    expect(container.textContent).toContain("redis");
    expect(container.textContent).toContain("redis:7");
    expect(container.textContent).toContain(i18n.t("operations.ports"));
    expect(container.textContent).toContain(i18n.t("operations.cpu"));
    expect(container.textContent).toContain(i18n.t("operations.memory"));
    expect(container.textContent).toContain("0.0.0.0:6379->6379/tcp");
    expect(container.textContent).toContain("1.25%");
    expect(container.textContent).toContain("1.5 MB");
  });

  it("disables start action for running containers", async () => {
    await i18n.changeLanguage("en-US");
    const actionButtons = vi.fn((_kind: "docker", _target: string, label: string, state: string) => {
      const startDisabled = state === "running";
      return (
        <button type="button" disabled={startDisabled} aria-label={`start ${label}`}>
          start
        </button>
      );
    });
    const exited = {
      ...sampleContainer,
      id: "c2",
      name: "mysql",
      state: "exited",
      status: "Exited (0)",
      ports: "",
      cpuPercent: 0,
      memoryBytes: 0,
    };

    await act(async () => {
      root.render(
        <DockerPanel
          {...panelProps}
          containers={[sampleContainer, exited]}
          images={[]}
          actionButtons={actionButtons}
        />,
      );
    });

    expect(actionButtons).toHaveBeenCalledWith("docker", "c1", "redis", "running");
    expect(actionButtons).toHaveBeenCalledWith("docker", "c2", "mysql", "exited");
    expect(container.querySelector<HTMLButtonElement>('button[aria-label="start redis"]')?.disabled).toBe(true);
    expect(container.querySelector<HTMLButtonElement>('button[aria-label="start mysql"]')?.disabled).toBe(false);
  });

  it.each(["en-US", "zh-CN"] as const)("renders images workspace in %s", async (language) => {
    await i18n.changeLanguage(language);
    const onImagesTab = vi.fn();
    await act(async () => {
      root.render(
        <DockerPanel
          {...panelProps}
          containers={[sampleContainer]}
          images={[sampleImage]}
          onImagesTab={onImagesTab}
        />,
      );
    });

    const imagesTab = container.querySelector<HTMLButtonElement>("#docker-tab-images");
    expect(imagesTab).not.toBeNull();
    await act(async () => { imagesTab!.click(); });

    expect(onImagesTab).toHaveBeenCalled();
    expect(container.textContent).toContain(i18n.t("operations.dockerImages.pull"));
    expect(container.textContent).toContain(i18n.t("operations.dockerImages.prune"));
    expect(container.textContent).toContain(i18n.t("operations.dockerImages.name"));
    expect(container.textContent).toContain(i18n.t("operations.dockerImages.usedBy"));
    expect(container.textContent).toContain("nginx:latest");
    expect(container.textContent).toContain("0cf1d6af5ca7");
    expect(container.textContent).toContain("nginx");
    expect(container.textContent).not.toContain(i18n.t("operations.dockerTabUnavailable"));
    expect(container.querySelector(`input[placeholder="${i18n.t("operations.dockerImages.searchPlaceholder")}"]`)).not.toBeNull();
  });

  it("shows empty state when there are no images", async () => {
    await i18n.changeLanguage("en-US");
    await act(async () => {
      root.render(
        <DockerPanel
          {...panelProps}
          containers={[]}
          images={[]}
        />,
      );
    });

    const imagesTab = container.querySelector<HTMLButtonElement>("#docker-tab-images");
    await act(async () => { imagesTab!.click(); });
    expect(container.textContent).toContain(i18n.t("operations.dockerImages.empty"));
    expect(container.textContent).toContain(i18n.t("operations.dockerImages.total", { count: 0 }));
  });

  it.each(["en-US", "zh-CN"] as const)("renders networks workspace in %s", async (language) => {
    await i18n.changeLanguage(language);
    const onNetworksTab = vi.fn();
    await act(async () => {
      root.render(
        <DockerPanel
          {...panelProps}
          containers={[]}
          images={[]}
          networks={[sampleNetwork]}
          onNetworksTab={onNetworksTab}
        />,
      );
    });

    const networksTab = container.querySelector<HTMLButtonElement>("#docker-tab-networks");
    expect(networksTab).not.toBeNull();
    await act(async () => { networksTab!.click(); });

    expect(onNetworksTab).toHaveBeenCalled();
    expect(container.textContent).toContain(i18n.t("operations.dockerNetworks.create"));
    expect(container.textContent).toContain(i18n.t("operations.dockerNetworks.prune"));
    expect(container.textContent).toContain(i18n.t("operations.dockerNetworks.name"));
    expect(container.textContent).toContain(i18n.t("operations.dockerNetworks.driver"));
    expect(container.textContent).toContain(i18n.t("operations.dockerNetworks.subnet"));
    expect(container.textContent).toContain("docker_default");
    expect(container.textContent).toContain("172.25.0.0/16");
    expect(container.textContent).not.toContain(i18n.t("operations.dockerTabUnavailable"));
  });

  it.each(["en-US", "zh-CN"] as const)("renders volumes workspace in %s", async (language) => {
    await i18n.changeLanguage(language);
    const onVolumesTab = vi.fn();
    await act(async () => {
      root.render(
        <DockerPanel
          {...panelProps}
          containers={[]}
          images={[]}
          volumes={[sampleVolume]}
          onVolumesTab={onVolumesTab}
        />,
      );
    });

    const volumesTab = container.querySelector<HTMLButtonElement>("#docker-tab-volumes");
    expect(volumesTab).not.toBeNull();
    await act(async () => { volumesTab!.click(); });

    expect(onVolumesTab).toHaveBeenCalled();
    expect(container.textContent).toContain(i18n.t("operations.dockerVolumes.create"));
    expect(container.textContent).toContain(i18n.t("operations.dockerVolumes.prune"));
    expect(container.textContent).toContain(i18n.t("operations.dockerVolumes.name"));
    expect(container.textContent).toContain(i18n.t("operations.dockerVolumes.mountpoint"));
    expect(container.textContent).toContain(i18n.t("operations.dockerVolumes.usedBy"));
    expect(container.textContent).toContain("b47dde889551a55d");
    expect(container.textContent).toContain("/var/lib/docker/volumes/b47dde889551a55d/_data");
    expect(container.textContent).toContain("redis");
    expect(container.textContent).not.toContain(i18n.t("operations.dockerTabUnavailable"));
  });

  it.each(["en-US", "zh-CN"] as const)("renders registries workspace in %s", async (language) => {
    await i18n.changeLanguage(language);
    const onRegistriesTab = vi.fn();
    const onRegistryUpsert = vi.fn(async () => undefined);

    await act(async () => {
      root.render(
        <DockerPanel
          {...panelProps}
          containers={[]}
          images={[]}
          registries={[sampleRegistry]}
          onRegistriesTab={onRegistriesTab}
          onRegistryUpsert={onRegistryUpsert}
        />,
      );
    });

    const registryTab = container.querySelector<HTMLButtonElement>("#docker-tab-registry");
    expect(registryTab).not.toBeNull();
    await act(async () => { registryTab!.click(); });

    expect(onRegistriesTab).toHaveBeenCalled();
    expect(container.textContent).toContain(i18n.t("operations.dockerRegistries.create"));
    expect(container.textContent).toContain(i18n.t("operations.dockerRegistries.url"));
    expect(container.textContent).toContain(i18n.t("operations.dockerRegistries.username"));
    expect(container.textContent).toContain(i18n.t("operations.dockerRegistries.name"));
    expect(container.textContent).toContain("docker.io");
    expect(container.textContent).toContain("docker-official");
    expect(container.textContent).toContain("runory");
    expect(container.textContent).not.toContain(i18n.t("operations.dockerTabUnavailable"));

    const addButton = Array.from(container.querySelectorAll("button")).find((button) =>
      button.textContent?.includes(i18n.t("operations.dockerRegistries.create")),
    );
    expect(addButton).toBeTruthy();
    await act(async () => { addButton!.click(); });

    expect(document.body.textContent).toContain(i18n.t("operations.dockerRegistries.createTitle"));
    const urlInput = document.body.querySelector<HTMLInputElement>("#docker-registry-url");
    const nameInput = document.body.querySelector<HTMLInputElement>("#docker-registry-name");
    const userInput = document.body.querySelector<HTMLInputElement>("#docker-registry-username");
    const passwordInput = document.body.querySelector<HTMLInputElement>("#docker-registry-password");
    const namespaceInput = document.body.querySelector<HTMLInputElement>("#docker-registry-namespace");
    expect(urlInput).not.toBeNull();
    expect(nameInput).not.toBeNull();
    expect(userInput).not.toBeNull();
    expect(passwordInput).not.toBeNull();
    expect(namespaceInput).not.toBeNull();

    await act(async () => {
      const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")?.set;
      setter?.call(urlInput!, "ccr.ccs.tencentyun.com");
      urlInput!.dispatchEvent(new Event("input", { bubbles: true }));
      setter?.call(nameInput!, "tencent");
      nameInput!.dispatchEvent(new Event("input", { bubbles: true }));
      setter?.call(userInput!, "alice");
      userInput!.dispatchEvent(new Event("input", { bubbles: true }));
      setter?.call(passwordInput!, "secret");
      passwordInput!.dispatchEvent(new Event("input", { bubbles: true }));
      setter?.call(namespaceInput!, "ns");
      namespaceInput!.dispatchEvent(new Event("input", { bubbles: true }));
    });

    const submitButton = Array.from(document.body.querySelectorAll('[role="dialog"] button')).find((button) =>
      button.textContent?.includes(i18n.t("operations.dockerRegistries.create")),
    );
    expect(submitButton).toBeTruthy();
    await act(async () => { submitButton!.click(); });

    expect(onRegistryUpsert).toHaveBeenCalledWith({
      id: undefined,
      url: "ccr.ccs.tencentyun.com",
      name: "tencent",
      username: "alice",
      password: "secret",
      namespace: "ns",
      remarks: "",
    });
  });

  it.each(["en-US", "zh-CN"] as const)("renders online images workspace in %s", async (language) => {
    await i18n.changeLanguage(language);
    const onImagesTab = vi.fn();
    const { searchDockerImages } = await import("../../lib/tauri/infrastructure");
    vi.mocked(searchDockerImages).mockClear();

    await act(async () => {
      root.render(
        <DockerPanel
          {...panelProps}
          containers={[]}
          images={[sampleImage]}
          onImagesTab={onImagesTab}
        />,
      );
    });

    const onlineTab = container.querySelector<HTMLButtonElement>("#docker-tab-onlineImages");
    expect(onlineTab).not.toBeNull();
    await act(async () => { onlineTab!.click(); });
    await act(async () => { await Promise.resolve(); });

    expect(onImagesTab).toHaveBeenCalled();
    expect(container.textContent).not.toContain(i18n.t("operations.dockerTabUnavailable"));
    expect(searchDockerImages).toHaveBeenCalledWith("session-1", "", 300);
    expect(container.textContent).toContain("nginx");
    expect(container.textContent).toContain(i18n.t("operations.dockerOnlineImages.version"));
    expect(container.textContent).toContain(i18n.t("operations.dockerOnlineImages.sourceOfficial"));
    const input = container.querySelector<HTMLInputElement>(
      `input[placeholder="${i18n.t("operations.dockerOnlineImages.searchPlaceholder")}"]`,
    );
    expect(input).not.toBeNull();

    const form = input!.closest("form");
    expect(form).not.toBeNull();
    await act(async () => {
      const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")?.set;
      setter?.call(input!, "nginx");
      input!.dispatchEvent(new Event("input", { bubbles: true }));
      form!.requestSubmit();
    });
    await act(async () => { await Promise.resolve(); });

    expect(searchDockerImages).toHaveBeenCalledWith("session-1", "nginx", 100);
    expect(container.textContent).toContain("19712");
    expect(container.textContent).toContain(i18n.t("operations.dockerOnlineImages.update"));
    expect(container.textContent).toContain(i18n.t("operations.dockerOnlineImages.delete"));
    expect(container.textContent).toContain("bitnami/redis");
    expect(container.textContent).toContain(i18n.t("operations.dockerOnlineImages.pull"));
  });

  it.each(["en-US", "zh-CN"] as const)("renders settings workspace and confirms save restart in %s", async (language) => {
    await i18n.changeLanguage(language);
    const onSettingsTab = vi.fn();
    const onApplySettings = vi.fn(async () => undefined);
    const onReloadSettings = vi.fn(async () => undefined);

    await act(async () => {
      root.render(
        <DockerPanel
          {...panelProps}
          containers={[]}
          images={[]}
          onSettingsTab={onSettingsTab}
          onApplySettings={onApplySettings}
          onReloadSettings={onReloadSettings}
        />,
      );
    });

    const settingsTab = container.querySelector<HTMLButtonElement>("#docker-tab-settings");
    expect(settingsTab).not.toBeNull();
    await act(async () => { settingsTab!.click(); });

    expect(onSettingsTab).toHaveBeenCalled();
    expect(container.textContent).not.toContain(i18n.t("operations.dockerTabUnavailable"));
    expect(container.textContent).toContain(i18n.t("operations.dockerSettings.engineInfo"));
    expect(container.textContent).toContain("24.0.7");
    expect(container.textContent).toContain(i18n.t("operations.dockerSettings.registryMirrors"));
    const mirrorInput = container.querySelector<HTMLInputElement>(
      `input[placeholder="${i18n.t("operations.dockerSettings.mirrorPlaceholder")}"]`,
    );
    expect(mirrorInput?.value).toBe("https://mirror.example");
    expect(container.textContent).toContain(i18n.t("operations.dockerSettings.saveAndRestart"));

    const saveButton = Array.from(container.querySelectorAll("button")).find((button) =>
      button.textContent?.includes(i18n.t("operations.dockerSettings.saveAndRestart")),
    );
    expect(saveButton).toBeTruthy();
    await act(async () => { saveButton!.click(); });

    expect(document.body.textContent).toContain(i18n.t("operations.dockerSettings.confirmRestart"));
    const confirmButton = Array.from(document.body.querySelectorAll('[role="dialog"] button')).find((button) =>
      button.textContent?.includes(i18n.t("operations.dockerSettings.saveAndRestart")),
    );
    expect(confirmButton).toBeTruthy();
    await act(async () => { confirmButton!.click(); });

    expect(onApplySettings).toHaveBeenCalledWith({
      registryMirrors: ["https://mirror.example"],
      insecureRegistries: ["registry.local:5000"],
      logDriver: "json-file",
      logOpts: { maxSize: "10m", maxFile: "3" },
      liveRestore: true,
    });
    expect(onReloadSettings).toHaveBeenCalled();
  });
});

describe("matchLocalImages", () => {
  it("matches repository names across tags and library prefix", () => {
    expect(matchLocalImages([sampleImage], "nginx").map((image) => image.id)).toEqual([sampleImage.id]);
    expect(matchLocalImages([sampleImage], "library/nginx").map((image) => image.id)).toEqual([sampleImage.id]);
    expect(matchLocalImages([{ ...sampleImage, name: "library/nginx:1" }], "nginx").map((image) => image.id)).toEqual([sampleImage.id]);
    expect(matchLocalImages([sampleImage], "redis")).toEqual([]);
  });
});
