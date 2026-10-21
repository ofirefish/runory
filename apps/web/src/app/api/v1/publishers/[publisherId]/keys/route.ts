import { NextResponse } from "next/server";
import { listPublisherKeys } from "@/lib/data/marketplace";

type Params = { params: Promise<{ publisherId: string }> };

export async function GET(_request: Request, { params }: Params) {
  const { publisherId } = await params;
  const keys = await listPublisherKeys(decodeURIComponent(publisherId));
  if (keys === null) {
    return NextResponse.json({ error: "unavailable" }, { status: 503 });
  }
  return NextResponse.json(keys, {
    headers: { "Access-Control-Allow-Origin": "*" },
  });
}
