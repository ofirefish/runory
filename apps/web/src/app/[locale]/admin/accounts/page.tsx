import Link from "next/link";
import { notFound, redirect } from "next/navigation";
import { AdminStatus } from "@/components/portal/admin-status";
import { PortalShell } from "@/components/portal/portal-shell";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table";
import { listAdminAccounts } from "@/lib/data/admin";
import { getDictionary, isLocale } from "@/lib/i18n";

function formatDate(value: string | null, locale: string, fallback: string) {
  return value ? new Intl.DateTimeFormat(locale, { dateStyle: "medium", timeStyle: "short" }).format(new Date(value)) : fallback;
}

export default async function AccountsPage({ params, searchParams }: { params: Promise<{ locale: string }>; searchParams: Promise<{ page?: string }> }) {
  const { locale } = await params;
  if (!isLocale(locale)) notFound();
  const query = await searchParams;
  const page = Math.max(1, Number.parseInt(query.page ?? "1", 10) || 1);
  const result = await listAdminAccounts(page);
  if (result.status === "signedOut") redirect(`/${locale}/auth/sign-in`);
  const t = getDictionary(locale);
  return <PortalShell locale={locale} area="admin-accounts">{result.status !== "ok" ? <AdminStatus locale={locale} status={result.status} /> : <>
    <header className="portal-header"><p className="eyebrow">{t.admin.eyebrow}</p><h1>{t.admin.title}</h1><p>{t.admin.description}</p></header>
    <div className="table-frame"><Table><TableHeader><TableRow><TableHead>{t.admin.email}</TableHead><TableHead>{t.admin.displayName}</TableHead><TableHead>{t.admin.status}</TableHead><TableHead>{t.admin.organizations}</TableHead><TableHead>{t.admin.lastSync}</TableHead><TableHead><span className="sr-only">{t.admin.view}</span></TableHead></TableRow></TableHeader><TableBody>{result.accounts.length ? result.accounts.map((account) => <TableRow key={account.id}><TableCell><div className="font-medium text-foreground">{account.email || "—"}</div><div className="mt-1 text-xs text-muted-foreground">{formatDate(account.createdAt, locale, t.admin.never)}</div></TableCell><TableCell>{account.displayName}</TableCell><TableCell><Badge variant={account.confirmed ? "default" : "secondary"}>{account.confirmed ? t.admin.confirmed : t.admin.pending}</Badge></TableCell><TableCell>{account.organizationCount}</TableCell><TableCell>{formatDate(account.lastSyncAt, locale, t.admin.never)}</TableCell><TableCell><Button asChild variant="ghost" size="sm"><Link href={`/${locale}/admin/accounts/${account.id}`}>{t.admin.view}</Link></Button></TableCell></TableRow>) : <TableRow><TableCell colSpan={6} className="py-12 text-center text-muted-foreground">{t.admin.empty}</TableCell></TableRow>}</TableBody></Table></div>
    <div className="pagination"><Button asChild variant="secondary" size="sm" className={page <= 1 ? "pointer-events-none opacity-50" : ""}><Link href={`/${locale}/admin/accounts?page=${page - 1}`}>{t.admin.previous}</Link></Button><span>{t.admin.page.replace("{page}", String(page))}</span><Button asChild variant="secondary" size="sm" className={!result.hasNext ? "pointer-events-none opacity-50" : ""}><Link href={`/${locale}/admin/accounts?page=${page + 1}`}>{t.admin.next}</Link></Button></div>
  </>}</PortalShell>;
}
