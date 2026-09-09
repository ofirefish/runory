import { NextResponse, type NextRequest } from "next/server";
import { isLocale } from "@/lib/i18n";
import { createSupabaseServerClient } from "@/lib/supabase/server";

export async function GET(request: NextRequest) {
  const code = request.nextUrl.searchParams.get("code");
  const localeValue = request.nextUrl.searchParams.get("locale") ?? "zh-CN";
  const locale = isLocale(localeValue) ? localeValue : "zh-CN";
  const supabase = await createSupabaseServerClient();
  if (!code || !supabase) return NextResponse.redirect(new URL(`/${locale}/auth/sign-in?error=confirm`, request.url));
  const { error } = await supabase.auth.exchangeCodeForSession(code);
  return NextResponse.redirect(new URL(error ? `/${locale}/auth/sign-in?error=confirm` : `/${locale}/auth/update-password`, request.url));
}
