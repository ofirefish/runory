import { NextResponse } from "next/server";
import { resolvePackageDownload } from "@/lib/data/marketplace";

type Params = { params: Promise<{ pluginId: string; version: string }> };

export async function GET(_request: Request, { params }: Params) {
  const { pluginId, version } = await params;
  const result = await resolvePackageDownload(
    decodeURIComponent(pluginId),
    decodeURIComponent(version),
  );
  if (result === undefined) {
    return NextResponse.json({ error: "unavailable" }, { status: 503 });
  }
  if (result === null) {
    return NextResponse.json({ error: "missing" }, { status: 404 });
  }
  return NextResponse.redirect(result.signedUrl, {
    headers: {
      "Access-Control-Allow-Origin": "*",
      "Cache-Control": "private, no-store",
    },
  });
}
