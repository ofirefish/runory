import Link from "next/link";
import { notFound, redirect } from "next/navigation";
import { AdminStatus } from "@/components/portal/admin-status";
import { PortalShell } from "@/components/portal/portal-shell";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table";
import { listAdminMarketplacePlugins } from "@/lib/data/marketplace";
import { getDictionary, isLocale } from "@/lib/i18n";

function formatDate(value: string | null, locale: string, fallback: string) {
  return value
    ? new Intl.DateTimeFormat(locale, { dateStyle: "medium", timeStyle: "short" }).format(new Date(value))
    : fallback;
}

export default async function StorePage({ params }: { params: Promise<{ locale: string }> }) {
  const { locale } = await params;
  if (!isLocale(locale)) notFound();
  const result = await listAdminMarketplacePlugins();
  if (result.status === "signedOut") redirect(`/${locale}/auth/sign-in`);
  const t = getDictionary(locale);

  return (
    <PortalShell locale={locale} area="admin-store">
      {result.status !== "ok" ? (
        <AdminStatus locale={locale} status={result.status} />
      ) : (
        <>
          <header className="portal-header">
            <p className="eyebrow">{t.admin.eyebrow}</p>
            <div className="flex flex-wrap items-start justify-between gap-4">
              <div>
                <h1>{t.admin.storeTitle}</h1>
                <p>{t.admin.storeLead}</p>
              </div>
              {result.role === "owner" ? (
                <Button asChild>
                  <Link href={`/${locale}/admin/store/new`}>{t.admin.storePublishNew}</Link>
                </Button>
              ) : null}
            </div>
            {result.role !== "owner" ? (
              <p className="mt-4 text-sm text-muted-foreground">{t.admin.storeReadOnly}</p>
            ) : null}
          </header>

          <div className="table-frame">
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>{t.admin.storeName}</TableHead>
                  <TableHead>{t.admin.storePluginId}</TableHead>
                  <TableHead>{t.admin.storeVersion}</TableHead>
                  <TableHead>{t.admin.storeCategory}</TableHead>
                  <TableHead>{t.admin.storeStatus}</TableHead>
                  <TableHead>{t.admin.storeUpdated}</TableHead>
                  <TableHead>
                    <span className="sr-only">{t.admin.view}</span>
                  </TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {result.plugins.length ? (
                  result.plugins.map((plugin) => (
                    <TableRow key={plugin.id}>
                      <TableCell className="font-medium">{plugin.name}</TableCell>
                      <TableCell className="font-mono text-xs text-muted-foreground">{plugin.id}</TableCell>
                      <TableCell>v{plugin.latestVersion}</TableCell>
                      <TableCell>{plugin.category ?? "—"}</TableCell>
                      <TableCell>
                        {plugin.verified ? (
                          <Badge>{t.admin.storeVerifiedBadge}</Badge>
                        ) : (
                          <Badge variant="secondary">{t.admin.storeUnverifiedBadge}</Badge>
                        )}
                      </TableCell>
                      <TableCell>{formatDate(plugin.updatedAt, locale, t.admin.never)}</TableCell>
                      <TableCell>
                        <Button asChild variant="ghost" size="sm">
                          <Link href={`/${locale}/admin/store/${encodeURIComponent(plugin.id)}`}>
                            {t.admin.view}
                          </Link>
                        </Button>
                      </TableCell>
                    </TableRow>
                  ))
                ) : (
                  <TableRow>
                    <TableCell colSpan={7} className="py-12 text-center text-muted-foreground">
                      <div className="grid justify-items-center gap-3">
                        <p>{t.admin.storeEmpty}</p>
                        {result.role === "owner" ? (
                          <Button asChild size="sm">
                            <Link href={`/${locale}/admin/store/new`}>{t.admin.storePublishNew}</Link>
                          </Button>
                        ) : null}
                      </div>
                    </TableCell>
                  </TableRow>
                )}
              </TableBody>
            </Table>
          </div>
        </>
      )}
    </PortalShell>
  );
}
