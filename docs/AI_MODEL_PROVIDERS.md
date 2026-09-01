# AI Model Providers

Runory's Agent Runtime can use its deterministic local diagnosis provider or a remote OpenAI-compatible Chat Completions API. DeepSeek and Zhipu GLM are presets over the same protocol; custom compatible gateways are supported without adding provider-specific SDKs.

## Architecture

```text
React settings
  -> business-specific Tauri commands
  -> Rust ModelGateway
      -> encrypted CredentialVault (API key only)
      -> HTTPS OpenAI-compatible endpoint

Agent Runtime
  -> Native Typed Read Tools
  -> sanitized typed evidence
  -> ModelGateway diagnosis
  -> evidence-ID validation
  -> existing Policy / ChangeSet / Approval / Verification pipeline
```

The model is not a tool executor. It never receives SSH passwords, private keys, vault contents, or unrestricted terminal access. Remote output is untrusted: Runory rejects invented evidence IDs, invalid structured output, redirects, embedded URL credentials, and public plain-HTTP endpoints. A model diagnosis is always `R0` and cannot grant approval or increase tool scope.

## Configuration

Open **Settings → Agent model** and select:

- **Local rules**: no network call or API key.
- **DeepSeek**: defaults to `https://api.deepseek.com`; the model name remains editable.
- **GLM**: defaults to `https://open.bigmodel.cn/api/paas/v4`; the model name remains editable.
- **OpenAI-compatible**: supply an HTTPS base URL and model name. Plain HTTP is accepted only for `localhost`, `127.0.0.1`, and `::1` to support local inference servers.

The gateway appends `/chat/completions` unless the configured URL already ends with that path. API keys can be session-only or saved in Runory's encrypted credential vault. Saved keys require the vault to be unlocked after application restart.

The right-side Agent conversation uses the active SSH session and this provider for Doctor runs. It extracts only explicit URL/service hints from the prompt, executes the existing fixed typed read tools, and then asks the model to diagnose the resulting evidence. Conversation results stay in memory and are not written to browser storage.

## Why there is no separate backend yet

BYOK calls fit Runory's local-first architecture and both named providers expose compatible HTTPS APIs. A separate managed backend would add authentication, key custody, quotas, abuse prevention, billing, telemetry, regional routing, and operational ownership without improving the local execution boundary.

Introduce a standalone backend only for an explicitly authorized **Runory Managed AI** product. Keep it stateless for prompts where possible, authenticate every device/user request, never accept SSH credentials, enforce request and token quotas, use provider keys only from a server-side secret manager, redact logs, and return the same constrained diagnosis schema. Tool execution and approvals must remain in the local Rust Runtime.

## Verification

The Rust test suite covers endpoint normalization, HTTPS enforcement, evidence binding, structured response validation, and the invariant that remote model output cannot acquire execution authority. Frontend checks cover TypeScript and i18n completeness. Live provider calls are intentionally not part of CI because they require billable user credentials; verify those from the settings UI and a connected test SSH target.
