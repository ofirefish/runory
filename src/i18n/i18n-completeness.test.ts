import { describe, expect, it } from "vitest";
import enUS from "./locales/en-US.json";
import zhCN from "./locales/zh-CN.json";

describe("i18n resources", () => {
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
