import { createHash, generateKeyPairSync } from "node:crypto";
import { constants, existsSync, writeFileSync } from "node:fs";
import { resolve } from "node:path";

const signingPath = resolve(".env.policy-signing");
const buildPath = resolve(".env.policy-build");
if (existsSync(signingPath) || existsSync(buildPath)) {
  throw new Error("Policy key files already exist; rotate keys through a reviewed release instead of overwriting them.");
}

const { privateKey, publicKey } = generateKeyPairSync("ed25519");
const privateDer = privateKey.export({ format: "der", type: "pkcs8" });
const publicJwk = publicKey.export({ format: "jwk" });
if (typeof publicJwk.x !== "string") throw new Error("Could not export the Ed25519 public key.");
const publicRaw = Buffer.from(publicJwk.x, "base64url");
const keyId = `policy-${createHash("sha256").update(publicRaw).digest("hex").slice(0, 12)}`;

writeFileSync(
  signingPath,
  [
    `RUNORY_POLICY_ACTIVE_SIGNING_KEY_ID=${keyId}`,
    `RUNORY_POLICY_SIGNING_KEYS_JSON=${JSON.stringify({ [keyId]: privateDer.toString("base64") })}`,
    "",
  ].join("\n"),
  { encoding: "utf8", mode: 0o600, flag: constants.O_CREAT | constants.O_EXCL | constants.O_WRONLY },
);
writeFileSync(
  buildPath,
  `RUNORY_POLICY_VERIFYING_KEYS_JSON=${JSON.stringify({ [keyId]: publicRaw.toString("base64") })}\n`,
  { encoding: "utf8", mode: 0o600, flag: constants.O_CREAT | constants.O_EXCL | constants.O_WRONLY },
);
console.log("Created .env.policy-signing and .env.policy-build without printing key material.");
