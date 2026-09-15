import "server-only";

import { getAdminContext } from "@/lib/data/admin";
import {
  isHttpsUrl,
  mapReleaseToDownloads,
  normalizeVersion,
  releasePlatforms,
  type ReleaseAssetInput,
  type ReleasePlatform,
  type ReleaseRecord,
  type ReleaseStatus,
} from "@/lib/releases";
import { createSupabaseAdminClient } from "@/lib/supabase/admin";
import { createSupabaseServerClient } from "@/lib/supabase/server";
import type { LatestReleaseDownloads } from "@/lib/github-releases";

type AdminClient = NonNullable<ReturnType<typeof createSupabaseAdminClient>>;

type ReleaseRow = {
  id: string;
  version: string;
  status: ReleaseStatus;
  is_latest: boolean;
  notes_zh: string | null;
  notes_en: string | null;
  release_page_url: string | null;
  published_at: string | null;
  created_at: string;
  updated_at: string;
};

type AssetRow = {
  release_id: string;
  platform: ReleasePlatform;
  format: string;
  download_url: string;
};

function toRecord(row: ReleaseRow, assets: AssetRow[]): ReleaseRecord {
  return {
    id: row.id,
    version: row.version,
    status: row.status,
    isLatest: row.is_latest,
    notesZh: row.notes_zh,
    notesEn: row.notes_en,
    releasePageUrl: row.release_page_url,
    publishedAt: row.published_at,
    createdAt: row.created_at,
    updatedAt: row.updated_at,
    assets: assets
      .filter(asset => asset.release_id === row.id)
      .map(asset => ({
        platform: asset.platform,
        format: asset.format,
        downloadUrl: asset.download_url,
      })),
  };
}

function normalizeAssets(assets: ReleaseAssetInput[]): ReleaseAssetInput[] | null {
  const byPlatform = new Map<ReleasePlatform, ReleaseAssetInput>();
  for (const asset of assets) {
    if (!releasePlatforms.includes(asset.platform)) return null;
    const format = asset.format.trim();
    const downloadUrl = asset.downloadUrl.trim();
    if (!format || format.length > 32) return null;
    if (!isHttpsUrl(downloadUrl) || downloadUrl.length > 2048) return null;
    byPlatform.set(asset.platform, { platform: asset.platform, format, downloadUrl });
  }
  return [...byPlatform.values()];
}

async function replaceAssets(admin: AdminClient, releaseId: string, assets: ReleaseAssetInput[]) {
  const { error: deleteError } = await admin.from("app_release_assets").delete().eq("release_id", releaseId);
  if (deleteError) return false;
  if (!assets.length) return true;
  const { error: insertError } = await admin.from("app_release_assets").insert(
    assets.map(asset => ({
      release_id: releaseId,
      platform: asset.platform,
      format: asset.format,
      download_url: asset.downloadUrl,
    })),
  );
  return !insertError;
}

export async function listAdminReleases() {
  const context = await getAdminContext();
  if (context.status !== "ok") return { status: context.status } as const;
  const { data, error } = await context.admin
    .from("app_releases")
    .select("id,version,status,is_latest,notes_zh,notes_en,release_page_url,published_at,created_at,updated_at")
    .order("created_at", { ascending: false });
  if (error) return { status: "unavailable" } as const;
  const ids = (data ?? []).map(row => row.id);
  const { data: assets, error: assetsError } = ids.length
    ? await context.admin.from("app_release_assets").select("release_id,platform,format,download_url").in("release_id", ids)
    : { data: [], error: null };
  if (assetsError) return { status: "unavailable" } as const;
  await context.admin.from("platform_admin_audit").insert({ actor_id: context.actorId, action: "releases.list" });
  return {
    status: "ok" as const,
    role: context.role,
    releases: (data ?? []).map(row => toRecord(row as ReleaseRow, (assets ?? []) as AssetRow[])),
  };
}

