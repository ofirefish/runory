import { notFound, redirect } from "next/navigation";
import { AuthShell } from "@/components/auth/auth-shell";
import { OpenAppClient } from "@/components/auth/open-app-client";
import { isLocale } from "@/lib/i18n";
import { createSupabaseServerClient } from "@/lib/supabase/server";

export default async function OpenAppPage({ params }: { params: Promise<{ locale: string }> }) {
  const { locale: localeValue } = await params;
  if (!isLocale(localeValue)) notFound();
  const locale = localeValue;
  const supabase = await createSupabaseServerClient();
  if (!supabase) redirect(`/${locale}/auth/sign-in?error=confirm`);
  const { data: { user } } = await supabase.auth.getUser();
  if (!user) redirect(`/${locale}/auth/sign-in?error=confirm`);
  return <AuthShell locale={locale}><OpenAppClient locale={locale} /></AuthShell>;
}
