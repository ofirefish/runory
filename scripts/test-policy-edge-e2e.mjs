import { execFileSync, spawn } from "node:child_process";
import { generateKeyPairSync, randomBytes, randomUUID, verify } from "node:crypto";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const ACTION = "connect";
const TTL_SECONDS = 300;
const KEY_ID_A = "policy-e2e-a";
const KEY_ID_B = "policy-e2e-b";

function statusEnvironment() {
  const output = execFileSync("supabase", ["status", "-o", "env"], {
    encoding: "utf8",
    stdio: ["ignore", "pipe", "ignore"],
  });
  return Object.fromEntries(output.split(/\r?\n/u).flatMap((line) => {
    const match = /^([A-Z][A-Z0-9_]*)=(?:"([^"]*)"|(.*))$/u.exec(line.trim());
    return match ? [[match[1], match[2] ?? match[3] ?? ""]] : [];
  }));
}

function localApiUrl(value) {
  const url = new URL(value);
  if (url.protocol !== "http:" || !["127.0.0.1", "localhost"].includes(url.hostname)) {
    throw new Error("Policy E2E refuses to use a non-local Supabase project.");
  }
  return url.origin;
}

async function request(url, options) {
  return await fetch(url, { ...options, signal: AbortSignal.timeout(10_000) });
}

async function jsonRequest(url, options) {
  const response = await request(url, options);
  const body = await response.json().catch(() => null);
  if (!response.ok) throw new Error(`Local E2E request failed with HTTP ${response.status}.`);
  return body;
}

function userHeaders(apiKey, accessToken) {
  return {
    apikey: apiKey,
    authorization: `Bearer ${accessToken}`,
    "content-type": "application/json",
  };
}

function adminHeaders(serviceRoleKey) {
  return {
    apikey: serviceRoleKey,
    authorization: `Bearer ${serviceRoleKey}`,
    "content-type": "application/json",
  };
}

async function removeStaleE2eUsers(apiUrl, serviceRoleKey) {
  const result = await jsonRequest(`${apiUrl}/auth/v1/admin/users?per_page=1000`, {
    headers: adminHeaders(serviceRoleKey),
  });
  const users = Array.isArray(result?.users) ? result.users : [];
  for (const user of users) {
    if (typeof user?.id !== "string" || typeof user.email !== "string"
      || !user.email.startsWith("policy-e2e-") || !user.email.endsWith("@example.test")) continue;
    await jsonRequest(`${apiUrl}/rest/v1/organizations?owner_id=eq.${user.id}`, {
      method: "DELETE",
      headers: { ...adminHeaders(serviceRoleKey), prefer: "return=minimal" },
    });
    await jsonRequest(`${apiUrl}/auth/v1/admin/users/${user.id}`, {
      method: "DELETE",
      headers: adminHeaders(serviceRoleKey),
    });
  }
}

function canonicalDecision(value) {
  if (value.version === 2 && typeof value.keyId === "string") {
    return [
      "runory-policy-decision-v2",
      value.keyId,
      value.organizationId,
      value.profileId,
      value.action,
      String(value.allowed),
      String(value.issuedAt),
      String(value.expiresAt),
    ].join("\n");
  }
  return [
    "runory-policy-decision-v1",
    value.organizationId,
    value.profileId,
    value.action,
    String(value.allowed),
    String(value.issuedAt),
    String(value.expiresAt),
  ].join("\n");
}

function verifyDecision(value, publicKey, expected) {
  if (value?.version !== 2
    || value.keyId !== expected.keyId
    || value.organizationId !== expected.organizationId
    || value.profileId !== expected.profileId
    || value.action !== expected.action
    || typeof value.allowed !== "boolean"
    || !Number.isSafeInteger(value.issuedAt)
    || !Number.isSafeInteger(value.expiresAt)
    || value.expiresAt - value.issuedAt !== TTL_SECONDS
    || value.expiresAt <= Math.floor(Date.now() / 1000)
    || typeof value.signature !== "string"
    || !verify(null, Buffer.from(canonicalDecision(value)), publicKey, Buffer.from(value.signature, "base64"))) {
    throw new Error("Edge Function returned an invalid signed decision.");
  }
}

async function waitForFunction(apiUrl, apiKey, accessToken, payload, expectedKeyId) {
  let lastError;
  for (let attempt = 0; attempt < 30; attempt += 1) {
    try {
      const decision = await jsonRequest(`${apiUrl}/functions/v1/evaluate-access-policy`, {
        method: "POST",
        headers: userHeaders(apiKey, accessToken),
        body: JSON.stringify(payload),
      });
      if (decision?.version !== 2 || decision.keyId !== expectedKeyId) {
        throw new Error("The updated policy worker is not ready.");
      }
      return decision;
    } catch (error) {
      lastError = error;
      await new Promise((resolve) => setTimeout(resolve, 500));
    }
  }
  throw lastError;
}

const environment = statusEnvironment();
const apiUrl = localApiUrl(environment.API_URL ?? environment.SUPABASE_URL ?? "");
const publishableKey = environment.PUBLISHABLE_KEY ?? environment.ANON_KEY;
const serviceRoleKey = environment.SERVICE_ROLE_KEY;
if (!publishableKey || !serviceRoleKey) throw new Error("Local Supabase keys are unavailable.");

const directory = mkdtempSync(join(tmpdir(), "runory-policy-e2e-"));
const email = `policy-e2e-${randomUUID()}@example.test`;
const password = randomBytes(24).toString("base64url");
const organizationId = randomUUID();
const profileId = randomUUID();
let userId;
let accessToken;
let organizationCreated = false;
let functionProcess;

