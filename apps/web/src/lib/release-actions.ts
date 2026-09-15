"use server";

import { revalidatePath } from "next/cache";
import { z } from "zod";
import {
  createRelease,
  setLatestRelease,
  setReleaseStatus,
  updateRelease,
} from "@/lib/data/releases";
import { isLocale } from "@/lib/i18n";
import type { ReleaseActionState } from "@/lib/release-action-state";
import { isHttpsUrl, releasePlatforms } from "@/lib/releases";

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
  notesZh: z.string().max(8000).optional().default(""),
  notesEn: z.string().max(8000).optional().default(""),
  releasePageUrl: z.string().trim().max(2048).optional().default(""),
  windowsUrl: z.string().trim().optional().default(""),
  windowsFormat: z.string().trim().optional().default(".msi"),
  macosAppleUrl: z.string().trim().optional().default(""),
  macosAppleFormat: z.string().trim().optional().default(".dmg"),
  macosIntelUrl: z.string().trim().optional().default(""),
  macosIntelFormat: z.string().trim().optional().default(".dmg"),
  linuxUrl: z.string().trim().optional().default(""),
  linuxFormat: z.string().trim().optional().default(".AppImage"),
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

function parseWriteForm(formData: FormData) {
  const parsed = writeSchema.safeParse(Object.fromEntries(formData));
  if (!parsed.success) return { status: "invalid" as const };
  const releasePageUrl = parsed.data.releasePageUrl;
  if (releasePageUrl && !isHttpsUrl(releasePageUrl)) return { status: "invalid" as const };
  const assets = collectAssets(parsed.data);
  if (!assets) return { status: "invalid" as const };
  return {
    status: "ok" as const,
    data: {
      locale: parsed.data.locale,
      version: parsed.data.version,
      notesZh: parsed.data.notesZh || null,
      notesEn: parsed.data.notesEn || null,
      releasePageUrl: releasePageUrl || null,
      assets,
    },
  };
}

export async function createReleaseAction(
  _state: ReleaseActionState,
  formData: FormData,
): Promise<ReleaseActionState> {
  try {
    const parsed = parseWriteForm(formData);
    if (parsed.status !== "ok") return { code: "invalid" };
    const result = await createRelease(parsed.data);
    if (result.status !== "ok") return { code: mapWriteStatus(result.status) };
    revalidateReleasePaths(parsed.data.locale);
    return { code: "created", id: result.id };
  } catch {
    return { code: "unavailable" };
  }
}

export async function updateReleaseAction(
  _state: ReleaseActionState,
  formData: FormData,
): Promise<ReleaseActionState> {
  try {
    const releaseId = String(formData.get("releaseId") ?? "");
    const parsed = parseWriteForm(formData);
    if (!releaseId || parsed.status !== "ok") return { code: "invalid" };
    const result = await updateRelease(releaseId, parsed.data);
    if (result.status !== "ok") return { code: mapWriteStatus(result.status) };
    revalidateReleasePaths(parsed.data.locale);
    return { code: "updated" };
  } catch {
    return { code: "unavailable" };
  }
}

export async function setReleaseStatusAction(
  _state: ReleaseActionState,
  formData: FormData,
): Promise<ReleaseActionState> {
  try {
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
  } catch {
    return { code: "unavailable" };
  }
}

export async function setLatestReleaseAction(
  _state: ReleaseActionState,
  formData: FormData,
): Promise<ReleaseActionState> {
  try {
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
  } catch {
    return { code: "unavailable" };
  }
}
