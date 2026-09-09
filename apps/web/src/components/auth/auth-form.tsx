"use client";

import Link from "next/link";
import { useActionState } from "react";
import { ArrowRight, LoaderCircle } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { initialActionState } from "@/lib/action-state";
import { type Locale, getDictionary } from "@/lib/i18n";
import { requestPasswordResetAction, signInAction, signUpAction, updatePasswordAction } from "@/lib/auth-actions";

type Mode = "sign-in" | "sign-up" | "forgot" | "update";

export function AuthForm({ locale, mode, initialError = false }: { locale: Locale; mode: Mode; initialError?: boolean }) {
  const t = getDictionary(locale).auth;
  const action = mode === "sign-in" ? signInAction : mode === "sign-up" ? signUpAction : mode === "forgot" ? requestPasswordResetAction : updatePasswordAction;
  const [state, formAction, pending] = useActionState(action, initialActionState);
  const titles = { "sign-in": [t.signInTitle, t.signInDescription], "sign-up": [t.signUpTitle, t.signUpDescription], forgot: [t.forgotTitle, t.forgotDescription], update: [t.updateTitle, t.updateDescription] } as const;
  const submit = mode === "sign-in" ? t.signIn : mode === "sign-up" ? t.signUp : mode === "forgot" ? t.sendReset : t.updatePassword;
  const message = initialError && state.code === "idle" ? t.confirmFailed : state.code === "idle" ? null : t[state.code];
  const success = state.code === "confirmationSent" || state.code === "resetSent" || state.code === "updated";

  return (
    <div className="auth-panel">
      <div><p className="eyebrow">RUNORY CLOUD</p><h1>{titles[mode][0]}</h1><p className="auth-description">{titles[mode][1]}</p></div>
      <form action={formAction} className="mt-8 grid gap-4">
        <input type="hidden" name="locale" value={locale} />
        {mode === "sign-up" && <label className="field-label">{t.displayName}<Input name="displayName" autoComplete="name" required maxLength={64} /></label>}
        {mode !== "update" && <label className="field-label">{t.email}<Input name="email" type="email" autoComplete="email" required /></label>}
        {(mode === "sign-in" || mode === "sign-up" || mode === "update") && <label className="field-label">{mode === "update" ? t.newPassword : t.password}<Input name="password" type="password" autoComplete={mode === "sign-in" ? "current-password" : "new-password"} required minLength={8} maxLength={128} /><span>{t.passwordHint}</span></label>}
        {message && <p className={success ? "form-message success" : "form-message error"} role="status">{message}</p>}
        <Button type="submit" disabled={pending} className="mt-1 w-full">{pending ? <LoaderCircle className="animate-spin" size={16} /> : <ArrowRight size={16} />}{submit}</Button>
      </form>
      <div className="mt-6 flex flex-wrap items-center justify-between gap-3 text-sm text-muted-foreground">
        {mode === "sign-in" && <><Link href={`/${locale}/auth/forgot-password`} className="hover:text-foreground">{t.forgot}</Link><span>{t.noAccount} <Link href={`/${locale}/auth/sign-up`} className="text-foreground hover:text-cyan-300">{t.signUp}</Link></span></>}
        {mode === "sign-up" && <span>{t.haveAccount} <Link href={`/${locale}/auth/sign-in`} className="text-foreground hover:text-cyan-300">{t.signIn}</Link></span>}
        {(mode === "forgot" || mode === "update") && <Link href={`/${locale}/auth/sign-in`} className="hover:text-foreground">{t.backToSignIn}</Link>}
      </div>
    </div>
  );
}