try {
  await removeStaleE2eUsers(apiUrl, serviceRoleKey);
  const firstPair = generateKeyPairSync("ed25519");
  const secondPair = generateKeyPairSync("ed25519");
  const firstPrivate = firstPair.privateKey.export({ format: "der", type: "pkcs8" }).toString("base64");
  const secondPrivate = secondPair.privateKey.export({ format: "der", type: "pkcs8" }).toString("base64");
  const envFile = join(directory, "function.env");
  writeFileSync(
    envFile,
    [
      `RUNORY_POLICY_ACTIVE_SIGNING_KEY_ID=${KEY_ID_A}`,
      `RUNORY_POLICY_SIGNING_KEYS_JSON=${JSON.stringify({ [KEY_ID_A]: firstPrivate })}`,
      "",
    ].join("\n"),
    { encoding: "utf8", mode: 0o600 },
  );

  const created = await jsonRequest(`${apiUrl}/auth/v1/admin/users`, {
    method: "POST",
    headers: {
      ...adminHeaders(serviceRoleKey),
    },
    body: JSON.stringify({ email, password, email_confirm: true }),
  });
  userId = created.id;

  const session = await jsonRequest(`${apiUrl}/auth/v1/token?grant_type=password`, {
    method: "POST",
    headers: { apikey: publishableKey, "content-type": "application/json" },
    body: JSON.stringify({ email, password }),
  });
  accessToken = session.access_token;
  if (typeof accessToken !== "string") throw new Error("Local sign-in did not return an access token.");

  await jsonRequest(`${apiUrl}/rest/v1/organizations`, {
    method: "POST",
    headers: { ...userHeaders(publishableKey, accessToken), prefer: "return=minimal" },
    body: JSON.stringify({ id: organizationId, name: "Policy E2E", owner_id: userId }),
  });
  organizationCreated = true;

  functionProcess = spawn("supabase", ["functions", "serve", "--env-file", envFile], {
    cwd: process.cwd(),
    stdio: ["ignore", "ignore", "ignore"],
    windowsHide: true,
  });

  const payload = {
    target_organization_id: organizationId,
    target_action: ACTION,
    target_resource_type: "server-profile",
    target_resource_id: profileId,
  };
  const allowed = await waitForFunction(
    apiUrl,
    publishableKey,
    accessToken,
    payload,
    KEY_ID_A,
  );
  verifyDecision(allowed, firstPair.publicKey, {
    organizationId,
    profileId,
    action: ACTION,
    keyId: KEY_ID_A,
  });
  if (!allowed.allowed) throw new Error("Default policy should allow an organization member.");

  await jsonRequest(`${apiUrl}/rest/v1/rpc/create_access_policy`, {
    method: "POST",
    headers: userHeaders(publishableKey, accessToken),
    body: JSON.stringify({
      target_organization_id: organizationId,
      target_name: "Policy E2E deny",
      target_effect: "deny",
      target_action: ACTION,
      target_resource_selector: {},
    }),
  });
  const denied = await jsonRequest(`${apiUrl}/functions/v1/evaluate-access-policy`, {
    method: "POST",
    headers: userHeaders(publishableKey, accessToken),
    body: JSON.stringify(payload),
  });
  verifyDecision(denied, firstPair.publicKey, {
    organizationId,
    profileId,
    action: ACTION,
    keyId: KEY_ID_A,
  });
  if (denied.allowed) throw new Error("Deny-overrides policy was not enforced.");

  const tampered = { ...denied, allowed: true };
  if (verify(null, Buffer.from(canonicalDecision(tampered)), firstPair.publicKey, Buffer.from(denied.signature, "base64"))) {
    throw new Error("Tampered decision unexpectedly retained a valid signature.");
  }

  functionProcess.kill();
  functionProcess = undefined;
  await new Promise((resolve) => setTimeout(resolve, 500));
  writeFileSync(
    envFile,
    [
      `RUNORY_POLICY_ACTIVE_SIGNING_KEY_ID=${KEY_ID_B}`,
      `RUNORY_POLICY_SIGNING_KEYS_JSON=${JSON.stringify({
        [KEY_ID_A]: firstPrivate,
        [KEY_ID_B]: secondPrivate,
      })}`,
      "",
    ].join("\n"),
    { encoding: "utf8", mode: 0o600 },
  );
  functionProcess = spawn("supabase", ["functions", "serve", "--env-file", envFile], {
    cwd: process.cwd(),
    stdio: ["ignore", "ignore", "ignore"],
    windowsHide: true,
  });
  const rotated = await waitForFunction(
    apiUrl,
    publishableKey,
    accessToken,
    payload,
    KEY_ID_B,
  );
  verifyDecision(rotated, secondPair.publicKey, {
    organizationId,
    profileId,
    action: ACTION,
    keyId: KEY_ID_B,
  });
  if (rotated.allowed) throw new Error("Deny policy changed during signing-key rotation.");
  console.log("Policy Edge E2E passed: JWT, RLS, allow/deny, tamper rejection, and key rotation.");
} finally {
  functionProcess?.kill();
  if (organizationCreated && accessToken) {
    const response = await request(`${apiUrl}/rest/v1/organizations?id=eq.${organizationId}`, {
      method: "DELETE",
      headers: { ...userHeaders(publishableKey, accessToken), prefer: "return=minimal" },
    });
    if (!response.ok) throw new Error("Could not clean up the local E2E organization.");
  }
  if (userId) {
    const response = await request(`${apiUrl}/auth/v1/admin/users/${userId}`, {
      method: "DELETE",
      headers: adminHeaders(serviceRoleKey),
    });
    if (!response.ok) throw new Error("Could not clean up the local E2E user.");
  }
  rmSync(directory, { recursive: true, force: true });
}
