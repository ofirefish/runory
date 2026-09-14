// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from "vitest";
import i18n from "../i18n";
import { applyLanguage } from "./use-language";

describe("applyLanguage", () => {
  afterEach(async () => {
    await i18n.changeLanguage("en-US");
    document.documentElement.lang = "en-US";
  });

  it("applies the persisted language before first paint", async () => {
    await applyLanguage("zh-CN");
    expect(i18n.language).toBe("zh-CN");
    expect(document.documentElement.lang).toBe("zh-CN");
    expect(i18n.t("sidebar.settings")).toBe("设置");
  });
});
