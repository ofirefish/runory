import Link from "next/link";
import { notFound, redirect } from "next/navigation";
import { AdminStatus } from "@/components/portal/admin-status";
import { MarketplacePublishForm } from "@/components/portal/marketplace-publish-form";
import { PortalShell } from "@/components/portal/portal-shell";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { getAdminContext } from "@/lib/data/admin";
import { getAdminMarketplacePlugin } from "@/lib/data/marketplace";
import { getDictionary, isLocale } from "@/lib/i18n";

export default async function StorePublishPage({
  params,
  searchParams,
}: {
  params: Promise<{ locale: string }>;
  searchParams: Promise<{ pluginId?: string }>;
}) {
  const { locale } = await params;
  const query = await searchParams;
  if (!isLocale(locale)) notFound();

  const context = await getAdminContext();
  if (context.status === "signedOut") redirect(`/${locale}/auth/sign-in`);
  if (context.status !== "ok") {
    return (
      <PortalShell locale={locale} area="admin-store">
        <AdminStatus locale={locale} status={context.status} />
      </PortalShell>
    );
  }
  if (context.role !== "owner") redirect(`/${locale}/admin/store`);

  const t = getDictionary(locale);
  const existing = query.pluginId
    ? await getAdminMarketplacePlugin(query.pluginId)
    : null;
  const defaults =
    existing && existing.status === "ok"
      ? {
          pluginId: existing.plugin.id,
          publisherId: existing.plugin.publisherId,
          name: existing.plugin.name,
          category: existing.plugin.category ?? undefined,
          description: existing.plugin.description,
          verified: existing.plugin.verified,
        }
      : undefined;

  return (
    <PortalShell locale={locale} area="admin-store">
      <header className="portal-header">
        <p className="eyebrow">{t.admin.eyebrow}</p>
        <div className="flex flex-wrap items-start justify-between gap-4">
          <div>
            <h1>{defaults ? t.admin.storePublishVersionTitle : t.admin.storePublishTitle}</h1>
            <p>{t.admin.storePublishDescription}</p>
          </div>
          <Button asChild variant="ghost">
            <Link href={`/${locale}/admin/store`}>{t.admin.storeBack}</Link>
          </Button>
        </div>
      </header>
      <Card className="portal-card max-w-3xl">
        <MarketplacePublishForm locale={locale} defaults={defaults} />
      </Card>
    </PortalShell>
  );
}