export async function getAdminRelease(releaseId: string) {
  const context = await getAdminContext();
  if (context.status !== "ok") return { status: context.status } as const;
  const { data, error } = await context.admin
    .from("app_releases")
    .select("id,version,status,is_latest,notes_zh,notes_en,release_page_url,published_at,created_at,updated_at")
    .eq("id", releaseId)
    .maybeSingle();
  if (error) return { status: "unavailable" } as const;
  if (!data) return { status: "missing" } as const;
  const { data: assets, error: assetsError } = await context.admin
    .from("app_release_assets")
    .select("release_id,platform,format,download_url")
    .eq("release_id", releaseId);
  if (assetsError) return { status: "unavailable" } as const;
  await context.admin.from("platform_admin_audit").insert({
    actor_id: context.actorId,
    action: "releases.read",
  });
  return {
    status: "ok" as const,
    role: context.role,
    release: toRecord(data as ReleaseRow, (assets ?? []) as AssetRow[]),
  };
}

export type ReleaseWriteInput = {
  version: string;
  notesZh?: string | null;
  notesEn?: string | null;
  releasePageUrl?: string | null;
  assets: ReleaseAssetInput[];
};

export async function createRelease(input: ReleaseWriteInput) {
  try {
    const context = await getAdminContext();
    if (context.status !== "ok") return { status: context.status } as const;
    if (context.role !== "owner") return { status: "forbidden" } as const;

    const version = normalizeVersion(input.version);
    if (!version || version.length > 32 || !/^[A-Za-z0-9][A-Za-z0-9._+-]*$/.test(version)) {
      return { status: "invalid" } as const;
    }
    const releasePageUrl = input.releasePageUrl?.trim() || null;
    if (releasePageUrl && !isHttpsUrl(releasePageUrl)) return { status: "invalid" } as const;
    const assets = normalizeAssets(input.assets);
    if (!assets) return { status: "invalid" } as const;
    const notesZh = input.notesZh?.trim() || null;
    const notesEn = input.notesEn?.trim() || null;
    if ((notesZh && notesZh.length > 8000) || (notesEn && notesEn.length > 8000)) return { status: "invalid" } as const;

    const { data, error } = await context.admin
      .from("app_releases")
      .insert({
        version,
        status: "draft",
        is_latest: false,
        notes_zh: notesZh,
        notes_en: notesEn,
        release_page_url: releasePageUrl,
        created_by: context.actorId,
        updated_by: context.actorId,
      })
      .select("id")
      .single();
    if (error || !data) return { status: error?.code === "23505" ? "conflict" : "unavailable" } as const;
    if (!(await replaceAssets(context.admin, data.id, assets))) {
      await context.admin.from("app_releases").delete().eq("id", data.id);
      return { status: "unavailable" } as const;
    }
    await context.admin.from("platform_admin_audit").insert({ actor_id: context.actorId, action: "releases.create" });
    return { status: "ok" as const, id: data.id as string };
  } catch {
    return { status: "unavailable" } as const;
  }
}

export async function updateRelease(releaseId: string, input: ReleaseWriteInput) {
  const context = await getAdminContext();
  if (context.status !== "ok") return { status: context.status } as const;
  if (context.role !== "owner") return { status: "forbidden" } as const;

  const version = normalizeVersion(input.version);
  if (!version || version.length > 32 || !/^[A-Za-z0-9][A-Za-z0-9._+-]*$/.test(version)) {
    return { status: "invalid" } as const;
  }
  const releasePageUrl = input.releasePageUrl?.trim() || null;
  if (releasePageUrl && !isHttpsUrl(releasePageUrl)) return { status: "invalid" } as const;
  const assets = normalizeAssets(input.assets);
  if (!assets) return { status: "invalid" } as const;
  const notesZh = input.notesZh?.trim() || null;
  const notesEn = input.notesEn?.trim() || null;
  if ((notesZh && notesZh.length > 8000) || (notesEn && notesEn.length > 8000)) return { status: "invalid" } as const;

  const { data: existing, error: existingError } = await context.admin
    .from("app_releases")
    .select("id")
    .eq("id", releaseId)
    .maybeSingle();
  if (existingError) return { status: "unavailable" } as const;
  if (!existing) return { status: "missing" } as const;

  const { error } = await context.admin
    .from("app_releases")
    .update({
      version,
      notes_zh: notesZh,
      notes_en: notesEn,
      release_page_url: releasePageUrl,
      updated_by: context.actorId,
      updated_at: new Date().toISOString(),
    })
    .eq("id", releaseId);
  if (error) return { status: error.code === "23505" ? "conflict" : "unavailable" } as const;
  if (!(await replaceAssets(context.admin, releaseId, assets))) return { status: "unavailable" } as const;
  await context.admin.from("platform_admin_audit").insert({ actor_id: context.actorId, action: "releases.update" });
  return { status: "ok" } as const;
}

