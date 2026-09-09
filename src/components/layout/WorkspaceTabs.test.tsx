// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import i18n from "../../i18n";
import type { SessionTab } from "../../stores/session-store";
import type { ServerProfile } from "../../types/domain";
import { WorkspaceTabs } from "./WorkspaceTabs";

let container: HTMLDivElement;
let root: Root;

const gateway: ServerProfile = { id: "gateway", name: "Edge Gateway", host: "gateway.test", port: 22, username: "ops", groupId: null, authMethod: "password", connectionRoute: { type: "direct" }, sortOrder: 0, createdAt: "", updatedAt: "" };
const target: ServerProfile = { id: "target", name: "Private API", host: "10.0.2.8", port: 22, username: "deploy", groupId: null, authMethod: "password", connectionRoute: { type: "jumpHost", profileId: gateway.id }, sortOrder: 1, createdAt: "", updatedAt: "" };
const tab: SessionTab = { id: "tab", profileId: target.id, sessionId: "session", connectionAttemptId: "attempt", state: "connected", view: "terminal" };

beforeEach(async () => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  await i18n.changeLanguage("en-US");
  container = document.createElement("div");
  document.body.append(container);
  root = createRoot(container);
});

afterEach(async () => {
  await act(async () => { root.unmount(); });
  container.remove();
  vi.unstubAllGlobals();
});

describe("workspace jump-host tab marker", () => {
  it("marks a jump-host session with the referenced gateway name", async () => {
    await act(async () => { root.render(<WorkspaceTabs tabs={[tab]} profiles={[gateway, target]} activeTabId={tab.id} onSelect={vi.fn()} onClose={vi.fn()} onContextMenu={vi.fn()} />); });

    const marker = container.querySelector(".jump-host-indicator");
    expect(marker?.getAttribute("aria-label")).toBe(i18n.t("profile.jumpHostIndicatorNamed", { name: gateway.name }));
    expect(container.querySelector(".workspace-tab-select")?.textContent).toContain(target.name);
  });
});
