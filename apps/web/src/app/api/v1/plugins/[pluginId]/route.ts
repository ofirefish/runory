import { NextResponse } from "next/server";
import { getMarketplacePlugin } from "@/lib/data/marketplace";

type Params = { params: Promise<{ pluginId: string }> };

export async function GET(_request: Request, { params }: Params) {
  const { pluginId } = await params;
  const plugin = await getMarketplacePlugin(decodeURIComponent(pluginId));
  if (plugin === undefined) {
    return NextResponse.json({ error: "unavailable" }, { status: 503 });
  }
  if (plugin === null) {
    return NextResponse.json(null, { status: 404 });
  }
  return NextResponse.json(plugin, {
    headers: { "Access-Control-Allow-Origin": "*" },
  });
}
