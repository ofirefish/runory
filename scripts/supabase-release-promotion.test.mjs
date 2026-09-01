import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

import {
  compareReleaseEvidence,
  prepareReleaseCandidate,
  validateCandidateEvidence,
  validateReleaseEvidence,
} from "./supabase-release-promotion.mjs";

const nowMilliseconds = Date.parse("2026-08-30T00:00:00.000Z");

function evidence(projectRef, overrides = {}) {
  return {
    schemaVersion: 2,
    generatedAt: "2026-08-29T23:45:00.000Z",
    outcome: "release_ready",
    mode: "remote",
    projectRef,
    migrationCount: 2,
    migrationVersions: ["20260828090557", "20260829055451"],
    sourceDigests: {
      applicationSourceSha256: "0".repeat(64),
      migrationsSha256: "a".repeat(64),
      policyFunctionSha256: "b".repeat(64),
      authTemplatesSha256: "c".repeat(64),
      readinessSqlSha256: "d".repeat(64),
      databaseTestsSha256: "e".repeat(64),
    },
    checks: {
      localInputs: true,
      linkedProject: true,
      edgeFunctionJwtVerification: true,
      requiredSecretNames: true,
      databaseReadiness: true,
      authConfiguration: true,
    },
    blockers: [],
    auditCronLatestStatus: "succeeded",
    ...overrides,
  };
}

function candidate(overrides = {}) {
  const value = evidence("candidateproject0001", overrides);
  return {
    ...value,
    outcome: "tooling_ready",
    mode: "check",
    projectRef: null,
    checks: { localInputs: true },
    blockers: [".env.cloud", "supabase link"],
    auditCronLatestStatus: null,
  };
}

test("fresh Staging evidence approves an identical local deployment candidate", () => {
  const result = prepareReleaseCandidate(
    evidence("stagingprojectref001"),
    candidate(),
    { nowMilliseconds, maxAgeMinutes: 60 },
  );
  assert.equal(result.outcome, "deployment_candidate_ready");
  assert.throws(() => validateCandidateEvidence({
    ...candidate(),
    projectRef: "stagingprojectref001",
  }, { nowMilliseconds }));
});

test("matching fresh Staging and Production evidence verifies the deployment", () => {
  const result = compareReleaseEvidence(
    evidence("stagingprojectref001"),
    evidence("productionproject001"),
    { nowMilliseconds, maxAgeMinutes: 60 },
  );
  assert.equal(result.outcome, "production_verified");
  assert.equal(result.migrationVersions.length, 2);
});

test("promotion rejects the same project, stale evidence, and source drift", () => {
  const staging = evidence("stagingprojectref001");
  assert.throws(() => compareReleaseEvidence(staging, staging, { nowMilliseconds }));
  assert.throws(() => validateReleaseEvidence(
    evidence("stagingprojectref001", { generatedAt: "2026-08-29T20:00:00.000Z" }),
    { nowMilliseconds, maxAgeMinutes: 60 },
  ));
  const production = evidence("productionproject001");
  production.sourceDigests.policyFunctionSha256 = "f".repeat(64);
  assert.throws(() => compareReleaseEvidence(staging, production, { nowMilliseconds }));
});

test("strict evidence schema rejects unexpected credential-shaped fields", () => {
  assert.throws(() => validateReleaseEvidence({
    ...evidence("stagingprojectref001"),
    accessToken: "must-not-enter-an-artifact",
  }, { nowMilliseconds }));
});

test("CLI preparation and finalization emit one JSON report per gate", () => {
  const directory = mkdtempSync(join(tmpdir(), "runory-promotion-"));
  try {
    const stagingPath = join(directory, "staging.json");
    const candidatePath = join(directory, "candidate.json");
    const productionPath = join(directory, "production.json");
    const generatedAt = new Date().toISOString();
    writeFileSync(stagingPath, JSON.stringify(evidence("stagingprojectref001", { generatedAt })));
    writeFileSync(candidatePath, JSON.stringify(candidate({ generatedAt })));
    writeFileSync(productionPath, JSON.stringify(evidence("productionproject001", { generatedAt })));
    const script = fileURLToPath(new URL("./supabase-release-promotion.mjs", import.meta.url));
    const prepared = execFileSync(process.execPath, [
      script,
      "--prepare",
      "--staging", stagingPath,
      "--candidate", candidatePath,
    ], { encoding: "utf8" });
    const finalized = execFileSync(process.execPath, [
      script,
      "--finalize",
      "--staging", stagingPath,
      "--production", productionPath,
    ], { encoding: "utf8" });
    assert.equal(JSON.parse(prepared).outcome, "deployment_candidate_ready");
    assert.equal(JSON.parse(finalized).outcome, "production_verified");
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
});
