import { NextResponse } from "next/server";
import {
  assertMarketplaceAdminWrite,
  insertRevocation,
} from "@/lib/data/marketplace";

export async function POST(request: Request) {
  const auth = await assertMarketplaceAdminWrite(request);
  if (!auth.ok) {
    return NextResponse.json({ error: auth.error }, { status: auth.status });
  }

  let body: {
    kind?: string;
    targetId?: string;
    version?: string | null;
    reason?: string | null;
  };
  try {
    body = await request.json();
  } catch {
    return NextResponse.json({ error: "invalid" }, { status: 400 });
  }

  if (!body.kind || !body.targetId) {
    return NextResponse.json({ error: "invalid" }, { status: 400 });
  }

  const result = await insertRevocation({
    kind: body.kind,
    targetId: body.targetId,
    version: body.version,
    reason: body.reason,
  });
  if (!result.ok) {
    return NextResponse.json({ error: result.error }, { status: result.status });
  }
  return NextResponse.json({ ok: true });
}
