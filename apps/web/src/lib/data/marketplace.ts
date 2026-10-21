import "server-only";

import { createHash } from "node:crypto";
import { createSupabaseAdminClient } from "@/lib/supabase/admin";
import { createSupabaseServerClient } from "@/lib/supabase/server";
import { getAdminContext } from "@/lib/data/admin";

export type MarketplacePluginDto = {
  id: string;
  publisherId: string;
  name: string;
  description: string | null;
  category: string | null;
  latestVersion: string;
  apiVersion: string;
  verified: boolean;
};

export type MarketplaceVersionDto = {
  pluginId: string;
  version: string;
  apiVersion: string;
  minRunoryVersion: string | null;
  packageSha256: string;
  downloadUrl: string;
  status: string;
  changelog: string | null;
  permissions: string[];
};

export type MarketplacePublisherKeyDto = {
  publisherId: string;
  keyId: string;
  algorithm: string;
  publicKey: string;
  status: string;
};

export type MarketplaceRevocationDto = {
  kind: string;
  targetId: string;
  version: string | null;
  reason: string | null;
};

export type PublishPluginInput = {
  pluginId: string;
  publisherId: string;
  name: string;
  description?: string | null;
  category?: string | null;
  version: string;
  apiVersion?: string;
  minRunoryVersion?: string | null;
  changelog?: string | null;
  permissions?: string[];
  verified?: boolean;
  packageBytes: Uint8Array;
};

const MAX_PACKAGE_BYTES = 50 * 1024 * 1024;
const MAX_PUBLISH_BODY_BYTES = 4 * 1024 * 1024;

type PluginRow = {
  id: string;
  publisher_id: string;
  name: string;
  description: string | null;
  category: string | null;
  latest_version: string;
  api_version: string;
  verified: boolean;
  updated_at?: string;
};

export type MarketplacePluginAdminDto = MarketplacePluginDto & {
  updatedAt: string | null;
};

function toAdminPluginDto(row: PluginRow): MarketplacePluginAdminDto {
  return {
    ...toPluginDto(row),
    updatedAt: row.updated_at ?? null,
  };
}

type VersionRow = {
  plugin_id: string;
  version: string;
  api_version: string;
  min_runory_version: string | null;
  package_sha256: string;
  storage_path: string;
  status: string;
  changelog: string | null;
  permissions: unknown;
  published_at?: string;
};

function permissionsOf(value: unknown): string[] {
  if (!Array.isArray(value)) return [];
  return value.filter((item): item is string => typeof item === "string");
}

function toPluginDto(row: PluginRow): MarketplacePluginDto {
  return {
    id: row.id,
    publisherId: row.publisher_id,
    name: row.name,
    description: row.description,
    category: row.category,
    latestVersion: row.latest_version,
    apiVersion: row.api_version,
    verified: row.verified,
  };
}

function toVersionDto(row: VersionRow): MarketplaceVersionDto {
  return {
    pluginId: row.plugin_id,
    version: row.version,
    apiVersion: row.api_version,
    minRunoryVersion: row.min_runory_version,
    packageSha256: row.package_sha256,
    downloadUrl: `/api/v1/plugins/${encodeURIComponent(row.plugin_id)}/versions/${encodeURIComponent(row.version)}/download`,
    status: row.status,
    changelog: row.changelog,
    permissions: permissionsOf(row.permissions),
  };
}

function sha256Hex(bytes: Uint8Array): string {
  return createHash("sha256").update(bytes).digest("hex");
}

function storagePathFor(pluginId: string, version: string): string {
  return `${pluginId}/${version}/plugin.runory-plugin`;
}

export async function listMarketplacePlugins(): Promise<MarketplacePluginDto[] | null> {
  const supabase = await createSupabaseServerClient();
  if (!supabase) return null;
  const { data, error } = await supabase
    .from("marketplace_plugins")
    .select("id,publisher_id,name,description,category,latest_version,api_version,verified")
    .order("name");
  if (error) return null;
  return (data as PluginRow[]).map(toPluginDto);
}

