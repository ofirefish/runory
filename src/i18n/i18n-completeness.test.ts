import { describe, expect, it } from "vitest";
import enUS from "./locales/en-US.json";
import zhCN from "./locales/zh-CN.json";

describe("i18n resources", () => {
  it("localizes every tunnel runtime state, health and stable error", () => {
    const labels: Record<string, string>[] = [enUS, zhCN];
    const keys = ["tunnels.nav", "tunnels.title", "tunnels.disconnectConfirm", "errors.TUNNEL_CONNECTION_REQUIRED", "tunnels.backgroundHint",
      ...["stopped", "running", "interrupted", "error"].map((state) => `tunnels.state.${state}`),
      ...["unchecked", "reachable", "unreachable"].map((health) => `tunnels.health.${health}`),
      ...["INVALID", "NOT_FOUND", "RUNNING", "STOPPED", "PORT_IN_USE", "BIND_FAILED", "DENIED", "TARGET_FAILED", "SESSION_MISMATCH", "LIMIT"].map((error) => `errors.TUNNEL_${error}`)];
    for (const locale of labels) for (const key of keys) expect(locale[key], key).toBeTruthy();
  });

  it("keeps zh-CN and en-US key sets identical", () => {
    expect(Object.keys(zhCN).sort()).toEqual(Object.keys(enUS).sort());
  });

  it("contains every Files foundation label in both languages", () => {
    const keys = [
      "files.title",
      "files.breadcrumb",
      "files.editPath",
      "files.pathInput",
      "files.refresh",
      "files.name",
      "files.size",
      "files.modified",
      "files.permissions",
      "files.upload",
      "files.uploadHistory",
      "files.noUploadHistory",
      "files.openUploadDirectory",
      "files.dropToUpload",
      "files.download",
      "files.addFolder",
      "files.contextMenu",
      "files.newFolder",
      "files.rename",
      "files.delete",
      "transfers.title",
      "transfers.cancel",
      "transfers.retry",
      "dashboard.title",
      "operations.title",
      "operations.logSource",
      "settings.models.suggestions",
      "settings.models.noSuggestions",
      "deployment.title",
      "deployment.environmentSecurity",
      "deployment.confirmHint",
      "details.title",
    ] as const;
    for (const key of keys) {
      expect(enUS[key]).toBeTruthy();
      expect(zhCN[key]).toBeTruthy();
    }
  });
});
