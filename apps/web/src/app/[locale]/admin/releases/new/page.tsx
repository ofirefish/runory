import Link from "next/link";
import { ArrowLeft } from "lucide-react";
import { notFound, redirect } from "next/navigation";
import { AdminStatus } from "@/components/portal/admin-status";
import { PortalShell } from "@/components/portal/portal-shell";
import { ReleaseForm } from "@/components/portal/release-form";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { getAdminContext } from "@/lib/data/admin";
import { getDictionary, isLocale } from "@/lib/i18n";

export default async function NewReleasePage({ params }: { params: Promise<{ locale: string }> }) {
  const { locale } = await params;
  if (!isLocale(locale)) notFound();
  const context = await getAdminContext();
  if (context.status === "signedOut") redirect(`/${locale}/auth/sign-in`);
  const t = getDictionary(locale);
  return (
    <PortalShell locale={locale} area="admin-releases">
      {context.status !== "ok" ? (
        <AdminStatus locale={locale} status={context.status} />
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
            <h1>{t.admin.releasesNew}</h1>
            <p>{t.admin.releasesDescription}</p>
          </header>
          <Card className="portal-card">
            <ReleaseForm locale={locale} readOnly={context.role !== "owner"} />
          </Card>
        </>
      )}
    </PortalShell>
  );
}
