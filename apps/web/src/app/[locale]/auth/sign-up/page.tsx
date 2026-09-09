import { notFound } from "next/navigation";
import { AuthForm } from "@/components/auth/auth-form";
import { AuthShell } from "@/components/auth/auth-shell";
import { isLocale } from "@/lib/i18n";

export default async function SignUpPage({ params }: { params: Promise<{ locale: string }> }) {
  const { locale } = await params;
  if (!isLocale(locale)) notFound();
  return <AuthShell locale={locale}><AuthForm locale={locale} mode="sign-up" /></AuthShell>;
}

