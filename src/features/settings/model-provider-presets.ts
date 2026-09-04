import type { AdvancedProviderKind, ModelProviderKind } from "../../types/agentic";

export const chatgptModels = ["gpt-5.4", "gpt-5.3", "gpt-5.2", "gpt-4.1", "o4-mini"];
export const openRouterQuickModels = [
  "openai/gpt-4o-mini",
  "openai/gpt-4o",
  "anthropic/claude-sonnet-4",
  "google/gemini-2.5-flash",
  "deepseek/deepseek-chat",
];

const advancedProviders: AdvancedProviderKind[] = [
  "open-ai",
  "anthropic",
  "google",
  "glm",
  "deep-seek",
  "qwen",
  "kimi",
  "minimax",
  "open-router",
  "open-ai-compatible",
];

export const advancedPresets: Record<
  AdvancedProviderKind,
  { baseUrl: string; model: string; maxContextTokens: number; models: string[]; keyHintKey: string }
> = {
  "open-ai": {
    baseUrl: "https://api.openai.com/v1",
    model: "gpt-4.1-mini",
    maxContextTokens: 128000,
    models: ["gpt-4.1", "gpt-4.1-mini", "gpt-4o", "o4-mini", "o3"],
    keyHintKey: "settings.modelApiKeyHint.openai",
  },
  anthropic: {
    baseUrl: "https://api.anthropic.com",
    model: "claude-sonnet-4-5",
    maxContextTokens: 200000,
    models: ["claude-opus-4-5", "claude-sonnet-4-5", "claude-haiku-4-5"],
    keyHintKey: "settings.modelApiKeyHint.anthropic",
  },
  google: {
    baseUrl: "https://generativelanguage.googleapis.com/v1beta/openai",
    model: "gemini-2.5-flash",
    maxContextTokens: 128000,
    models: ["gemini-2.5-pro", "gemini-2.5-flash", "gemini-2.0-flash"],
    keyHintKey: "settings.modelApiKeyHint.google",
  },
  glm: {
    baseUrl: "https://open.bigmodel.cn/api/paas/v4",
    model: "glm-4.7",
    maxContextTokens: 128000,
    models: ["glm-4.7"],
    keyHintKey: "settings.models.keyHint",
  },
  "deep-seek": {
    baseUrl: "https://api.deepseek.com",
    model: "deepseek-v4-flash",
    maxContextTokens: 128000,
    models: ["deepseek-v4-flash", "deepseek-v4-pro"],
    keyHintKey: "settings.models.keyHint",
  },
  qwen: {
    baseUrl: "https://dashscope.aliyuncs.com/compatible-mode/v1",
    model: "qwen-plus",
    maxContextTokens: 128000,
    models: ["qwen-plus"],
    keyHintKey: "settings.models.keyHint",
  },
  kimi: {
    baseUrl: "https://api.moonshot.ai/v1",
    model: "kimi-k3",
    maxContextTokens: 128000,
    models: ["kimi-k3", "kimi-k2.6"],
    keyHintKey: "settings.models.keyHint",
  },
  minimax: {
    baseUrl: "https://api.minimax.cn/v1",
    model: "MiniMax-M3",
    maxContextTokens: 128000,
    models: ["MiniMax-M3", "MiniMax-M2.7", "MiniMax-M2.5"],
    keyHintKey: "settings.models.keyHint",
  },
  "open-router": {
    baseUrl: "https://openrouter.ai/api/v1",
    model: "openai/gpt-4o-mini",
    maxContextTokens: 128000,
    models: openRouterQuickModels,
    keyHintKey: "settings.modelApiKeyHint.openrouter",
  },
  "open-ai-compatible": {
    baseUrl: "",
    model: "",
    maxContextTokens: 8192,
    models: [],
    keyHintKey: "settings.modelApiKeyHint.custom",
  },
};

export function isAdvancedKind(kind: ModelProviderKind): kind is AdvancedProviderKind {
  return (advancedProviders as string[]).includes(kind);
}


export const providerLabelKeys: Record<ModelProviderKind, string> = {
  qwen: "settings.modelProvider.qwen", kimi: "settings.modelProvider.kimi", minimax: "settings.modelProvider.minimax",
  local: "settings.modelProvider.local", "chat-gpt": "settings.provider.chatgpt",
  "open-router": "settings.modelProvider.openrouter", "open-ai": "settings.modelProvider.openai",
  anthropic: "settings.modelProvider.anthropic", google: "settings.modelProvider.google",
  "open-ai-compatible": "settings.modelProvider.custom", "deep-seek": "settings.modelProvider.deepseek", glm: "settings.modelProvider.glm",
};
