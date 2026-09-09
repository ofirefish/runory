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
  if (error) return NextResponse.redirect(new URL(`/${locale}/auth/sign-in?error=confirm`, request.url));
  const { data: { user } } = await supabase.auth.getUser();
  if (!user) return NextResponse.redirect(new URL(`/${locale}/auth/sign-in?error=confirm`, request.url));
  const displayName = typeof user.user_metadata.display_name === "string" ? user.user_metadata.display_name : user.email?.split("@")[0] ?? "Runory";
  await supabase.rpc("ensure_my_profile", { target_display_name: displayName.slice(0, 64) });
  await supabase.rpc("ensure_personal_workspace");
  return NextResponse.redirect(new URL(`/${locale}/account`, request.url));
}

