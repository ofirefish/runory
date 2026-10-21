import { NextResponse } from "next/server";
import {
  assertMarketplaceAdminWrite,
  upsertPublisherKey,
} from "@/lib/data/marketplace";

export async function POST(request: Request) {
  const auth = await assertMarketplaceAdminWrite(request);
  if (!auth.ok) {
    return NextResponse.json({ error: auth.error }, { status: auth.status });
  }

  let body: {
    publisherId?: string;
    keyId?: string;
    publicKey?: string;
    algorithm?: string;
    status?: string;
  };
  try {
    body = await request.json();
  } catch {
    return NextResponse.json({ error: "invalid" }, { status: 400 });
  }

  const result = await upsertPublisherKey({
    publisherId: body.publisherId ?? "",
    keyId: body.keyId ?? "",
    publicKey: body.publicKey ?? "",
    algorithm: body.algorithm,
    status: body.status,
  });
  if (!result.ok) {
    return NextResponse.json({ error: result.error }, { status: result.status });
  }
  return NextResponse.json({ ok: true });
}
