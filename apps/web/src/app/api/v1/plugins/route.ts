import { NextResponse } from "next/server";
import { listMarketplacePlugins } from "@/lib/data/marketplace";

export async function GET() {
  const plugins = await listMarketplacePlugins();
  if (plugins === null) {
    return NextResponse.json({ error: "unavailable" }, { status: 503 });
  }
  return NextResponse.json(plugins, {
    headers: { "Access-Control-Allow-Origin": "*" },
  });
}

export async function OPTIONS() {
  return new NextResponse(null, {
    status: 204,
    headers: {
      "Access-Control-Allow-Origin": "*",
      "Access-Control-Allow-Methods": "GET, OPTIONS",
      "Access-Control-Allow-Headers": "Content-Type",
    },
  });
}