export async function getMarketplacePlugin(pluginId: string): Promise<MarketplacePluginDto | null | undefined> {
  const supabase = await createSupabaseServerClient();
  if (!supabase) return undefined;
  const { data, error } = await supabase
    .from("marketplace_plugins")
    .select("id,publisher_id,name,description,category,latest_version,api_version,verified")
    .eq("id", pluginId)
    .maybeSingle();
  if (error) return undefined;
  if (!data) return null;
  return toPluginDto(data as PluginRow);
}

export async function listMarketplaceVersions(pluginId: string): Promise<MarketplaceVersionDto[] | null> {
  const supabase = await createSupabaseServerClient();
  if (!supabase) return null;
  const { data, error } = await supabase
    .from("marketplace_plugin_versions")
    .select(
      "plugin_id,version,api_version,min_runory_version,package_sha256,storage_path,status,changelog,permissions",
    )
    .eq("plugin_id", pluginId)
    .eq("status", "published")
    .order("version");
  if (error) return null;
  return (data as VersionRow[]).map(toVersionDto);
}

export async function getMarketplaceVersion(
  pluginId: string,
  version: string,
): Promise<MarketplaceVersionDto | null | undefined> {
  const versions = await listMarketplaceVersions(pluginId);
  if (versions === null) return undefined;
  return versions.find((item) => item.version === version) ?? null;
}

export async function listPublisherKeys(publisherId: string): Promise<MarketplacePublisherKeyDto[] | null> {
  const supabase = await createSupabaseServerClient();
  if (!supabase) return null;
  const { data, error } = await supabase
    .from("marketplace_publisher_keys")
    .select("publisher_id,key_id,algorithm,public_key,status")
    .eq("publisher_id", publisherId)
    .eq("status", "active")
    .order("key_id");
  if (error) return null;
  return (data ?? []).map((row) => ({
    publisherId: row.publisher_id,
    keyId: row.key_id,
    algorithm: row.algorithm,
    publicKey: row.public_key,
    status: row.status,
  }));
}

export async function getPublisher(publisherId: string) {
  return {
    id: publisherId,
    name: publisherId,
    verified: publisherId === "runory",
  };
}

export async function listRevocations(): Promise<MarketplaceRevocationDto[] | null> {
  const supabase = await createSupabaseServerClient();
  if (!supabase) return null;
  const { data, error } = await supabase
    .from("marketplace_revocations")
    .select("kind,target_id,version,reason");
  if (error) return null;
  return (data ?? []).map((row) => ({
    kind: row.kind,
    targetId: row.target_id,
    version: row.version ? row.version : null,
    reason: row.reason,
  }));
}

export async function resolvePackageDownload(
  pluginId: string,
  version: string | null,
): Promise<{ signedUrl: string; sha256: string } | null | undefined> {
  const admin = createSupabaseAdminClient();
  if (!admin) return undefined;

  let row: VersionRow | null = null;
  if (version) {
    const { data, error } = await admin
      .from("marketplace_plugin_versions")
      .select(
        "plugin_id,version,api_version,min_runory_version,package_sha256,storage_path,status,changelog,permissions",
      )
      .eq("plugin_id", pluginId)
      .eq("version", version)
      .eq("status", "published")
      .maybeSingle();
    if (error) return undefined;
    row = (data as VersionRow | null) ?? null;
  } else {
    const { data: plugin, error: pluginError } = await admin
      .from("marketplace_plugins")
      .select("latest_version")
      .eq("id", pluginId)
      .maybeSingle();
    if (pluginError) return undefined;
    if (!plugin) return null;
    const { data, error } = await admin
      .from("marketplace_plugin_versions")
      .select(
        "plugin_id,version,api_version,min_runory_version,package_sha256,storage_path,status,changelog,permissions",
      )
      .eq("plugin_id", pluginId)
      .eq("version", plugin.latest_version)
      .eq("status", "published")
      .maybeSingle();
    if (error) return undefined;
    row = (data as VersionRow | null) ?? null;
  }

  if (!row) return null;
  const { data: signed, error: signError } = await admin.storage
    .from("plugin-packages")
    .createSignedUrl(row.storage_path, 3600);
  if (signError || !signed?.signedUrl) return undefined;
  return { signedUrl: signed.signedUrl, sha256: row.package_sha256 };
}

