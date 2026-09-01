const ACTIONS = new Set([
  "connect",
  "read-files",
  "write-files",
  "operate",
  "deploy",
  "ai-execute",
]);
const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[1-8][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i;
const DECISION_TTL_SECONDS = 300;
const KEY_ID = /^[A-Za-z0-9_-]{1,32}$/u;
const MAX_SIGNING_KEYS = 4;

type DecisionRequest = {
  target_organization_id: string;
  target_action: string;
  target_resource_type: string;
  target_resource_id: string;
};

type SignedDecision = {
  version: number;
  keyId?: string;
  organizationId: string;
  profileId: string;
  action: string;
  allowed: boolean;
  issuedAt: number;
  expiresAt: number;
  signature: string;
};

function json(status: number, body: unknown): Response {
  return Response.json(body, {
    status,
    headers: { "cache-control": "no-store" },
  });
}

function validRequest(value: unknown): value is DecisionRequest {
  if (typeof value !== "object" || value === null) return false;
  const request = value as Partial<DecisionRequest>;
  return typeof request.target_organization_id === "string"
    && UUID.test(request.target_organization_id)
    && typeof request.target_resource_id === "string"
    && UUID.test(request.target_resource_id)
    && typeof request.target_action === "string"
    && ACTIONS.has(request.target_action)
    && request.target_resource_type === "server-profile";
}

function decodeBase64(value: string): Uint8Array {
  const binary = atob(value);
  return Uint8Array.from(binary, (character) => character.charCodeAt(0));
}

function encodeBase64(value: ArrayBuffer): string {
  const bytes = new Uint8Array(value);
  let binary = "";
  for (const byte of bytes) binary += String.fromCharCode(byte);
  return btoa(binary);
}

function canonicalDecision(decision: Omit<SignedDecision, "signature">): string {
  if (decision.version === 2 && decision.keyId && KEY_ID.test(decision.keyId)) {
    return [
      "runory-policy-decision-v2",
      decision.keyId,
      decision.organizationId,
      decision.profileId,
      decision.action,
      String(decision.allowed),
      String(decision.issuedAt),
      String(decision.expiresAt),
    ].join("\n");
  }
  return [
    "runory-policy-decision-v1",
    decision.organizationId,
    decision.profileId,
    decision.action,
    String(decision.allowed),
    String(decision.issuedAt),
    String(decision.expiresAt),
  ].join("\n");
}

function publishableKey(): string | null {
  const current = Deno.env.get("SUPABASE_PUBLISHABLE_KEYS");
  if (current) {
    try {
      const values = JSON.parse(current) as Record<string, unknown>;
      if (typeof values.default === "string") return values.default;
    } catch {
      return null;
    }
  }
  return Deno.env.get("SUPABASE_ANON_KEY") ?? null;
}

async function importSigningKey(encoded: string): Promise<CryptoKey> {
  return await crypto.subtle.importKey(
    "pkcs8",
    decodeBase64(encoded),
    { name: "Ed25519" },
    false,
    ["sign"],
  );
}

async function signingConfiguration(): Promise<{
  version: 1 | 2;
  keyId?: string;
  key: CryptoKey;
}> {
  const activeKeyId = Deno.env.get("RUNORY_POLICY_ACTIVE_SIGNING_KEY_ID");
  const encodedKeys = Deno.env.get("RUNORY_POLICY_SIGNING_KEYS_JSON");
  if (activeKeyId || encodedKeys) {
    if (!activeKeyId || !encodedKeys || !KEY_ID.test(activeKeyId)) {
      throw new Error("policy signing key set is invalid");
    }
    const keys = JSON.parse(encodedKeys) as Record<string, unknown>;
    const entries = Object.entries(keys);
    if (entries.length === 0 || entries.length > MAX_SIGNING_KEYS
      || entries.some(([keyId, value]) => !KEY_ID.test(keyId) || typeof value !== "string")) {
      throw new Error("policy signing key set is invalid");
    }
    const encoded = keys[activeKeyId];
    if (typeof encoded !== "string") throw new Error("active policy signing key is unavailable");
    return { version: 2, keyId: activeKeyId, key: await importSigningKey(encoded) };
  }

  const legacy = Deno.env.get("RUNORY_POLICY_SIGNING_KEY_PKCS8_BASE64");
  if (!legacy) throw new Error("policy signing key is not configured");
  return { version: 1, key: await importSigningKey(legacy) };
}

Deno.serve(async (request: Request) => {
  if (request.method !== "POST") return json(405, { code: "METHOD_NOT_ALLOWED" });
  const authorization = request.headers.get("authorization");
  const apiKey = publishableKey();
  const supabaseUrl = Deno.env.get("SUPABASE_URL");
  if (!authorization || !apiKey || !supabaseUrl) {
    return json(503, { code: "POLICY_SERVICE_UNAVAILABLE" });
  }

  let payload: unknown;
  try {
    payload = await request.json();
  } catch {
    return json(400, { code: "INVALID_REQUEST" });
  }
  if (!validRequest(payload)) return json(400, { code: "INVALID_REQUEST" });

  try {
    const policyResponse = await fetch(`${supabaseUrl}/rest/v1/rpc/evaluate_access_policy`, {
      method: "POST",
      headers: {
        apikey: apiKey,
        authorization,
        "content-type": "application/json",
      },
      body: JSON.stringify(payload),
    });
    if (!policyResponse.ok) return json(503, { code: "POLICY_SERVICE_UNAVAILABLE" });
    const allowed = await policyResponse.json();
    if (typeof allowed !== "boolean") {
      return json(503, { code: "POLICY_SERVICE_UNAVAILABLE" });
    }

    const issuedAt = Math.floor(Date.now() / 1000);
    const signing = await signingConfiguration();
    const unsigned = {
      version: signing.version,
      ...(signing.keyId ? { keyId: signing.keyId } : {}),
      organizationId: payload.target_organization_id,
      profileId: payload.target_resource_id,
      action: payload.target_action,
      allowed,
      issuedAt,
      expiresAt: issuedAt + DECISION_TTL_SECONDS,
    };
    const signature = await crypto.subtle.sign(
      "Ed25519",
      signing.key,
      new TextEncoder().encode(canonicalDecision(unsigned)),
    );
    return json(200, { ...unsigned, signature: encodeBase64(signature) });
  } catch {
    return json(503, { code: "POLICY_SERVICE_UNAVAILABLE" });
  }
});
