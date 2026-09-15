import { Boxes } from "lucide-react";
import { notFound, redirect } from "next/navigation";
import { AdminStatus } from "@/components/portal/admin-status";
import { PortalShell } from "@/components/portal/portal-shell";
import { Badge } from "@/components/ui/badge";
import { Card } from "@/components/ui/card";
import { getAdminContext } from "@/lib/data/admin";
import { getDictionary, isLocale } from "@/lib/i18n";

export default async function SkillsPage({ params }: { params: Promise<{ locale: string }> }) {
  const { locale } = await params;
  if (!isLocale(locale)) notFound();
  const result = await getAdminContext();
  if (result.status === "signedOut") redirect(`/${locale}/auth/sign-in`);
  const t = getDictionary(locale);
  return <PortalShell locale={locale} area="admin-skills">{result.status !== "ok" ? <AdminStatus locale={locale} status={result.status} /> : <Card className="reserved-card"><Boxes size={26} /><Badge variant="secondary">{t.common.planned}</Badge><h1>{t.admin.skillsTitle}</h1><p>{t.admin.skillsDescription}</p></Card>}</PortalShell>;
}
