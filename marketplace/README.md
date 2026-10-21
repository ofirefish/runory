# runory-marketplace (local MVP)

Local SQLite Marketplace for development. **Production Marketplace is hosted on the official website:**

```text
https://runory.app/api/v1/plugins
```

Desktop default `marketplaceUrl` is `https://runory.app`.

## Local development server

```bash
cargo run --manifest-path marketplace/Cargo.toml -- --root ./marketplace-data --bind 127.0.0.1:8787
```

Then set Settings `marketplaceUrl` to `http://127.0.0.1:8787`.

## Production APIs (apps/web + Supabase)

Compatible with Core `MarketplaceClient`:

- `GET /api/v1/plugins`
- `GET /api/v1/plugins/{id}`
- `GET /api/v1/plugins/{id}/versions`
- `GET /api/v1/plugins/{id}/versions/{version}`
- `GET /api/v1/plugins/{id}/download`
- `GET /api/v1/plugins/{id}/versions/{version}/download`
- `GET /api/v1/publishers/{id}`
- `GET /api/v1/publishers/{id}/keys`
- `GET /api/v1/security/revocations`
- `POST /api/v1/admin/publish` (owner session or `x-runory-marketplace-token`)
- `POST /api/v1/admin/publisher-keys`
- `POST /api/v1/admin/revoke`

Publish packages via Operations Console → Store, or:

```bash
node scripts/publish-marketplace-plugin.mjs \
  --url https://runory.app \
  --token "$MARKETPLACE_ADMIN_TOKEN" \
  --package tests/fixtures/plugins/runory.hello.runory-plugin \
  --plugin-id runory.hello \
  --name "Runory Hello" \
  --version 1.0.0
```
