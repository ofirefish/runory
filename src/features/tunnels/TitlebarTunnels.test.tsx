// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import i18n from "../../i18n";
import type { TunnelView } from "../../types/tunnels";
import { TitlebarTunnels } from "./TitlebarTunnels";

const source = vi.hoisted(() => ({ items: [] as TunnelView[], loading: false, error: null as string | null }));
vi.mock("./use-tunnels", () => ({ useTunnels: () => source }));
const view = (id: string, sent = 0, received = 0): TunnelView => ({
  rule: { id, name: `Database ${id}`, profileId: "profile", targetHost: "db.internal", targetPort: 5432, localPort: 15432 },
  status: { state: "running", sessionId: "session", startedAt: 1, checkedAt: null, health: "unchecked", errorCode: null, activeConnections: 2, bytesSent: sent, bytesReceived: received, events: [] },
});
let root: Root;
let container: HTMLDivElement;
const navigate = vi.fn();
const trigger = () => container.querySelector<HTMLButtonElement>(".tunnel-summary-trigger");
async function render(items = source.items) {
  source.items = items;
  await act(async () => root.render(<TitlebarTunnels onOpenTunnels={navigate} />));
}
async function tick(ms: number) { await act(async () => vi.advanceTimersByTime(ms)); }
beforeEach(async () => {
  vi.useFakeTimers(); vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  await i18n.changeLanguage("en-US");
  source.items = []; source.loading = false; source.error = null; navigate.mockReset();
  container = document.createElement("div"); document.body.append(container); root = createRoot(container);
});
afterEach(() => { act(() => root.unmount()); container.remove(); vi.useRealTimers(); vi.unstubAllGlobals(); });

describe("titlebar tunnel activity", () => {
  it("hides without running listeners and when status is unavailable", async () => {
    await render(); expect(trigger()).toBeNull();
    const stopped = view("a"); stopped.status.state = "stopped";
    await render([stopped]); expect(trigger()).toBeNull();
    await render([view("a")]); expect(trigger()).not.toBeNull();
    source.error = "UNKNOWN"; await render(); expect(trigger()).toBeNull();
    source.error = null; source.loading = true; await render(); expect(trigger()).toBeNull();
  });

  it("animates real directional deltas, goes idle, and expires activity on a stalled poll", async () => {
    await render([view("a", 1000, 2000)]);
    expect(trigger()?.dataset.sending).toBe("false");
    await tick(2000); await render([view("a", 3048, 2000)]);
    expect(trigger()?.dataset.sending).toBe("true");
    expect(trigger()?.dataset.receiving).toBe("false");
    await tick(2000); await render([view("a", 3048, 6096)]);
    expect(trigger()?.dataset.sending).toBe("false");
    expect(trigger()?.dataset.receiving).toBe("true");
    await tick(2000); await render([view("a", 3048, 6096)]);
    expect(trigger()?.dataset.receiving).toBe("false");
    await tick(2000); await render([view("a", 5000, 6096)]);
    expect(trigger()?.dataset.sending).toBe("true");
    await tick(2400); expect(trigger()?.dataset.sending).toBe("false");
  });

  it("does not interpret listener membership, restart or counter resets as traffic", async () => {
    await render([view("a", 5000)]);
    await tick(2000); await render([view("a", 5000), view("b", 100000)]);
    expect(trigger()?.dataset.sending).toBe("false");
    await tick(2000); await render([view("b", 100000)]);
    expect(trigger()?.dataset.sending).toBe("false");
    const restarted = view("b", 200000); restarted.status.startedAt = 2;
    await tick(2000); await render([restarted]); expect(trigger()?.dataset.sending).toBe("false");
    await tick(2000); await render([view("b", 0)]); expect(trigger()?.dataset.sending).toBe("false");
  });

  it("shows live routes and opens management", async () => {
    vi.useRealTimers();
    await render([view("a"), view("b")]);
    act(() => trigger()?.click());
    expect(document.querySelectorAll(".tunnel-summary-list li")).toHaveLength(2);
    expect(document.querySelector(".tunnel-summary-route")?.textContent).toContain("db.internal:5432");
    expect(document.querySelector(".tunnel-summary-status")?.textContent).toBe("Listening · idle");
    act(() => document.querySelector<HTMLButtonElement>(".tunnel-summary-footer button")?.click());
    expect(navigate).toHaveBeenCalledOnce();
    expect(document.querySelector(".tunnel-summary-popover")).toBeNull();
  });
});
