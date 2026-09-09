import { CheckCircle2, KeyRound, TriangleAlert } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "../../components/ui/button";
import { Input } from "../../components/ui/input";
import { consumeCloudAuthRedirect, updateCloudPassword } from "../../lib/supabase/cloud";

type AuthWebMode = "confirm" | "reset";
type PageState = "loading" | "ready" | "success" | "error";

export function AuthWebPage({ mode }: { mode: AuthWebMode }) {
  const { t } = useTranslation();
  const password = useRef<HTMLInputElement>(null);
  const confirmation = useRef<HTMLInputElement>(null);
  const redirectConsumed = useRef(false);
  const [state, setState] = useState<PageState>("loading");
  const [formError, setFormError] = useState(false);

  useEffect(() => {
    if (redirectConsumed.current) return;
    redirectConsumed.current = true;
    void consumeCloudAuthRedirect()
      .then(() => setState(mode === "confirm" ? "success" : "ready"))
      .catch(() => setState("error"));
  }, [mode]);

  const updatePassword = async () => {
    const value = password.current?.value ?? "";
    const repeated = confirmation.current?.value ?? "";
    if (value.length < 8 || value !== repeated) {
      setFormError(true);
      return;
    }
    setFormError(false);
    setState("loading");
    try {
      await updateCloudPassword(value);
      if (password.current) password.current.value = "";
      if (confirmation.current) confirmation.current.value = "";
      setState("success");
    } catch {
      if (password.current) password.current.value = "";
      if (confirmation.current) confirmation.current.value = "";
      setFormError(true);
      setState("ready");
    }
  };

  return <main className="flex min-h-screen items-center justify-center bg-[hsl(var(--background))] p-6 text-[hsl(var(--foreground))]">
    <section className="w-full max-w-md rounded-xl border bg-[hsl(var(--surface))] p-6 shadow-sm">
      <p className="text-sm font-semibold tracking-wide">Runory</p>
      <h1 className="mt-4 text-xl font-semibold">{t(mode === "confirm" ? "cloud.webConfirmTitle" : "cloud.webResetTitle")}</h1>
      {state === "loading" && <p className="mt-3 text-sm text-[hsl(var(--muted))]">{t("cloud.webAuthLoading")}</p>}
      {state === "ready" && mode === "reset" && <div className="mt-4 grid gap-3">
        <Input ref={password} type="password" minLength={8} autoComplete="new-password" placeholder={t("cloud.webNewPassword")} aria-label={t("cloud.webNewPassword")} />
        <Input ref={confirmation} type="password" minLength={8} autoComplete="new-password" placeholder={t("cloud.webConfirmPassword")} aria-label={t("cloud.webConfirmPassword")} />
        <Button onClick={() => void updatePassword()}><KeyRound size={15} />{t("cloud.webUpdatePassword")}</Button>
        {formError && <p className="text-sm text-red-500">{t("cloud.webPasswordError")}</p>}
      </div>}
      {state === "success" && <div className="mt-4 flex items-start gap-2 text-sm text-emerald-600"><CheckCircle2 className="mt-0.5 shrink-0" size={17} /><p>{t(mode === "confirm" ? "cloud.webConfirmSuccess" : "cloud.webResetSuccess")}</p></div>}
      {state === "error" && <div className="mt-4 flex items-start gap-2 text-sm text-red-500"><TriangleAlert className="mt-0.5 shrink-0" size={17} /><p>{t("cloud.webAuthError")}</p></div>}
      <a className="mt-5 inline-block text-sm text-[hsl(var(--primary))] underline-offset-4 hover:underline" href="/">{t("cloud.webBackHome")}</a>
    </section>
  </main>;
}
