import Link from "next/link";
import { ArrowLeft } from "lucide-react";
import { notFound, redirect } from "next/navigation";
import { AdminStatus } from "@/components/portal/admin-status";
import { PortalShell } from "@/components/portal/portal-shell";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { getAdminAccount } from "@/lib/data/admin";
import { getDictionary, isLocale } from "@/lib/i18n";

function formatDate(value: string | null, locale: string, fallback: string) {
  return value ? new Intl.DateTimeFormat(locale, { dateStyle: "medium", timeStyle: "short" }).format(new Date(value)) : fallback;
}

export default async function AccountDetailPage({ params }: { params: Promise<{ locale: string; id: string }> }) {
  const { locale, id } = await params;
  if (!isLocale(locale)) notFound();
  const result = await getAdminAccount(id);
  if (result.status === "signedOut") redirect(`/${locale}/auth/sign-in`);
  if (result.status === "missing") notFound();
  const t = getDictionary(locale);
  const roleLabels: Record<string, string> = { owner: t.admin.roleOwner, admin: t.admin.roleAdmin, operator: t.admin.roleOperator, viewer: t.admin.roleViewer };
  return <PortalShell locale={locale} area="admin-accounts">{result.status !== "ok" ? <AdminStatus locale={locale} status={result.status} /> : <>
    <Button asChild variant="ghost" size="sm"><Link href={`/${locale}/admin/accounts`}><ArrowLeft size={15} />{t.admin.back}</Link></Button>
    <header className="portal-header compact"><p className="eyebrow">{t.admin.eyebrow}</p><h1>{t.admin.details}</h1><p>{result.account.email}</p></header>
    <section className="detail-grid"><Card className="portal-card"><h2>{t.portal.profileTitle}</h2><dl className="detail-list"><div><dt>{t.admin.displayName}</dt><dd>{result.account.displayName}</dd></div><div><dt>{t.admin.userId}</dt><dd className="font-mono text-xs">{result.account.id}</dd></div><div><dt>{t.admin.status}</dt><dd><Badge>{result.account.confirmed ? t.admin.confirmed : t.admin.pending}</Badge></dd></div><div><dt>{t.admin.created}</dt><dd>{formatDate(result.account.createdAt, locale, t.admin.never)}</dd></div><div><dt>{t.admin.lastSignIn}</dt><dd>{formatDate(result.account.lastSignInAt, locale, t.admin.never)}</dd></div><div><dt>{t.admin.syncObjects}</dt><dd>{result.account.syncObjectCount}</dd></div></dl></Card>
      <Card className="portal-card"><h2>{t.admin.memberships}</h2>{result.account.memberships.length ? <div className="membership-list">{result.account.memberships.map((membership) => <div key={membership.organizationId}><div><p>{membership.organizationKind === "personal" ? t.portal.personalWorkspace : membership.organizationName}</p><span>{membership.organizationKind === "personal" ? t.portal.personalWorkspace : t.portal.teamWorkspace}</span></div><Badge variant="secondary">{roleLabels[membership.role] ?? membership.role}</Badge></div>)}</div> : <p className="text-muted-foreground">{t.portal.noWorkspace}</p>}</Card></section>
  </>}</PortalShell>;
}
