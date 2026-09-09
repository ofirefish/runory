import Link from "next/link";
import { Boxes, LayoutDashboard, LogOut, ShieldCheck, Store } from "lucide-react";
import { BrandMark } from "@/components/brand-mark";
import { Button } from "@/components/ui/button";
import { signOutAction } from "@/lib/auth-actions";
import { getDictionary, type Locale } from "@/lib/i18n";

export function PortalShell({ locale, area, children }: { locale: Locale; area: "account" | "admin"; children: React.ReactNode }) {
  const t = getDictionary(locale);
  return (
    <div className="portal-layout">
      <aside className="portal-sidebar">
        <Link href={`/${locale}`} className="flex items-center gap-3 font-semibold"><BrandMark />Runory</Link>
        <nav className="portal-nav" aria-label={t.nav.label}>
          <Link href={`/${locale}/account`} data-active={area === "account"}><LayoutDashboard size={17} />{t.common.account}</Link>
          <Link href={`/${locale}/admin/accounts`} data-active={area === "admin"}><ShieldCheck size={17} />{t.admin.accounts}</Link>
          <Link href={`/${locale}/admin/skills`} data-active={false}><Boxes size={17} />{t.admin.skills}</Link>
          <Link href={`/${locale}/admin/store`} data-active={false}><Store size={17} />{t.admin.store}</Link>
        </nav>
        <form action={signOutAction} className="mt-auto">
          <input type="hidden" name="locale" value={locale} />
          <Button type="submit" variant="ghost" className="w-full justify-start"><LogOut size={16} />{t.common.signOut}</Button>
        </form>
      </aside>
      <main className="portal-main">{children}</main>
    </div>
  );
}
