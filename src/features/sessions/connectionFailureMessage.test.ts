import { beforeAll, describe, expect, it } from "vitest";
import i18n, { ensureLanguageResources } from "../../i18n";
import {
  connectionFailureMessage,
  formatFailureEndpoint,
} from "./connectionFailureMessage";

describe("formatFailureEndpoint", () => {
  it("wraps a present endpoint", () => {
    expect(formatFailureEndpoint("127.0.0.1:2222")).toBe(" (127.0.0.1:2222)");
  });

  it("returns empty when absent", () => {
    expect(formatFailureEndpoint(undefined)).toBe("");
  });
});

describe("connectionFailureMessage", () => {
  beforeAll(async () => {
    await ensureLanguageResources("zh-CN");
  });

  it("keeps direct SSH CONNECTION_LOST free of JumpServer or KoKo tips", async () => {
    await i18n.changeLanguage("en-US");
    const message = connectionFailureMessage(i18n.t.bind(i18n), "CONNECTION_LOST");
    expect(message).not.toMatch(/KoKo|JumpServer/i);
    expect(message).toBe(i18n.t("connection.errors.CONNECTION_LOST"));
  });

  it("uses JumpServer-specific guidance for bastion gateway failures", async () => {
    await i18n.changeLanguage("zh-CN");
    const message = connectionFailureMessage(i18n.t.bind(i18n), "BASTION_GATEWAY_UNREACHABLE", {
      provider: "jumpserver",
      endpoint: "192.168.1.10:2222",
    });
    expect(message).toContain("JumpServer");
    expect(message).toContain("192.168.1.10:2222");
    expect(message).not.toMatch(/KoKo/i);
  });

  it("uses generic bastion copy for non-JumpServer providers", async () => {
    await i18n.changeLanguage("en-US");
    const message = connectionFailureMessage(i18n.t.bind(i18n), "BASTION_KOKO_UNREACHABLE", {
      provider: "teleport",
      endpoint: "proxy.example:3022",
    });
    expect(message).not.toMatch(/KoKo|JumpServer/i);
    expect(message).toContain("proxy.example:3022");
  });
});
