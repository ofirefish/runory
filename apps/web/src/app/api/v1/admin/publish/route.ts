import { NextResponse } from "next/server";
import {
  assertMarketplaceAdminWrite,
  publishMarketplacePlugin,
} from "@/lib/data/marketplace";

type PublishBody = {
  pluginId?: string;
  publisherId?: string;
  name?: string;
  description?: string | null;
  category?: string | null;
  version?: string;
  apiVersion?: string;
  minRunoryVersion?: string | null;
  changelog?: string | null;
  permissions?: string[];
  verified?: boolean;
  packageBase64?: string;
};

export async function OPTIONS() {
  return new NextResponse(null, {
    status: 204,
    headers: {
      "Access-Control-Allow-Origin": "*",
      "Access-Control-Allow-Methods": "POST, OPTIONS",
      "Access-Control-Allow-Headers": "Content-Type, x-runory-marketplace-token",
    },
  });
}

export async function POST(request: Request) {
  const auth = await assertMarketplaceAdminWrite(request);
  if (!auth.ok) {
    return NextResponse.json({ error: auth.error }, { status: auth.status });
  }

  let body: PublishBody;
  try {
    body = (await request.json()) as PublishBody;
  } catch {
    return NextResponse.json({ error: "invalid" }, { status: 400 });
  }

  const packageBase64 = body.packageBase64?.trim();
  if (!packageBase64) {
    return NextResponse.json({ error: "invalid" }, { status: 400 });
  }

  let packageBytes: Uint8Array;
  try {
    packageBytes = Uint8Array.from(Buffer.from(packageBase64, "base64"));
  } catch {
    return NextResponse.json({ error: "invalid" }, { status: 400 });
  }

  const result = await publishMarketplacePlugin(
    {
      pluginId: body.pluginId ?? "",
      publisherId: body.publisherId ?? "",
      name: body.name ?? "",
      description: body.description,
      category: body.category,
      version: body.version ?? "",
      apiVersion: body.apiVersion,
      minRunoryVersion: body.minRunoryVersion,
      changelog: body.changelog,
      permissions: body.permissions,
      verified: body.verified,
      packageBytes,
    },
    auth.actorId,
  );

  if (!result.ok) {
    return NextResponse.json({ error: result.error }, { status: result.status });
  }

  return NextResponse.json({
    ok: true,
    pluginId: result.pluginId,
    version: result.version,
    packageSha256: result.packageSha256,
    downloadUrl: result.downloadUrl,
  });
}
