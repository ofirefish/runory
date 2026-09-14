"use client";

import { useEffect, useMemo, useState } from "react";
import Link from "next/link";
import { ArrowRight, CheckCircle2, LoaderCircle } from "lucide-react";
import { Button } from "@/components/ui/button";
import { buildDesktopEmailConfirmDeepLink } from "@/lib/desktop-deep-link";
import { type Locale, getDictionary } from "@/lib/i18n";
import { createSupabaseBrowserClient } from "@/lib/supabase/browser";

export function OpenAppClient({ locale }: { locale: Locale }) {
  const t = getDictionary(locale).auth;
  const [deepLink, setDeepLink] = useState<string | null>(null);
  const [status, setStatus] = useState<"loading" | "ready" | "missing">("loading");

  useEffect(() => {
    let active = true;
    console.info("[runory:auth]", "web:open-app:start", { locale });
    const supabase = createSupabaseBrowserClient();
    if (!supabase) {
      console.info("[runory:auth]", "web:open-app:missing_config");
      setStatus("missing");
      return;
    }
    void supabase.auth.getSession().then(({ data }) => {
      if (!active) return;
      const session = data.session;
      if (!session?.access_token || !session.refresh_token) {
        console.info("[runory:auth]", "web:open-app:missing_session");
        setStatus("missing");
        return;
      }
      const link = buildDesktopEmailConfirmDeepLink(session.access_token, session.refresh_token);
      setDeepLink(link);
      setStatus("ready");
      console.info("[runory:auth]", "web:open-app:launch_desktop", {
        userId: session.user.id,
        email: session.user.email ?? null,
        deepLink: "runory://auth/callback#…",
      });
      window.location.href = link;
    }).catch((error: unknown) => {
      console.info("[runory:auth]", "web:open-app:session_error", {
        message: error instanceof Error ? error.message : String(error),
      });
      if (active) setStatus("missing");
    });
    return () => { active = false; };
  }, [locale]);

  const accountHref = useMemo(() => `/${locale}/account`, [locale]);

  return (
    <div className="auth-panel">
      <div>
        <p className="eyebrow">RUNORY CLOUD</p>
        <h1>{t.openAppTitle}</h1>
        <p className="auth-description">{t.openAppDescription}</p>
      </div>
      {status === "loading" && (
        <p className="mt-8 flex items-center gap-2 text-sm text-muted-foreground">
          <LoaderCircle className="animate-spin" size={16} />
          {t.openAppOpening}
        </p>
      )}
      {status === "ready" && deepLink && (
        <div className="mt-8 grid gap-4">
          <p className="flex items-start gap-2 text-sm text-emerald-600">
            <CheckCircle2 className="mt-0.5 shrink-0" size={17} />
            {t.openAppOpening}
          </p>
          <Button asChild className="w-full">
            <a href={deepLink}><ArrowRight size={16} />{t.openAppButton}</a>
          </Button>
          <Link href={accountHref} className="text-sm text-muted-foreground hover:text-foreground">{t.openAppAccount}</Link>
        </div>
      )}
      {status === "missing" && (
        <div className="mt-8 grid gap-4">
          <p className="form-message error" role="status">{t.openAppMissing}</p>
          <Button asChild className="w-full">
            <Link href={`/${locale}/auth/sign-in`}><ArrowRight size={16} />{t.signIn}</Link>
          </Button>
        </div>
      )}
    </div>
  );
}
