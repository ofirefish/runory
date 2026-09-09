# AI Model Providers

Runory's Agent Runtime can use Runory Managed AI, Quick connect (ChatGPT / OpenRouter account OAuth), or a BYOK OpenAI-compatible Chat Completions API. DeepSeek and Zhipu GLM remain available as direct API-key presets as well as server-side routes behind the managed product.

## Architecture

```text
React settings (saved model list + Quick add / Add API Key)
  -> business-specific Tauri commands
    -> Rust ModelGateway / ProviderOAuth
      -> Runory Managed: Supabase account token -> agent-turn Edge Function
      -> ChatGPT: browser PKCE -> OAuth tokens -> Codex Responses API
      -> OpenRouter: browser PKCE -> user API key -> Chat Completions
      -> DeepSeek / GLM / compatible: HTTPS Chat Completions + API key

Agent Runtime
  -> Native Typed Read Tools / Terminal observations
  -> sanitized evidence
  -> ModelGateway turn
  -> existing Policy / ChangeSet / Approval / Verification pipeline

Runory Managed AI backend
  -> authenticate the Runory Cloud account
  -> reserve workspace credits atomically
  -> resolve the public model alias to a versioned GLM / DeepSeek route
  -> validate the single Runtime V2 decision
  -> settle actual token usage or release the reservation
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

## Runory Managed AI backend

The explicitly authorized managed product is implemented as the narrow `agent-turn` Supabase Edge Function. Open **Settings → Agent & models → Quick add**, choose **Runory Managed AI**, and bind the profile to a personal or team billing workspace. The saved profile contains only the Supabase project URL, public model alias, and workspace UUID. It never contains a GLM or DeepSeek provider key.

The Rust gateway loads the existing Runory Cloud access token from the platform credential store and sends a typed Runtime V2 turn. The backend authenticates the user, verifies workspace membership through the billing RPC, reserves credits under a request/idempotency UUID, resolves `runory-agent-fast` or `runory-agent-pro` through `model_price_versions`, calls the official provider API, validates the one-decision JSON contract, and settles actual input/output tokens. Provider failures release the hold; an under-reserved request remains `settlement-pending` for operator reconciliation and never silently creates a negative balance.

`runory-agent-fast` uses DeepSeek in non-thinking mode for Runtime V2 turns. These turns need one short structured decision, and DeepSeek's default high-effort thinking can otherwise consume the bounded output budget before emitting JSON. Provider response failures retain distinct stable codes for empty content, malformed/truncated JSON, invalid decision fields, invalid multiline commands, missing usage, and invalid provider envelopes.

This endpoint is intentionally not OpenAI-compatible and is not a general model proxy. It accepts only the Runtime V2 goal, bounded/redacted observations, user replies, language, workspace, public model alias, and output budget. SSH, Terminal injection, Tool Registry, Policy, risk classification, approvals, ChangeSet, verification, and rollback remain local Rust responsibilities.

Deployment requires `RUNORY_DEEPSEEK_API_KEY` and/or `RUNORY_GLM_API_KEY` as Edge Function secrets. Supabase service credentials are read only by the function. Apply the billing migrations, deploy `agent-turn`, and configure the desktop build with `VITE_SUPABASE_URL` plus `VITE_SUPABASE_PUBLISHABLE_KEY`. A provider key must never be placed in a `VITE_` variable.

## Verification

The ChatGPT account transport requests SSE with `stream: true` and `store: false`, uses the account claim for routing when present, and keeps stream processing in Rust. It omits the API-key path's output-token and JSON-object formatting parameters; the existing reasoner still validates the completed JSON decision. Text is accepted only after `response.completed`; interrupted, incomplete, failed, or oversized responses are rejected. Provider failures retain stable error codes and the Agent panel renders one error alert per run.

The Rust test suite covers endpoint normalization, HTTPS enforcement, ChatGPT SSE framing and completion, UTF-8 fragmentation, transport failures, OAuth PKCE helpers, disconnect reset, and evidence binding. Frontend checks cover error deduplication, TypeScript and i18n completeness. Live provider calls are intentionally not part of CI because they require user credentials; verify those from the settings UI and a connected test SSH target.
