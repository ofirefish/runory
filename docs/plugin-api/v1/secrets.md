# Secrets (API 1)

## Shape

Opaque references only:

```text
secret://<path>
```

Rules (Core `validate_secret_ref_shape`):

- Must start with `secret://`
- Length ≤ 256
- ASCII alphanumeric plus `: / . - _`
- No NUL

## Binding

- Bindings live in process memory via Core secret resolver.
- Not persisted in `plugins.db`.
- Restart clears bindings; unbound refs fail closed.
- Requires permission `secret.reference` for secret-consuming actions.

## Workflow usage

Env-style values:

```yaml
# literal
TOKEN: "public"

# explicit value wrapper
TOKEN:
  value: "public"

# opaque ref (never plaintext in context)
DB_PASSWORD:
  secretRef: secret://django/shop
```

UI input schemas may mark fields with `"format": "secret-ref"`.

## Hard rules

1. Secret plaintext never enters WorkflowContext, ActionResult outputs, or audit logs.
2. `ssh.command` V1 **rejects** `secretRef` env entries — use `remote.fs.write_env` or future dedicated actions.
3. Resolved values are zeroized after use where Core supports it.
4. Redaction helpers scrub common secret patterns from remote stdout/stderr before persistence/UI.
