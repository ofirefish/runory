import { NextResponse } from "next/server";
import { getMarketplaceVersion } from "@/lib/data/marketplace";

type Params = { params: Promise<{ pluginId: string; version: string }> };

export async function GET(_request: Request, { params }: Params) {
  const { pluginId, version } = await params;
  const row = await getMarketplaceVersion(
    decodeURIComponent(pluginId),
    decodeURIComponent(version),
  );
  if (row === undefined) {
    return NextResponse.json({ error: "unavailable" }, { status: 503 });
  }
  if (row === null) {
    return NextResponse.json(null, { status: 404 });
  }
  return NextResponse.json(row, {
    headers: { "Access-Control-Allow-Origin": "*" },
  });
}
