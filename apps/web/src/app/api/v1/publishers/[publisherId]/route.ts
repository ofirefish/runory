import { NextResponse } from "next/server";
import { getPublisher } from "@/lib/data/marketplace";

type Params = { params: Promise<{ publisherId: string }> };

export async function GET(_request: Request, { params }: Params) {
  const { publisherId } = await params;
  const publisher = await getPublisher(decodeURIComponent(publisherId));
  return NextResponse.json(publisher, {
    headers: { "Access-Control-Allow-Origin": "*" },
  });
}
