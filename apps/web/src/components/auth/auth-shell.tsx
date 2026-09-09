import Link from "next/link";
import { ShieldCheck } from "lucide-react";
import { BrandMark } from "@/components/brand-mark";
import { getDictionary, type Locale } from "@/lib/i18n";

export function AuthShell({ locale, children }: { locale: Locale; children: React.ReactNode }) {
  const t = getDictionary(locale).auth;
  return (
    <main className="auth-shell">
      <div className="auth-aside">
        <Link href={`/${locale}`} className="flex items-center gap-3 font-semibold"><BrandMark />Runory</Link>
        <div className="mt-auto max-w-md"><ShieldCheck className="mb-5 text-cyan-300" size={26} /><p className="text-3xl font-semibold tracking-tight">{t.asideTitle}</p><p className="mt-3 leading-7 text-muted-foreground">{t.asideDescription}</p></div>
      </div>
      <div className="auth-main">{children}</div>
    </main>
  );
}
