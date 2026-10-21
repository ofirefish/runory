import { NextResponse } from "next/server";
import { listMarketplaceVersions } from "@/lib/data/marketplace";

type Params = { params: Promise<{ pluginId: string }> };

export async function GET(_request: Request, { params }: Params) {
  const { pluginId } = await params;
  const versions = await listMarketplaceVersions(decodeURIComponent(pluginId));
  if (versions === null) {
    return NextResponse.json({ error: "unavailable" }, { status: 503 });
  }
  return NextResponse.json(versions, {
    headers: { "Access-Control-Allow-Origin": "*" },
  });
}
