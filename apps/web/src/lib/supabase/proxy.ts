import { createServerClient } from "@supabase/ssr";
import { type NextRequest, NextResponse } from "next/server";
import { getSupabasePublicConfig } from "./config";

export async function updateSupabaseSession(request: NextRequest, requestHeaders: Headers) {
  const config = getSupabasePublicConfig();
  const responseOptions = { request: { headers: requestHeaders } };
  if (!config) return NextResponse.next(responseOptions);
  let response = NextResponse.next(responseOptions);
  const supabase = createServerClient(config.url, config.publishableKey, {
    cookies: {
      getAll: () => request.cookies.getAll(),
      setAll: (items) => {
        items.forEach(({ name, value }) => request.cookies.set(name, value));
        response = NextResponse.next(responseOptions);
        items.forEach(({ name, value, options }) => response.cookies.set(name, value, options));
      },
    },
  });
  await supabase.auth.getClaims();
  return response;
}
