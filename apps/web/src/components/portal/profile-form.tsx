"use client";

import { useActionState } from "react";
import { LoaderCircle, Save } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { initialActionState } from "@/lib/action-state";
import { updateDisplayNameAction } from "@/lib/auth-actions";
import { getDictionary, type Locale } from "@/lib/i18n";

export function ProfileForm({ locale, displayName, email }: { locale: Locale; displayName: string; email: string }) {
  const [state, formAction, pending] = useActionState(updateDisplayNameAction, initialActionState);
  const t = getDictionary(locale);
  return (
    <form action={formAction} className="profile-form">
      <input type="hidden" name="locale" value={locale} />
      <label className="field-label">{t.portal.displayName}<Input name="displayName" defaultValue={displayName} required minLength={1} maxLength={64} /></label>
      <label className="field-label">{t.portal.email}<Input value={email} disabled /></label>
      {state.code !== "idle" && <p className={`form-message ${state.code === "updated" ? "success" : "error"}`} role="status">{t.auth[state.code]}</p>}
      <Button type="submit" disabled={pending}>{pending ? <LoaderCircle className="animate-spin" size={16} /> : <Save size={16} />}{t.portal.save}</Button>
    </form>
  );
}