export async function setReleaseStatus(releaseId: string, status: ReleaseStatus) {
  const context = await getAdminContext();
  if (context.status !== "ok") return { status: context.status } as const;
  if (context.role !== "owner") return { status: "forbidden" } as const;

  const { data: existing, error: existingError } = await context.admin
    .from("app_releases")
    .select("id,status,is_latest")
    .eq("id", releaseId)
    .maybeSingle();
  if (existingError) return { status: "unavailable" } as const;
  if (!existing) return { status: "missing" } as const;

  const patch: Record<string, unknown> = {
    status,
    updated_by: context.actorId,
    updated_at: new Date().toISOString(),
  };
  if (status === "published" && existing.status !== "published") {
    patch.published_at = new Date().toISOString();
  }
  if (status !== "published") {
    patch.is_latest = false;
  }

  const { error } = await context.admin.from("app_releases").update(patch).eq("id", releaseId);
  if (error) return { status: "unavailable" } as const;
  await context.admin.from("platform_admin_audit").insert({
    actor_id: context.actorId,
    action: status === "published" ? "releases.publish" : status === "archived" ? "releases.archive" : "releases.draft",
  });
  return { status: "ok" } as const;
}

export async function setLatestRelease(releaseId: string) {
  const context = await getAdminContext();
  if (context.status !== "ok") return { status: context.status } as const;
  if (context.role !== "owner") return { status: "forbidden" } as const;

  const { data: existing, error: existingError } = await context.admin
    .from("app_releases")
    .select("id,status")
    .eq("id", releaseId)
    .maybeSingle();
  if (existingError) return { status: "unavailable" } as const;
  if (!existing) return { status: "missing" } as const;
  if (existing.status !== "published") return { status: "invalid" } as const;

  const { error: clearError } = await context.admin
    .from("app_releases")
    .update({ is_latest: false, updated_by: context.actorId, updated_at: new Date().toISOString() })
    .eq("is_latest", true);
  if (clearError) return { status: "unavailable" } as const;

  const { error } = await context.admin
    .from("app_releases")
    .update({
      is_latest: true,
      updated_by: context.actorId,
      updated_at: new Date().toISOString(),
    })
    .eq("id", releaseId)
    .eq("status", "published");
  if (error) return { status: "unavailable" } as const;
  await context.admin.from("platform_admin_audit").insert({ actor_id: context.actorId, action: "releases.set_latest" });
  return { status: "ok" } as const;
}

export async function getPublishedLatestDownloads(): Promise<LatestReleaseDownloads | null> {
  const supabase = await createSupabaseServerClient();
  if (!supabase) return null;

  const { data: release, error } = await supabase
    .from("app_releases")
    .select("id,version,release_page_url")
    .eq("status", "published")
    .eq("is_latest", true)
    .maybeSingle();
  if (error || !release) return null;

  const { data: assets, error: assetsError } = await supabase
    .from("app_release_assets")
    .select("platform,format,download_url")
    .eq("release_id", release.id);
  if (assetsError) return null;

  return mapReleaseToDownloads({
    version: release.version,
    releasePageUrl: release.release_page_url,
    assets: (assets ?? []).map(asset => ({
      platform: asset.platform as ReleasePlatform,
      format: asset.format,
      downloadUrl: asset.download_url,
    })),
  });
}
