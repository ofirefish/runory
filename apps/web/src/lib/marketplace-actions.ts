"use server";

import { revalidatePath } from "next/cache";
import { publishMarketplacePlugin } from "@/lib/data/marketplace";
import { getAdminContext } from "@/lib/data/admin";

export type MarketplacePublishState = {
  code:
    | "idle"
    | "created"
    | "package_required"
    | "too_large_for_serverless"
    | "version_immutable"
    | "invalid"
    | "forbidden"
    | "unavailable"
    | "storage"
    | "db"
    | "error";
  pluginId?: string;
  version?: string;
};

export const initialMarketplacePublishState: MarketplacePublishState = { code: "idle" };

export async function publishMarketplaceAction(
  _state: MarketplacePublishState,
  formData: FormData,
): Promise<MarketplacePublishState> {
  const context = await getAdminContext();
  if (context.status === "forbidden" || context.status === "signedOut") {
    return { code: "forbidden" };
  }
  if (context.status !== "ok") return { code: "unavailable" };
  if (context.role !== "owner") return { code: "forbidden" };

  const file = formData.get("package");
  if (!(file instanceof File) || file.size === 0) {
    return { code: "package_required" };
  }
  if (file.size > 4 * 1024 * 1024) {
    return { code: "too_large_for_serverless" };
  }

  const buffer = new Uint8Array(await file.arrayBuffer());
  const result = await publishMarketplacePlugin(
    {
      pluginId: String(formData.get("pluginId") ?? ""),
      publisherId: String(formData.get("publisherId") ?? "runory"),
      name: String(formData.get("name") ?? ""),
      description: String(formData.get("description") ?? "") || null,
      category: String(formData.get("category") ?? "") || null,
      version: String(formData.get("version") ?? ""),
      apiVersion: String(formData.get("apiVersion") ?? "1") || "1",
      minRunoryVersion: String(formData.get("minRunoryVersion") ?? "") || null,
      changelog: String(formData.get("changelog") ?? "") || null,
      verified: formData.get("verified") === "on",
      packageBytes: buffer,
    },
    context.actorId,
  );

  if (!result.ok) {
    if (
      result.error === "version_immutable" ||
      result.error === "invalid" ||
      result.error === "too_large_for_serverless" ||
      result.error === "forbidden" ||
      result.error === "unavailable" ||
      result.error === "storage" ||
      result.error === "db"
    ) {
      return { code: result.error };
    }
    return { code: "error" };
  }

  revalidatePath("/zh-CN/admin/store");
  revalidatePath("/en-US/admin/store");
  revalidatePath(`/zh-CN/admin/store/${result.pluginId}`);
  revalidatePath(`/en-US/admin/store/${result.pluginId}`);
  return { code: "created", pluginId: result.pluginId, version: result.version };
}
