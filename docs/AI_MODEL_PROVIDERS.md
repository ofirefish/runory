# AI Model Providers

Runory's Agent Runtime can use a remote model through Quick connect (ChatGPT / OpenRouter account OAuth) or a BYOK OpenAI-compatible Chat Completions API. DeepSeek and Zhipu GLM remain API-key presets over the same Completions protocol.

## Architecture

```text
React settings (saved model list + Quick add / Add API Key)
  -> business-specific Tauri commands
    -> Rust ModelGateway / ProviderOAuth
      -> ChatGPT: browser PKCE -> OAuth tokens -> Codex Responses API
      -> OpenRouter: browser PKCE -> user API key -> Chat Completions
      -> DeepSeek / GLM / compatible: HTTPS Chat Completions + API key

Agent Runtime
  -> Native Typed Read Tools / Terminal observations
  -> sanitized evidence
  -> ModelGateway turn
  -> existing Policy / ChangeSet / Approval / Verification pipeline
```

The model is not a tool executor. It never receives SSH passwords, private keys, vault contents, or unrestricted terminal access. Remote output is untrusted. A model diagnosis remains non-authoritative for execution: Rust classifies risk and the user approves commands.

Every remote model turn reads the current application language through `SettingsService`. The system instructions require user-facing explanations, command purposes, analysis, questions, and final answers in that UI language (`en-US` or `zh-CN`), regardless of the language of the request, previous replies, or terminal observations. A language change applies to the next turn of an existing run, including approval resumes; commands, JSON protocol fields, paths, and evidence remain unchanged.

## Quick connect

Open **Settings → Agent & models → Quick add**:

- **ChatGPT**: opens the system browser to `auth.openai.com`, completes PKCE on a localhost callback, stores access/refresh tokens in the local credential vault, and calls the ChatGPT/Codex Responses endpoint. The model can be changed using Edit.
- **OpenRouter**: opens OpenRouter's OAuth page, exchanges the code for a user-controlled API key, and uses `https://openrouter.ai/api/v1/chat/completions`.

OAuth is Desktop-first. Mobile builds compile but return `MODEL_OAUTH_UNSUPPORTED` for browser authorization; use Add API Key there.

The main page lists saved configurations. The first addition is enabled automatically; later additions keep the current model enabled. Enable switches the single active profile in Rust and persists the selection. Multiple configurations for the same provider have independent credentials. Enable a different profile before deleting the active one.

`agent-model.json` stores profile metadata, opaque credential references, and the active ID. Tokens and API keys are stored through `CredentialService` in `CredentialVault`; the UI receives only configuration status. Legacy single-model JSON is read compatibly and migrated on the next successful mutation. A locked vault keeps metadata visible but blocks credential-dependent changes. They must never appear in logs, AgentEvent payloads, or React-owned persistent storage.

## Add API Key

Open **Settings → Agent & models → Add API Key**:

- **Name (identifier)**: optional local label (max 50 characters).
- **Provider**: OpenAI, Anthropic, Google, GLM, DeepSeek, Qwen, Kimi, MiniMax, OpenRouter, or Custom, in that order.
- **Base URL**: provider defaults; all except OpenAI, Anthropic, and Google support editable endpoints.
- **API key**: pasted BYOK credential with show/hide toggle.
- **Model**: editable model ID with provider suggestions.

Editing never returns a saved key to React. Leave the key empty to keep it; changing the endpoint requires a new key. Test model checks the currently enabled configuration without saving a draft or switching providers.

Presets:

- **OpenAI** → `https://api.openai.com/v1` (Chat Completions)
- **Anthropic** → Messages API at `https://api.anthropic.com/v1/messages`
- **Google** → Gemini OpenAI-compatible endpoint
- **OpenRouter** → `https://openrouter.ai/api/v1` (Chat Completions; also available via Quick connect OAuth)
- **Custom** → user-supplied HTTPS base URL

GLM, DeepSeek, Qwen, Kimi, and MiniMax use the existing Rust OpenAI-compatible transport. Each has its own persisted provider identity. Model suggestions remain editable. MiniMax requests separate reasoning from answer content; Kimi uses the provider's default sampling parameters.

Preset references: [GLM](https://docs.bigmodel.cn/cn/guide/models/text/glm-4.7), [DeepSeek](https://api-docs.deepseek.com/), [Qwen](https://help.aliyun.com/en/model-studio/model-calling-in-sub-workspace), [Kimi](https://platform.kimi.ai/docs/overview), [MiniMax](https://platform.minimaxi.com/docs/api-reference/text-openai-api).

## Why there is no separate managed backend yet

BYOK and account-linked OAuth fit Runory's local-first architecture. A separate managed backend would add authentication, key custody, quotas, abuse prevention, billing, telemetry, regional routing, and operational ownership without improving the local execution boundary.

Introduce a standalone backend only for an explicitly authorized **Runory Managed AI** product. Keep tool execution and approvals in the local Rust Runtime.

## Verification

The ChatGPT account transport requests SSE with `stream: true` and `store: false`, uses the account claim for routing when present, and keeps stream processing in Rust. It omits the API-key path's output-token and JSON-object formatting parameters; the existing reasoner still validates the completed JSON decision. Text is accepted only after `response.completed`; interrupted, incomplete, failed, or oversized responses are rejected. Provider failures retain stable error codes and the Agent panel renders one error alert per run.

The Rust test suite covers endpoint normalization, HTTPS enforcement, ChatGPT SSE framing and completion, UTF-8 fragmentation, transport failures, OAuth PKCE helpers, disconnect reset, and evidence binding. Frontend checks cover error deduplication, TypeScript and i18n completeness. Live provider calls are intentionally not part of CI because they require user credentials; verify those from the settings UI and a connected test SSH target.
