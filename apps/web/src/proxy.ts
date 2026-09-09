import type { NextRequest } from "next/server";
import { updateSupabaseSession } from "@/lib/supabase/proxy";

export async function proxy(request: NextRequest) {
  const requestHeaders = new Headers(request.headers);
  const locale = request.nextUrl.pathname.split("/")[1];
  requestHeaders.set("x-runory-locale", locale === "en-US" ? "en-US" : "zh-CN");
  return updateSupabaseSession(request, requestHeaders);
}

export const config = {
  matcher: ["/((?!_next/static|_next/image|favicon.ico|.*\\.(?:svg|png|jpg|jpeg|gif|webp)$).*)"],
};
