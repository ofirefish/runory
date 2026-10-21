import { NextResponse } from "next/server";
import { listRevocations } from "@/lib/data/marketplace";

export async function GET() {
  const rows = await listRevocations();
  if (rows === null) {
    return NextResponse.json({ error: "unavailable" }, { status: 503 });
  }
  return NextResponse.json(rows, {
    headers: { "Access-Control-Allow-Origin": "*" },
  });
}
