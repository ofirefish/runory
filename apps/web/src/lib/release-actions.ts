"use server";

import { revalidatePath } from "next/cache";
import { redirect } from "next/navigation";
import { z } from "zod";
import {
  createRelease,
  setLatestRelease,
  setReleaseStatus,
  updateRelease,
} from "@/lib/data/releases";
import { isLocale } from "@/lib/i18n";
import { isHttpsUrl, releasePlatforms } from "@/lib/releases";

export type ReleaseActionState = {
  code: "idle" | "invalid" | "forbidden" | "configuration" | "unavailable" | "conflict" | "updated" | "missing";
};

export const initialReleaseActionState: ReleaseActionState = { code: "idle" };

const assetSchema = z.object({
  platform: z.enum(releasePlatforms),
  format: z.string().trim().min(1).max(32),
  downloadUrl: z.string().trim().min(12).max(2048).refine(isHttpsUrl),
});

const writeSchema = z.object({
  locale: z.string().refine(isLocale),
  version: z
    .string()
    .trim()
    .min(1)
    .max(32)
    .regex(/^[A-Za-z0-9][A-Za-z0-9._+-]*$/),
  notesZh: z.string().max(8000).optional(),
  notesEn: z.string().max(8000).optional(),
  releasePageUrl: z
    .string()
    .trim()
    .max(2048)
    .optional()
    .transform(value => (value ? value : undefined))
    .refine(value => value === undefined || isHttpsUrl(value)),
  windowsUrl: z.string().trim(),
  windowsFormat: z.string().trim(),
  macosAppleUrl: z.string().trim(),
  macosAppleFormat: z.string().trim(),
  macosIntelUrl: z.string().trim(),
  macosIntelFormat: z.string().trim(),
  linuxUrl: z.string().trim(),
  linuxFormat: z.string().trim(),
});

function collectAssets(values: z.infer<typeof writeSchema>) {
  const candidates = [
    { platform: "windows" as const, format: values.windowsFormat, downloadUrl: values.windowsUrl },
    { platform: "macos_apple" as const, format: values.macosAppleFormat, downloadUrl: values.macosAppleUrl },
    { platform: "macos_intel" as const, format: values.macosIntelFormat, downloadUrl: values.macosIntelUrl },
    { platform: "linux" as const, format: values.linuxFormat, downloadUrl: values.linuxUrl },
  ];
  const assets = [];
  for (const candidate of candidates) {
    if (!candidate.downloadUrl) continue;
    const parsed = assetSchema.safeParse(candidate);
    if (!parsed.success) return null;
    assets.push(parsed.data);
  }
  return assets;
}

function mapWriteStatus(status: string): ReleaseActionState["code"] {
  if (status === "ok") return "updated";
  if (status === "signedOut" || status === "forbidden") return "forbidden";
  if (status === "configuration") return "configuration";
  if (status === "conflict") return "conflict";
  if (status === "missing") return "missing";
  if (status === "invalid") return "invalid";
  return "unavailable";
}

function revalidateReleasePaths(locale: string) {
  revalidatePath(`/${locale}/admin/releases`);
  revalidatePath(`/${locale}`);
  revalidatePath(`/${locale}/download`);
  revalidatePath(`/${locale === "zh-CN" ? "en-US" : "zh-CN"}`);
  revalidatePath(`/${locale === "zh-CN" ? "en-US" : "zh-CN"}/download`);
}

export async function createReleaseAction(
  _state: ReleaseActionState,
  formData: FormData,
): Promise<ReleaseActionState> {
  const parsed = writeSchema.safeParse(Object.fromEntries(formData));
  if (!parsed.success) return { code: "invalid" };
  const assets = collectAssets(parsed.data);
  if (!assets) return { code: "invalid" };
  const result = await createRelease({
    version: parsed.data.version,
    notesZh: parsed.data.notesZh ?? null,
    notesEn: parsed.data.notesEn ?? null,
    releasePageUrl: parsed.data.releasePageUrl ?? null,
    assets,
  });
  if (result.status !== "ok") return { code: mapWriteStatus(result.status) };
  revalidateReleasePaths(parsed.data.locale);
  redirect(`/${parsed.data.locale}/admin/releases/${result.id}`);
}

export async function updateReleaseAction(
  _state: ReleaseActionState,
  formData: FormData,
): Promise<ReleaseActionState> {
  const releaseId = String(formData.get("releaseId") ?? "");
  const parsed = writeSchema.safeParse(Object.fromEntries(formData));
  if (!releaseId || !parsed.success) return { code: "invalid" };
  const assets = collectAssets(parsed.data);
  if (!assets) return { code: "invalid" };
  const result = await updateRelease(releaseId, {
    version: parsed.data.version,
    notesZh: parsed.data.notesZh ?? null,
    notesEn: parsed.data.notesEn ?? null,
    releasePageUrl: parsed.data.releasePageUrl ?? null,
    assets,
  });
  if (result.status !== "ok") return { code: mapWriteStatus(result.status) };
  revalidateReleasePaths(parsed.data.locale);
  return { code: "updated" };
}

export async function setReleaseStatusAction(
  _state: ReleaseActionState,
  formData: FormData,
): Promise<ReleaseActionState> {
  const parsed = z
    .object({
      locale: z.string().refine(isLocale),
      releaseId: z.string().uuid(),
      status: z.enum(["draft", "published", "archived"]),
    })
    .safeParse(Object.fromEntries(formData));
  if (!parsed.success) return { code: "invalid" };
  const result = await setReleaseStatus(parsed.data.releaseId, parsed.data.status);
  if (result.status !== "ok") return { code: mapWriteStatus(result.status) };
  revalidateReleasePaths(parsed.data.locale);
  return { code: "updated" };
}

export async function setLatestReleaseAction(
  _state: ReleaseActionState,
  formData: FormData,
): Promise<ReleaseActionState> {
  const parsed = z
    .object({
      locale: z.string().refine(isLocale),
      releaseId: z.string().uuid(),
    })
    .safeParse(Object.fromEntries(formData));
  if (!parsed.success) return { code: "invalid" };
  const result = await setLatestRelease(parsed.data.releaseId);
  if (result.status !== "ok") return { code: mapWriteStatus(result.status) };
  revalidateReleasePaths(parsed.data.locale);
  return { code: "updated" };
}
