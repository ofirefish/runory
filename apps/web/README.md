# Runory Web

Runory Web is the public landing page, optional cloud-sync account portal, and read-only operations console. It is intentionally isolated from the Tauri client and never decrypts synced infrastructure data.

## Local development

1. Copy `.env.example` to `.env.local` and set the Supabase project values.
2. Apply the repository Supabase migrations.
3. Run `pnpm --dir apps/web dev` from the repository root.

Required variables:

- `NEXT_PUBLIC_SUPABASE_URL`
- `NEXT_PUBLIC_SUPABASE_PUBLISHABLE_KEY`
- `SUPABASE_SECRET_KEY` (server-only)
- `RUNORY_APP_ORIGIN` (exact trusted origin, without a path)

## Landing page

The public homepage is available at `/zh-CN` and `/en-US`. Its content lives in
`src/lib/landing-copy.ts`; styles are scoped to `.landing` so account pages keep
their own theme. The workspace and AI tours use explicitly labeled sample data
and never establish connections or execute commands. Platform selection explains
package formats and links to GitHub Releases without claiming a package exists.

Run `pnpm --dir apps/web test`, `lint`, `typecheck`, and `build` from the workspace
root (the tests reuse the workspace's Vitest and jsdom dependencies). On Windows,
if another process holds `.next` files open, set `RUNORY_WEB_BUILD_DIR` to a fresh
directory such as `.next-validation` for both the build and subsequent `start`.

Public search pages are generated in both `zh-CN` and `en-US` for `/product`,
`/ssh`, `/operations`, `/ai`, `/security`, and `/download`. Keep the matching
content, metadata, keywords, canonical paths, language alternates, FAQs, and
structured data together in `src/lib/marketing-pages.ts`. `sitemap.ts` includes
only public marketing routes; authentication, account, and administration pages
are excluded from indexing.

## Vercel

Import the repository, select `apps/web` as the Root Directory, and add the four variables above for every deployed environment. Set `RUNORY_APP_ORIGIN` to that environment's canonical HTTPS origin. Add the matching confirmation and password-recovery URLs to the Supabase Auth redirect allow list.

## First platform administrator

The web application deliberately cannot promote an account. After applying the migrations, register the first confirmed user through the Supabase SQL Editor:

```sql
insert into public.platform_admins (user_id, role, created_by)
values ('<confirmed-auth-user-uuid>', 'owner', '<confirmed-auth-user-uuid>');
```

Subsequent administration remains a database-controlled operation until a separately reviewed privilege-management workflow is implemented.
