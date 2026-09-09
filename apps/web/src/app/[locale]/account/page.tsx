import { Database, LockKeyhole, RefreshCw } from "lucide-react";
import { notFound, redirect } from "next/navigation";
import { Badge } from "@/components/ui/badge";
import { Card } from "@/components/ui/card";
import { ConfigurationPanel } from "@/components/portal/configuration-panel";
import { PortalShell } from "@/components/portal/portal-shell";
import { ProfileForm } from "@/components/portal/profile-form";
import { loadAccountDashboard } from "@/lib/data/account";
import { getDictionary, isLocale } from "@/lib/i18n";

function formatDate(value: string, locale: string) {
  return new Intl.DateTimeFormat(locale, { dateStyle: "medium", timeStyle: "short" }).format(new Date(value));
}

export default async function AccountPage({ params }: { params: Promise<{ locale: string }> }) {
  const { locale } = await params;
  if (!isLocale(locale)) notFound();
  const t = getDictionary(locale);
  const result = await loadAccountDashboard();
  if (result.status === "signedOut") redirect(`/${locale}/auth/sign-in`);
  if (result.status === "configuration") {
    return <PortalShell locale={locale} area="account"><ConfigurationPanel title={t.common.configuration} description={t.common.configurationDescription} /></PortalShell>;
  }
  return (
    <PortalShell locale={locale} area="account">
      <>
        <header className="portal-header"><p className="eyebrow">{t.portal.eyebrow}</p><h1>{t.portal.title}</h1><p>{t.portal.description}</p></header>
        <section className="portal-grid">
          <Card className="portal-card"><h2>{t.portal.profileTitle}</h2><ProfileForm locale={locale} displayName={result.account.displayName} email={result.account.email} /></Card>
          <Card className="portal-card boundary-card"><LockKeyhole size={22} /><h2>{t.portal.privacyTitle}</h2><p>{t.portal.privacyDescription}</p></Card>
        </section>
        <section className="mt-10"><div className="section-heading"><h2>{t.portal.workspaces}</h2><Badge variant="secondary"><LockKeyhole size={13} />{t.portal.encrypted}</Badge></div>
          {result.account.workspaces.length ? <div className="workspace-grid">{result.account.workspaces.map((workspace) => <Card key={workspace.id} className="workspace-card"><div className="flex items-center justify-between gap-3"><Database size={19} className="text-cyan-300" /><Badge>{workspace.kind === "personal" ? t.portal.personalWorkspace : t.portal.teamWorkspace}</Badge></div><h3>{workspace.kind === "personal" ? t.portal.personalWorkspace : workspace.name}</h3><dl><div><dt>{t.portal.revision}</dt><dd>{workspace.revision ?? "—"}</dd></div><div><dt>{t.portal.lastSync}</dt><dd>{workspace.updatedAt ? formatDate(workspace.updatedAt, locale) : t.portal.noSync}</dd></div></dl></Card>)}</div> : <div className="empty-state"><RefreshCw size={20} /><p>{t.portal.noWorkspace}</p></div>}
        </section>
      </>
    </PortalShell>
  );
}
