// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import i18n from "../../../i18n";
import type { ServerProfile } from "../../../types/domain";
import { InspectorPanel } from "./InspectorPanel";

let container: HTMLDivElement;
let root: Root;

const gateway: ServerProfile = { id: "gateway", name: "Edge Gateway", host: "203.0.113.10", port: 2222, username: "ops", groupId: null, authMethod: "privateKey", connectionRoute: { type: "direct" }, sortOrder: 0, createdAt: "", updatedAt: "" };
const target: ServerProfile = { id: "target", name: "Private API", host: "10.0.2.8", port: 22, username: "deploy", groupId: null, authMethod: "password", connectionRoute: { type: "jumpHost", profileId: gateway.id }, sortOrder: 1, createdAt: "", updatedAt: "" };

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

describe("jump-host inspector route", () => {
  it("shows the gateway, target, endpoints, explanation, and accessible route diagram", async () => {
    await act(async () => { root.render(<InspectorPanel profile={target} jumpProfile={gateway} state="connected" connected onNewTerminal={vi.fn()} onDisconnect={vi.fn()} onEdit={vi.fn()} />); });

    const route = container.querySelector(".inspector-jump-route");
    expect(route?.textContent).toContain(i18n.t("profile.routeJumpHost"));
    expect(route?.textContent).toContain("ops@203.0.113.10:2222");
    expect(route?.textContent).toContain("deploy@10.0.2.8:22");
    expect(route?.textContent).toContain(i18n.t("inspector.jumpRouteDescription", { jumpHost: gateway.name, target: target.name }));
    expect(route?.querySelector(".inspector-route-diagram")?.getAttribute("aria-label")).toBe(i18n.t("inspector.jumpRouteDiagramLabel", { jumpHost: gateway.name, target: target.name }));
  });
});
