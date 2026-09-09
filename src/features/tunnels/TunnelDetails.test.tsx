// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import i18n from "../../i18n";
import type { TunnelStatus, TunnelView } from "../../types/tunnels";
import { TunnelDetails } from "./TunnelDetails";

const initial: TunnelView = {
  rule: { id: "a", name: "Database", profileId: "profile", localPort: 13306, targetHost: "db.internal", targetPort: 3306 },
  status: { state: "running", sessionId: "background", startedAt: 10, checkedAt: null,
    health: "unchecked", errorCode: null, activeConnections: 0, bytesSent: 100, bytesReceived: 200, events: [] },
};
let root: Root;
let container: HTMLDivElement;
async function render(status: Partial<TunnelStatus> = {}, id = "a") {
  await act(async () => root.render(<TunnelDetails
    tunnel={{ rule: { ...initial.rule, id }, status: { ...initial.status, ...status } }}
    profileName="Gateway" busy={false} onClose={() => {}} onCopy={() => {}} onCheck={() => {}} onSession={() => {}}
  />));
}
function activity() {
  const route = container.querySelector(".tunnel-route")!;
  return [route.getAttribute("data-sending"), route.getAttribute("data-receiving")];
}
beforeEach(async () => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  await i18n.changeLanguage("en-US");
  vi.useFakeTimers();
  container = document.createElement("div"); document.body.append(container); root = createRoot(container);
});
afterEach(async () => {
  await act(async () => root.unmount());
  expect(vi.getTimerCount()).toBe(0);
  container.remove(); vi.useRealTimers(); vi.unstubAllGlobals();
});

describe("tunnel route traffic animation", () => {
  it("does not animate historical bytes, listener state, connections or health alone", async () => {
    await render(); expect(activity()).toEqual(["false", "false"]);
    await render({ activeConnections: 2, health: "reachable", checkedAt: 20 });
    expect(activity()).toEqual(["false", "false"]);
    expect(container.querySelectorAll('.tunnel-flow[aria-hidden="true"]')).toHaveLength(2);
  });
  it("animates only the direction whose byte counter increases", async () => {
    await render();
    await render({ bytesSent: 110 }); expect(activity()).toEqual(["true", "false"]);
    await render({ bytesSent: 110, bytesReceived: 220 }); expect(activity()).toEqual(["false", "true"]);
    await render({ bytesSent: 120, bytesReceived: 230 }); expect(activity()).toEqual(["true", "true"]);
  });
  it("expires activity without fresh growth even when polling stalls", async () => {
    await render(); await render({ bytesSent: 120 });
    await act(async () => vi.advanceTimersByTime(2000));
    await render({ bytesSent: 120 });
    expect(activity()).toEqual(["true", "false"]);
    await act(async () => vi.advanceTimersByTime(400));
    expect(activity()).toEqual(["false", "false"]);
  });
  it("extends activity for continued transfer and cancels it immediately on stop", async () => {
    await render(); await render({ bytesSent: 120 });
    await act(async () => vi.advanceTimersByTime(2000));
    await render({ bytesSent: 140 });
    await act(async () => vi.advanceTimersByTime(1000));
    expect(activity()).toEqual(["true", "false"]);
    await render({ state: "stopped", bytesSent: 140 });
    expect(activity()).toEqual(["false", "false"]);
    expect(vi.getTimerCount()).toBe(0);
  });
  it("resets the baseline when selecting another rule or reconnecting", async () => {
    await render(); await render({ bytesSent: 120 });
    await render({ bytesSent: 1000 }, "b"); expect(activity()).toEqual(["false", "false"]);
    await render({ bytesSent: 1100 }, "b"); expect(activity()).toEqual(["true", "false"]);
    await render({ sessionId: "new-background", bytesSent: 1200 }, "b");
    expect(activity()).toEqual(["false", "false"]);
    await render({ sessionId: "new-background", startedAt: 30, bytesSent: 1300 }, "b");
    expect(activity()).toEqual(["false", "false"]);
  });
  it("treats counter resets and interruptions as idle", async () => {
    await render(); await render({ bytesSent: 120, bytesReceived: 220 });
    await render({ bytesSent: 0, bytesReceived: 0 }); expect(activity()).toEqual(["false", "false"]);
    await render({ bytesSent: 10, bytesReceived: 0 }); expect(activity()).toEqual(["true", "false"]);
    await render({ state: "interrupted", bytesSent: 10, bytesReceived: 0 });
    expect(activity()).toEqual(["false", "false"]);
  });
});
