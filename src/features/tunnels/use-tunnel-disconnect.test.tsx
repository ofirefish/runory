// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import i18n from "../../i18n";
import { sessionTunnelImpact } from "../../lib/tauri/tunnels";
import { useSessionStore } from "../../stores/session-store";
import { useTunnelDisconnect } from "./use-tunnel-disconnect";

vi.mock("../../lib/tauri/tunnels", () => ({ sessionTunnelImpact: vi.fn() }));
const perform = vi.fn().mockResolvedValue(undefined);
function Harness() { const disconnect = useTunnelDisconnect(); return <><button onClick={() => void disconnect.request("tab", perform)}>disconnect fixture</button>{disconnect.dialog}</>; }
let root: Root; let container: HTMLDivElement;
async function click(text: string) { await act(async () => { [...document.querySelectorAll<HTMLButtonElement>("button")].find((item) => item.textContent === text)!.click(); }); }
beforeEach(async () => {
  vi.clearAllMocks(); vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true); await i18n.changeLanguage("en-US");
  useSessionStore.setState({ tabs: [{ id: "tab", profileId: "profile", sessionId: "session", connectionAttemptId: "attempt", state: "connected", view: "terminal" }] });
  vi.mocked(sessionTunnelImpact).mockResolvedValue([{ id: "rule", name: "Private DB", profileId: "profile", targetHost: "db", targetPort: 5432, localPort: 15432 }]);
  container = document.createElement("div"); document.body.append(container); root = createRoot(container);
  await act(async () => { root.render(<Harness />); });
});
afterEach(async () => { await act(async () => root.unmount()); container.remove(); vi.unstubAllGlobals(); });
it("shows impacted rules and waits for explicit confirmation", async () => {
  await click("disconnect fixture"); expect(perform).not.toHaveBeenCalled(); expect(document.body.textContent).toContain("Private DB");
  await click(i18n.t("common.cancel")); expect(perform).not.toHaveBeenCalled();
  await click("disconnect fixture"); await click(i18n.t("tunnels.disconnectConfirm")); expect(perform).toHaveBeenCalledOnce();
});
it("does not apply a stale confirmation to a replacement session", async () => {
  await click("disconnect fixture");
  useSessionStore.setState({ tabs: [{ ...useSessionStore.getState().tabs[0], sessionId: "replacement" }] });
  await click(i18n.t("tunnels.disconnectConfirm")); expect(perform).not.toHaveBeenCalled();
});
it("disconnects immediately when no runtime tunnels are affected", async () => {
  vi.mocked(sessionTunnelImpact).mockResolvedValue([]);
  await click("disconnect fixture"); expect(perform).toHaveBeenCalledOnce();
});
