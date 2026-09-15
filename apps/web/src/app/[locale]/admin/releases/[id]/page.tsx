import Link from "next/link";
import { ArrowLeft } from "lucide-react";
import { notFound, redirect } from "next/navigation";
import { AdminStatus } from "@/components/portal/admin-status";
import { PortalShell } from "@/components/portal/portal-shell";
import { ReleaseForm } from "@/components/portal/release-form";
import { ReleaseStatusActions } from "@/components/portal/release-status-actions";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { getAdminRelease } from "@/lib/data/releases";
import { getDictionary, isLocale } from "@/lib/i18n";
import type { ReleaseStatus } from "@/lib/releases";

function statusLabel(status: ReleaseStatus, t: ReturnType<typeof getDictionary>["admin"]) {
  if (status === "published") return t.releasesStatusPublished;
  if (status === "archived") return t.releasesStatusArchived;
  return t.releasesStatusDraft;
}

export default async function ReleaseDetailPage({
  params,
}: {
  params: Promise<{ locale: string; id: string }>;
}) {
  const { locale, id } = await params;
  if (!isLocale(locale)) notFound();
  const result = await getAdminRelease(id);
  if (result.status === "signedOut") redirect(`/${locale}/auth/sign-in`);
  if (result.status === "missing") notFound();
  const t = getDictionary(locale);
  return (
    <PortalShell locale={locale} area="admin-releases">
      {result.status !== "ok" ? (
        <AdminStatus locale={locale} status={result.status} />
      ) : (
        <>
          <Button asChild variant="ghost" size="sm">
            <Link href={`/${locale}/admin/releases`}>
              <ArrowLeft size={15} />
              {t.admin.releasesBack}
            </Link>
          </Button>
          <header className="portal-header compact">
            <p className="eyebrow">{t.admin.eyebrow}</p>
            <h1>{t.admin.releasesEdit}</h1>
            <p className="flex flex-wrap items-center gap-2">
              <span className="font-mono">{result.release.version}</span>
              <Badge variant={result.release.status === "published" ? "default" : "secondary"}>
                {statusLabel(result.release.status, t.admin)}
              </Badge>
              {result.release.isLatest ? <Badge>{t.admin.releasesLatest}</Badge> : null}
            </p>
          </header>
          <Card className="portal-card mb-4">
            <h2 className="mb-4 text-base font-semibold">{t.admin.releasesStatus}</h2>
            <ReleaseStatusActions
              locale={locale}
              releaseId={result.release.id}
              status={result.release.status}
              isLatest={result.release.isLatest}
              readOnly={result.role !== "owner"}
            />
          </Card>
          <Card className="portal-card">
            <ReleaseForm locale={locale} release={result.release} readOnly={result.role !== "owner"} />
          </Card>
        </>
      )}
    </PortalShell>
  );
}
