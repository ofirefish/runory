// @vitest-environment jsdom
import { act, type ReactNode } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import i18n from "../../i18n";
import { useCatalogStore } from "../../stores/catalog-store";
import type { ServerProfile } from "../../types/domain";
import { ProfileDialog } from "./ProfileDialog";

vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));
vi.mock("../../lib/tauri/ssh", () => ({
  forgetPrivateKey: vi.fn().mockResolvedValue(undefined),
  importPrivateKey: vi.fn(),
}));
// Keep Command's real filtering while avoiding Popper's layout loop in jsdom.
vi.mock("../../components/ui/popover", () => ({
  Popover: ({ children }: { children: ReactNode }) => <>{children}</>,
  PopoverTrigger: ({ children }: { children: ReactNode }) => <>{children}</>,
  PopoverContent: ({ children }: { children: ReactNode }) => <div>{children}</div>,
}));

let container: HTMLDivElement;
let root: Root;
const onClose = vi.fn();
const createProfile = vi.fn().mockResolvedValue(undefined);

async function flush() {
  await act(async () => { await new Promise((resolve) => setTimeout(resolve, 0)); });
}

async function renderDialog(profile?: ServerProfile) {
  await act(async () => { root.render(<ProfileDialog profile={profile} onClose={onClose} />); });
  await flush();
}

async function type(name: string, value: string) {
  const input = document.body.querySelector<HTMLInputElement>(`input[name="${name}"]`);
  expect(input).toBeTruthy();
  await act(async () => {
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!.call(input, value);
    input!.dispatchEvent(new Event("input", { bubbles: true }));
  });
}

async function save() {
  const button = [...document.body.querySelectorAll<HTMLButtonElement>("button")].find((candidate) => candidate.textContent === i18n.t("common.save"));
  expect(button).toBeTruthy();
  await act(async () => { button!.click(); });
  await flush();
}

beforeEach(async () => {
  vi.clearAllMocks();
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  vi.stubGlobal("matchMedia", vi.fn().mockReturnValue({ matches: false }));
  vi.stubGlobal("ResizeObserver", class { observe() {} unobserve() {} disconnect() {} });
  Object.defineProperty(HTMLElement.prototype, "scrollIntoView", { configurable: true, value: vi.fn() });
  vi.spyOn(HTMLElement.prototype, "offsetWidth", "get").mockReturnValue(240);
  vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockReturnValue(36);
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockReturnValue({ x: 0, y: 0, top: 0, left: 0, right: 240, bottom: 36, width: 240, height: 36, toJSON() {} });
  const style = document.createElement("div").style;
  style.cssText = "position: static; display: block; visibility: visible; direction: ltr; width: 240px; height: 36px; transform: none; filter: none; perspective: none;";
  vi.spyOn(window, "getComputedStyle").mockReturnValue(style);
  await i18n.changeLanguage("en-US");
  useCatalogStore.setState({ groups: [], profiles: [], createProfile });
  container = document.createElement("div");
  document.body.append(container);
  root = createRoot(container);
});

