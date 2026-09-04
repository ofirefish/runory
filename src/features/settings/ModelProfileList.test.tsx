import { renderToStaticMarkup } from "react-dom/server";
import { beforeEach, describe, expect, it } from "vitest";
import i18n from "../../i18n";
import type { ModelProfile } from "../../types/agentic";
import { ModelProfileForm } from "./ModelProfileForm";
import { ModelProfileList } from "./ModelProfileList";

const profiles: ModelProfile[] = [
  { id: "a", active: true, kind: "chat-gpt", name: "My account", model: "gpt-5.4", baseUrl: "https://chatgpt.com", maxContextTokens: 128000, apiKeyConfigured: true, authMode: "oauth", oauthInProgress: false, connectedAccountLabel: null },
  { id: "b", active: false, kind: "open-ai-compatible", name: "Work API", model: "work-model", baseUrl: "https://example.com/v1", maxContextTokens: 128000, apiKeyConfigured: true, authMode: "api-key", oauthInProgress: false, connectedAccountLabel: null },
];

describe("model settings", () => {
  beforeEach(async () => { await i18n.changeLanguage("zh-CN"); });

  it("shows one enabled model, explicit switching and a disabled delete for the enabled model", () => {
    const html = renderToStaticMarkup(<ModelProfileList profiles={profiles} busy={false} onActivate={() => {}} onEdit={() => {}} onRemove={() => {}} />);
    expect(html.match(/data-active="true"/g)).toHaveLength(1);
    expect(html).toContain('aria-label="启用 Work API"');
    expect(html).not.toContain('aria-label="启用 My account"');
    expect(html).toMatch(/<button[^>]*disabled=""[^>]*aria-label="删除 My account"/);
    expect(html).toContain("work-model");
    expect(html).toContain("https://example.com/v1");
  });

  it("starts API key additions with an empty password field and editable model", () => {
    const html = renderToStaticMarkup(<ModelProfileForm profile={null} busy={false} onSave={async () => {}} onCancel={() => {}} />);
    expect(html).toMatch(/type="password"[^>]*required=""[^>]*value=""/);
    expect(html).toContain('aria-label="选择推荐模型"');
    expect(html).not.toContain("<datalist");
    expect(html).toContain('type="submit" disabled=""');
    expect(html).toContain('role="combobox"');
    expect(html).toContain(`aria-label="${i18n.t("settings.modelProvider")}"`);
  });

  it("allows editing an authorized account without requesting an API key", () => {
    const html = renderToStaticMarkup(<ModelProfileForm profile={profiles[0]} busy={false} onSave={async () => {}} onCancel={() => {}} />);
    expect(html).not.toContain('type="password"');
    expect(html).toContain('value="gpt-5.4"');
    expect(html).not.toContain('type="submit" disabled');
  });
});
