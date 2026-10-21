import Link from "next/link";
import { notFound, redirect } from "next/navigation";
import { AdminStatus } from "@/components/portal/admin-status";
import { PortalShell } from "@/components/portal/portal-shell";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table";
import { getAdminMarketplacePlugin } from "@/lib/data/marketplace";
import { getDictionary, isLocale } from "@/lib/i18n";

function formatDate(value: string | null, locale: string, fallback: string) {
  return value
    ? new Intl.DateTimeFormat(locale, { dateStyle: "medium", timeStyle: "short" }).format(new Date(value))
    : fallback;
}

function shortHash(value: string) {
  return value.length > 12 ? `${value.slice(0, 8)}…${value.slice(-4)}` : value;
}

export default async function StorePluginDetailPage({
  params,
}: {
  params: Promise<{ locale: string; pluginId: string }>;
}) {
  const { locale, pluginId: rawId } = await params;
  if (!isLocale(locale)) notFound();
  const pluginId = decodeURIComponent(rawId);
  const result = await getAdminMarketplacePlugin(pluginId);
  if (result.status === "signedOut") redirect(`/${locale}/auth/sign-in`);
  if (result.status === "missing") notFound();
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
                <h1>{result.plugin.name}</h1>
                <p>
                  {result.plugin.id} · {result.plugin.publisherId}
                  {result.plugin.category ? ` · ${result.plugin.category}` : ""}
                </p>
              </div>
              <div className="flex flex-wrap gap-2">
                <Button asChild variant="ghost">
                  <Link href={`/${locale}/admin/store`}>{t.admin.storeBack}</Link>
                </Button>
                {result.role === "owner" ? (
                  <Button asChild>
                    <Link href={`/${locale}/admin/store/new?pluginId=${encodeURIComponent(result.plugin.id)}`}>
                      {t.admin.storePublishVersion}
                    </Link>
                  </Button>
                ) : null}
              </div>
            </div>
          </header>

          <div className="detail-grid mb-6">
            <Card className="portal-card">
              <h2>{t.admin.storeOverview}</h2>
              <dl className="grid gap-3 text-sm">
                <div>
                  <dt className="text-muted-foreground">{t.admin.storeVersion}</dt>
                  <dd className="font-medium">v{result.plugin.latestVersion}</dd>
                </div>
                <div>
                  <dt className="text-muted-foreground">{t.admin.storeStatus}</dt>
                  <dd className="mt-1">
                    {result.plugin.verified ? (
                      <Badge>{t.admin.storeVerifiedBadge}</Badge>
                    ) : (
                      <Badge variant="secondary">{t.admin.storeUnverifiedBadge}</Badge>
                    )}
                  </dd>
                </div>
                {result.plugin.description ? (
                  <div>
                    <dt className="text-muted-foreground">{t.admin.storeSummary}</dt>
                    <dd className="mt-1 leading-relaxed">{result.plugin.description}</dd>
                  </div>
                ) : null}
              </dl>
            </Card>
            <Card className="portal-card">
              <h2>{t.admin.storeApi}</h2>
              <p className="text-sm text-muted-foreground">{t.admin.storeApiHint}</p>
              <code className="mt-3 block break-all rounded-lg bg-muted px-3 py-2 text-xs">
                /api/v1/plugins/{result.plugin.id}
              </code>
            </Card>
          </div>

          <div className="table-frame">
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>{t.admin.storeVersion}</TableHead>
                  <TableHead>{t.admin.storeStatus}</TableHead>
                  <TableHead>SHA-256</TableHead>
                  <TableHead>{t.admin.storePublishedAt}</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {result.versions.length ? (
                  result.versions.map((version) => (
                    <TableRow key={version.version}>
                      <TableCell className="font-medium">
                        v{version.version}
                        {version.version === result.plugin.latestVersion ? (
                          <Badge className="ml-2" variant="secondary">
                            {t.admin.storeLatest}
                          </Badge>
                        ) : null}
                      </TableCell>
                      <TableCell>{version.status}</TableCell>
                      <TableCell className="font-mono text-xs">{shortHash(version.packageSha256)}</TableCell>
                      <TableCell>
                        {formatDate(version.publishedAt, locale, t.admin.never)}
                      </TableCell>
                    </TableRow>
                  ))
                ) : (
                  <TableRow>
                    <TableCell colSpan={4} className="py-10 text-center text-muted-foreground">
                      {t.admin.storeVersionsEmpty}
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
