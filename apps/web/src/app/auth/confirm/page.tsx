"use client";

import { useEffect, useState } from "react";
import { useRouter } from "next/navigation";
import { LoaderCircle } from "lucide-react";
import { completeWebAuthConfirm } from "@/lib/auth-confirm";
import { isLocale } from "@/lib/i18n";
import { createSupabaseBrowserClient } from "@/lib/supabase/browser";

type Status = "working" | "error";

export default function ConfirmAuthPage() {
  const router = useRouter();
  const [status, setStatus] = useState<Status>("working");

  useEffect(() => {
    let active = true;
    const href = window.location.href;
    const localeValue = new URL(href).searchParams.get("locale") ?? "zh-CN";
    const locale = isLocale(localeValue) ? localeValue : "zh-CN";
    const fail = () => {
      if (!active) return;
      setStatus("error");
      router.replace(`/${locale}/auth/sign-in?error=confirm`);
    };

    const supabase = createSupabaseBrowserClient();
    if (!supabase) {
      console.info("[runory:auth]", "web:confirm:missing_config");
      fail();
      return;
    }

    console.info("[runory:auth]", "web:confirm:start", {
      hasCode: href.includes("code="),
      hasTokenHash: href.includes("token_hash="),
      hasHash: href.includes("#"),
      locale,
    });

    void completeWebAuthConfirm(supabase, href)
      .then(async ({ session, strategy }) => {
        console.info("[runory:auth]", "web:confirm:ok", {
          strategy,
          userId: session.user.id,
          email: session.user.email ?? null,
        });
        const displayName = typeof session.user.user_metadata.display_name === "string"
          ? session.user.user_metadata.display_name
          : session.user.email?.split("@")[0] ?? "Runory";
        await supabase.rpc("ensure_my_profile", { target_display_name: displayName.slice(0, 64) });
        await supabase.rpc("ensure_personal_workspace");
        if (!active) return;
        window.history.replaceState({}, document.title, "/auth/confirm");
        router.replace(`/${locale}/auth/open-app`);
      })
      .catch((error: unknown) => {
        console.info("[runory:auth]", "web:confirm:error", {
          message: error instanceof Error ? error.message : String(error),
        });
        fail();
      });

    return () => { active = false; };
  }, [router]);

  return (
    <main className="flex min-h-screen items-center justify-center bg-background p-6 text-foreground">
      <p className="flex items-center gap-2 text-sm text-muted-foreground">
        <LoaderCircle className="animate-spin" size={16} />
        {status === "working" ? "Confirming your email…" : "Redirecting…"}
      </p>
    </main>
  );
}
