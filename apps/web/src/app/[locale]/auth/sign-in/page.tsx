import { notFound } from "next/navigation";
import { AuthForm } from "@/components/auth/auth-form";
import { AuthShell } from "@/components/auth/auth-shell";
import { isLocale } from "@/lib/i18n";

export default async function SignInPage({ params, searchParams }: { params: Promise<{ locale: string }>; searchParams: Promise<{ error?: string }> }) {
  const [{ locale }, query] = await Promise.all([params, searchParams]);
  if (!isLocale(locale)) notFound();
  return <AuthShell locale={locale}><AuthForm locale={locale} mode="sign-in" initialError={query.error === "confirm"} /></AuthShell>;
}

