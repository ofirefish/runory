import Link from "next/link";
import { notFound, redirect } from "next/navigation";
import { AdminStatus } from "@/components/portal/admin-status";
import { PortalShell } from "@/components/portal/portal-shell";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table";
import { listAdminReleases } from "@/lib/data/releases";
import { getDictionary, isLocale } from "@/lib/i18n";
import type { ReleaseStatus } from "@/lib/releases";

function formatDate(value: string | null, locale: string, fallback: string) {
  return value ? new Intl.DateTimeFormat(locale, { dateStyle: "medium", timeStyle: "short" }).format(new Date(value)) : fallback;
}

function statusLabel(status: ReleaseStatus, t: ReturnType<typeof getDictionary>["admin"]) {
  if (status === "published") return t.releasesStatusPublished;
  if (status === "archived") return t.releasesStatusArchived;
  return t.releasesStatusDraft;
}

export default async function ReleasesPage({ params }: { params: Promise<{ locale: string }> }) {
  const { locale } = await params;
  if (!isLocale(locale)) notFound();
  const result = await listAdminReleases();
  if (result.status === "signedOut") redirect(`/${locale}/auth/sign-in`);
  const t = getDictionary(locale);
  return (
    <PortalShell locale={locale} area="admin-releases">
      {result.status !== "ok" ? (
        <AdminStatus locale={locale} status={result.status} />
      ) : (
        <>
          <header className="portal-header">
            <p className="eyebrow">{t.admin.eyebrow}</p>
            <div className="flex flex-wrap items-start justify-between gap-4">
              <div>
                <h1>{t.admin.releasesTitle}</h1>
                <p>{t.admin.releasesDescription}</p>
              </div>
              {result.role === "owner" ? (
                <Button asChild>
                  <Link href={`/${locale}/admin/releases/new`}>{t.admin.releasesNew}</Link>
                </Button>
              ) : null}
            </div>
          </header>
          <div className="table-frame">
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>{t.admin.releasesVersion}</TableHead>
                  <TableHead>{t.admin.releasesStatus}</TableHead>
                  <TableHead>{t.admin.releasesLatest}</TableHead>
                  <TableHead>{t.admin.releasesPublishedAt}</TableHead>
                  <TableHead>
                    <span className="sr-only">{t.admin.view}</span>
                  </TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {result.releases.length ? (
                  result.releases.map(release => (
                    <TableRow key={release.id}>
                      <TableCell className="font-medium">{release.version}</TableCell>
                      <TableCell>
                        <Badge variant={release.status === "published" ? "default" : "secondary"}>
                          {statusLabel(release.status, t.admin)}
                        </Badge>
                      </TableCell>
                      <TableCell>{release.isLatest ? t.admin.releasesYes : t.admin.releasesNo}</TableCell>
                      <TableCell>{formatDate(release.publishedAt, locale, t.admin.never)}</TableCell>
                      <TableCell>
                        <Button asChild variant="ghost" size="sm">
                          <Link href={`/${locale}/admin/releases/${release.id}`}>{t.admin.view}</Link>
                        </Button>
                      </TableCell>
                    </TableRow>
                  ))
                ) : (
                  <TableRow>
                    <TableCell colSpan={5} className="py-12 text-center text-muted-foreground">
                      {t.admin.releasesEmpty}
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
