"use server";

import { redirect } from "next/navigation";
import { revalidatePath } from "next/cache";
import { z } from "zod";
import type { ActionState } from "@/lib/action-state";
import { isLocale } from "@/lib/i18n";
import { getRunoryOrigin } from "@/lib/supabase/config";
import { createSupabaseServerClient } from "@/lib/supabase/server";

const credentialsSchema = z.object({
  locale: z.string().refine(isLocale),
  email: z.email().trim(),
  password: z.string().min(8).max(128),
});
const signupSchema = credentialsSchema.extend({ displayName: z.string().trim().min(1).max(64) });

export async function signInAction(_state: ActionState, formData: FormData): Promise<ActionState> {
  const values = credentialsSchema.safeParse(Object.fromEntries(formData));
  if (!values.success) return { code: "invalid" };
  const supabase = await createSupabaseServerClient();
  if (!supabase) return { code: "configuration" };
  const { error } = await supabase.auth.signInWithPassword({ email: values.data.email, password: values.data.password });
  if (error) return { code: "authFailed" };
  redirect(`/${values.data.locale}/account`);
}

export async function signUpAction(_state: ActionState, formData: FormData): Promise<ActionState> {
  const values = signupSchema.safeParse(Object.fromEntries(formData));
  const origin = getRunoryOrigin();
  if (!values.success) return { code: "invalid" };
  const supabase = await createSupabaseServerClient();
  if (!supabase || !origin) return { code: "configuration" };
  const { data, error } = await supabase.auth.signUp({
    email: values.data.email,
    password: values.data.password,
    options: {
      emailRedirectTo: `${origin}/auth/confirm?locale=${encodeURIComponent(values.data.locale)}`,
      data: { display_name: values.data.displayName },
    },
  });
  if (error) return { code: "authFailed" };
  if (data.session) {
    await supabase.rpc("ensure_my_profile", { target_display_name: values.data.displayName });
    await supabase.rpc("ensure_personal_workspace");
    redirect(`/${values.data.locale}/account`);
  }
  return { code: "confirmationSent" };
}

export async function requestPasswordResetAction(_state: ActionState, formData: FormData): Promise<ActionState> {
  const parsed = z.object({ locale: z.string().refine(isLocale), email: z.email().trim() }).safeParse(Object.fromEntries(formData));
  const origin = getRunoryOrigin();
  if (!parsed.success) return { code: "invalid" };
  const supabase = await createSupabaseServerClient();
  if (!supabase || !origin) return { code: "configuration" };
  const { error } = await supabase.auth.resetPasswordForEmail(parsed.data.email, {
    redirectTo: `${origin}/auth/reset?locale=${encodeURIComponent(parsed.data.locale)}`,
  });
  return error ? { code: "authFailed" } : { code: "resetSent" };
}

export async function updatePasswordAction(_state: ActionState, formData: FormData): Promise<ActionState> {
  const parsed = z.object({ locale: z.string().refine(isLocale), password: z.string().min(8).max(128) }).safeParse(Object.fromEntries(formData));
  if (!parsed.success) return { code: "invalid" };
  const supabase = await createSupabaseServerClient();
  if (!supabase) return { code: "configuration" };
  const { data: { user } } = await supabase.auth.getUser();
  if (!user) return { code: "authFailed" };
  const { error } = await supabase.auth.updateUser({ password: parsed.data.password });
  if (error) return { code: "authFailed" };
  redirect(`/${parsed.data.locale}/account`);
}

export async function updateDisplayNameAction(_state: ActionState, formData: FormData): Promise<ActionState> {
  const parsed = z.object({ locale: z.string().refine(isLocale), displayName: z.string().trim().min(1).max(64) }).safeParse(Object.fromEntries(formData));
  if (!parsed.success) return { code: "invalid" };
  const supabase = await createSupabaseServerClient();
  if (!supabase) return { code: "configuration" };
  const { data: { user } } = await supabase.auth.getUser();
  if (!user) return { code: "authFailed" };
  const { error } = await supabase.rpc("update_my_display_name", { target_display_name: parsed.data.displayName });
  if (error) return { code: "authFailed" };
  revalidatePath(`/${parsed.data.locale}/account`);
  return { code: "updated" };
}

export async function signOutAction(formData: FormData) {
  const localeValue = formData.get("locale");
  const locale = typeof localeValue === "string" && isLocale(localeValue) ? localeValue : "zh-CN";
  const supabase = await createSupabaseServerClient();
  if (supabase) await supabase.auth.signOut();
  redirect(`/${locale}`);
}