export async function assertMarketplaceAdminWrite(
  request: Request,
): Promise<{ ok: true; actorId: string } | { ok: false; status: number; error: string }> {
  const token = process.env.MARKETPLACE_ADMIN_TOKEN?.trim();
  const header = request.headers.get("x-runory-marketplace-token")?.trim();
  if (token && header && header === token) {
    return { ok: true, actorId: "marketplace-token" };
  }

  const context = await getAdminContext();
  if (context.status === "signedOut") return { ok: false, status: 401, error: "unauthorized" };
  if (context.status === "forbidden") return { ok: false, status: 403, error: "forbidden" };
  if (context.status !== "ok") return { ok: false, status: 503, error: "unavailable" };
  if (context.role !== "owner") return { ok: false, status: 403, error: "forbidden" };
  return { ok: true, actorId: context.actorId };
}

export async function publishMarketplacePlugin(
  input: PublishPluginInput,
  actorId: string,
): Promise<
  | { ok: true; pluginId: string; version: string; packageSha256: string; downloadUrl: string }
  | { ok: false; status: number; error: string }
> {
  const admin = createSupabaseAdminClient();
  if (!admin) return { ok: false, status: 503, error: "unavailable" };

  const pluginId = input.pluginId.trim();
  const version = input.version.trim();
  const publisherId = input.publisherId.trim();
  const name = input.name.trim();
  if (!pluginId || !version || !publisherId || !name) {
    return { ok: false, status: 400, error: "invalid" };
  }
  if (input.packageBytes.byteLength === 0 || input.packageBytes.byteLength > MAX_PACKAGE_BYTES) {
    return { ok: false, status: 413, error: "too_large" };
  }
  if (input.packageBytes.byteLength > MAX_PUBLISH_BODY_BYTES) {
    return { ok: false, status: 413, error: "too_large_for_serverless" };
  }

  const { data: existing } = await admin
    .from("marketplace_plugin_versions")
    .select("plugin_id")
    .eq("plugin_id", pluginId)
    .eq("version", version)
    .maybeSingle();
  if (existing) return { ok: false, status: 409, error: "version_immutable" };

  const packageSha256 = sha256Hex(input.packageBytes);
  const path = storagePathFor(pluginId, version);
  const { error: uploadError } = await admin.storage.from("plugin-packages").upload(path, input.packageBytes, {
    contentType: "application/octet-stream",
    upsert: false,
  });
  if (uploadError) return { ok: false, status: 500, error: "storage" };

  const apiVersion = (input.apiVersion ?? "1").trim() || "1";
  const permissions = input.permissions ?? [];

  const { error: versionError } = await admin.from("marketplace_plugin_versions").insert({
    plugin_id: pluginId,
    version,
    api_version: apiVersion,
    min_runory_version: input.minRunoryVersion ?? null,
    package_sha256: packageSha256,
    storage_path: path,
    status: "published",
    changelog: input.changelog ?? null,
    permissions,
  });
  if (versionError) {
    await admin.storage.from("plugin-packages").remove([path]);
    return { ok: false, status: 500, error: "db" };
  }

  const { error: pluginError } = await admin.from("marketplace_plugins").upsert(
    {
      id: pluginId,
      publisher_id: publisherId,
      name,
      description: input.description ?? null,
      category: input.category ?? null,
      latest_version: version,
      api_version: apiVersion,
      verified: Boolean(input.verified),
      updated_at: new Date().toISOString(),
    },
    { onConflict: "id" },
  );
  if (pluginError) return { ok: false, status: 500, error: "db" };

  if (actorId !== "marketplace-token") {
    await admin.from("platform_admin_audit").insert({
      actor_id: actorId,
      action: "marketplace.publish",
    });
  }

  return {
    ok: true,
    pluginId,
    version,
    packageSha256,
    downloadUrl: `/api/v1/plugins/${encodeURIComponent(pluginId)}/versions/${encodeURIComponent(version)}/download`,
  };
}