afterEach(async () => {
  await act(async () => root.unmount());
  container.remove();
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

describe("profile dialog", () => {
  it("uses shadcn select triggers and reports validation beside each invalid field", async () => {
    await renderDialog();

    expect(document.body.querySelector<HTMLButtonElement>("#profile-group")?.getAttribute("role")).toBe("combobox");
    expect(document.body.querySelector<HTMLButtonElement>("#profile-route")?.getAttribute("role")).toBe("combobox");
    expect(document.body.querySelector<HTMLButtonElement>("#profile-auth")?.getAttribute("role")).toBe("combobox");
    expect([...document.body.querySelectorAll("select")].every((select) => select.getAttribute("aria-hidden") === "true")).toBe(true);
    expect(document.body.querySelector('[role="dialog"]')).toBeTruthy();

    await save();

    expect(document.body.textContent).toContain(i18n.t("validation.profileName"));
    expect(document.body.textContent).toContain(i18n.t("validation.profileHost"));
    expect(document.body.textContent).toContain(i18n.t("validation.profileUsername"));
    expect(createProfile).not.toHaveBeenCalled();
  });

  it("keeps the direct password connection request unchanged", async () => {
    await renderDialog();
    await type("name", "Production API");
    await type("host", "api.example.com");
    await type("username", "deploy");
    await save();

    expect(createProfile).toHaveBeenCalledWith({
      name: "Production API",
      host: "api.example.com",
      port: 22,
      username: "deploy",
      groupId: null,
      authMethod: "password",
      keySource: undefined,
      connectionRoute: { type: "direct" },
    });
    expect(onClose).toHaveBeenCalledOnce();
  });

  it("filters and selects groups and jump hosts from their search fields", async () => {
    const gatewayA: ServerProfile = {
      id: "gateway-a", name: "Gateway Alpha", host: "gateway-a.example.com", port: 22, username: "ops-a",
      groupId: null, authMethod: "password", connectionRoute: { type: "direct" }, sortOrder: 0, createdAt: "", updatedAt: "",
    };
    const gatewayB: ServerProfile = {
      ...gatewayA, id: "gateway-b", name: "Bastion Beta", host: "10.0.0.20", username: "ops-b", sortOrder: 1,
    };
    const target: ServerProfile = {
      ...gatewayA, id: "target", name: "Private API", host: "10.0.1.8", groupId: "production",
      connectionRoute: { type: "jumpHost", profileId: gatewayA.id }, sortOrder: 2,
    };
    useCatalogStore.setState({
      groups: [
        { id: "production", name: "Production", sortOrder: 0, collapsed: false, createdAt: "", updatedAt: "" },
        { id: "staging", name: "Staging", sortOrder: 1, collapsed: false, createdAt: "", updatedAt: "" },
      ],
      profiles: [gatewayA, gatewayB, target],
    });
    await renderDialog(target);

    const groupTrigger = document.body.querySelector<HTMLButtonElement>("#profile-group");
    expect(groupTrigger).toBeTruthy();
    const groupSearch = document.body.querySelector<HTMLInputElement>(`input[placeholder="${i18n.t("profile.groupSearchPlaceholder")}"]`);
    expect(groupSearch).toBeTruthy();
    await act(async () => {
      Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!.call(groupSearch, "stag");
      groupSearch!.dispatchEvent(new Event("input", { bubbles: true }));
    });
    await flush();
    const groupCommand = groupSearch!.closest<HTMLElement>("[cmdk-root]");
    const groupOptions = [...groupCommand!.querySelectorAll<HTMLElement>('[role="option"]')];
    expect(groupOptions.map((option) => option.textContent)).toEqual(["Staging"]);
    await act(async () => { groupOptions[0].click(); });
    expect(groupTrigger!.textContent).toContain("Staging");

    const jumpTrigger = document.body.querySelector<HTMLButtonElement>("#profile-jump-host");
    expect(jumpTrigger).toBeTruthy();
    const jumpSearch = document.body.querySelector<HTMLInputElement>(`input[placeholder="${i18n.t("profile.jumpHostSearchPlaceholder")}"]`);
    expect(jumpSearch).toBeTruthy();
    await act(async () => {
      Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!.call(jumpSearch, "10.0.0.20");
      jumpSearch!.dispatchEvent(new Event("input", { bubbles: true }));
    });
    await flush();
    const jumpCommand = jumpSearch!.closest<HTMLElement>("[cmdk-root]");
    const jumpOptions = [...jumpCommand!.querySelectorAll<HTMLElement>('[role="option"]')];
    expect(jumpOptions).toHaveLength(1);
    expect(jumpOptions[0].textContent).toContain("Bastion Beta");
    await act(async () => { jumpOptions[0].click(); });
    expect(jumpTrigger!.textContent).toContain("Bastion Beta");
  });
});