export async function upsertPublisherKey(input: {
  publisherId: string;
  keyId: string;
  publicKey: string;
  algorithm?: string;
  status?: string;
}): Promise<{ ok: true } | { ok: false; status: number; error: string }> {
  const admin = createSupabaseAdminClient();
  if (!admin) return { ok: false, status: 503, error: "unavailable" };
  const publisherId = input.publisherId.trim();
  const keyId = input.keyId.trim();
  const publicKey = input.publicKey.trim();
  if (!publisherId || !keyId || !publicKey) return { ok: false, status: 400, error: "invalid" };
  const { error } = await admin.from("marketplace_publisher_keys").upsert(
    {
      publisher_id: publisherId,
      key_id: keyId,
      algorithm: input.algorithm?.trim() || "ed25519",
      public_key: publicKey,
      status: input.status?.trim() || "active",
    },
    { onConflict: "publisher_id,key_id" },
  );
  if (error) return { ok: false, status: 500, error: "db" };
  return { ok: true };
}

export async function insertRevocation(input: {
  kind: string;
  targetId: string;
  version?: string | null;
  reason?: string | null;
}): Promise<{ ok: true } | { ok: false; status: number; error: string }> {
  const admin = createSupabaseAdminClient();
  if (!admin) return { ok: false, status: 503, error: "unavailable" };
  const { error } = await admin.from("marketplace_revocations").upsert(
    {
      kind: input.kind,
      target_id: input.targetId,
      version: input.version ?? "",
      reason: input.reason ?? null,
    },
    { onConflict: "kind,target_id,version" },
  );
  if (error) return { ok: false, status: 500, error: "db" };
  return { ok: true };
}

export async function listAdminMarketplacePlugins() {
  const context = await getAdminContext();
  if (context.status !== "ok") return { status: context.status } as const;
  const { data, error } = await context.admin
    .from("marketplace_plugins")
    .select("id,publisher_id,name,description,category,latest_version,api_version,verified,updated_at")
    .order("name");
  if (error) return { status: "unavailable" } as const;
  await context.admin.from("platform_admin_audit").insert({
    actor_id: context.actorId,
    action: "marketplace.list",
  });
  return {
    status: "ok" as const,
    role: context.role,
    plugins: (data as PluginRow[]).map(toAdminPluginDto),
  };
}

export async function getAdminMarketplacePlugin(pluginId: string) {
  const context = await getAdminContext();
  if (context.status !== "ok") return { status: context.status } as const;
  const { data: plugin, error: pluginError } = await context.admin
    .from("marketplace_plugins")
    .select("id,publisher_id,name,description,category,latest_version,api_version,verified,updated_at")
    .eq("id", pluginId)
    .maybeSingle();
  if (pluginError) return { status: "unavailable" } as const;
  if (!plugin) return { status: "missing" } as const;

  const { data: versions, error: versionError } = await context.admin
    .from("marketplace_plugin_versions")
    .select(
      "plugin_id,version,api_version,min_runory_version,package_sha256,storage_path,status,changelog,permissions,published_at",
    )
    .eq("plugin_id", pluginId)
    .order("published_at", { ascending: false });
  if (versionError) return { status: "unavailable" } as const;

  await context.admin.from("platform_admin_audit").insert({
    actor_id: context.actorId,
    action: "marketplace.read",
  });

  return {
    status: "ok" as const,
    role: context.role,
    plugin: toAdminPluginDto(plugin as PluginRow),
    versions: ((versions ?? []) as Array<VersionRow & { published_at?: string }>).map((row) => ({
      ...toVersionDto(row),
      publishedAt: row.published_at ?? null,
      packageSha256: row.package_sha256,
    })),
  };
}
