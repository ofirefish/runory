import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const root = fileURLToPath(new URL("../", import.meta.url));
const defaultMaxAgeMinutes = 60;
const maxEvidenceBytes = 64 * 1024;
const clockSkewMilliseconds = 5 * 60 * 1000;

const topLevelKeys = [
  "auditCronLatestStatus",
  "blockers",
  "checks",
  "generatedAt",
  "migrationCount",
  "migrationVersions",
  "mode",
  "outcome",
  "projectRef",
  "schemaVersion",
  "sourceDigests",
];
const checkKeys = [
  "authConfiguration",
  "databaseReadiness",
  "edgeFunctionJwtVerification",
  "linkedProject",
  "localInputs",
  "requiredSecretNames",
];
const candidateCheckKeys = ["localInputs"];
const digestKeys = [
  "applicationSourceSha256",
  "authTemplatesSha256",
  "databaseTestsSha256",
  "migrationsSha256",
  "policyFunctionSha256",
  "readinessSqlSha256",
];

function fail(message) {
  throw new Error(message);
}

function isRecord(value) {
  return value !== null && typeof value === "object" && !Array.isArray(value);
}

function verifyExactKeys(value, expected, label) {
  if (!isRecord(value)) fail(`${label} must be an object`);
  const actual = Object.keys(value).sort();
  if (JSON.stringify(actual) !== JSON.stringify([...expected].sort())) {
    fail(`${label} fields do not match the approved schema`);
  }
}

export function validateReleaseEvidence(evidence, {
  nowMilliseconds = Date.now(),
  maxAgeMinutes = defaultMaxAgeMinutes,
} = {}) {
  verifyExactKeys(evidence, topLevelKeys, "release evidence");
  if (evidence.schemaVersion !== 2) fail("release evidence schemaVersion is unsupported");
  if (!Number.isInteger(evidence.migrationCount) || evidence.migrationCount <= 0) {
    fail("release evidence migrationCount is invalid");
  }
  if (!Array.isArray(evidence.migrationVersions)
    || evidence.migrationVersions.length !== evidence.migrationCount
    || evidence.migrationVersions.some((value) => !/^\d{14}$/u.test(value))
    || JSON.stringify(evidence.migrationVersions) !== JSON.stringify([...new Set(evidence.migrationVersions)].sort())) {
    fail("release evidence migrationVersions are invalid");
  }

  verifyExactKeys(evidence.checks, checkKeys, "release evidence checks");
  if (checkKeys.some((key) => evidence.checks[key] !== true)) {
    fail("release evidence contains an incomplete readiness check");
  }
  verifyExactKeys(evidence.sourceDigests, digestKeys, "release evidence sourceDigests");
  if (digestKeys.some((key) => !/^[a-f0-9]{64}$/u.test(evidence.sourceDigests[key]))) {
    fail("release evidence contains an invalid source digest");
  }
  const generatedAt = Date.parse(evidence.generatedAt);
  const maxAgeMilliseconds = maxAgeMinutes * 60 * 1000;
  if (!Number.isFinite(generatedAt)
    || !Number.isFinite(maxAgeMilliseconds)
    || maxAgeMinutes <= 0
    || generatedAt > nowMilliseconds + clockSkewMilliseconds
    || nowMilliseconds - generatedAt > maxAgeMilliseconds) {
    fail("release evidence is stale or has an invalid timestamp");
  }
  if (evidence.mode !== "remote" || evidence.outcome !== "release_ready") {
    fail("release evidence was not produced by a successful remote gate");
  }
  if (!/^[a-z0-9]{20}$/u.test(evidence.projectRef)) fail("release evidence projectRef is invalid");
  if (!Array.isArray(evidence.blockers) || evidence.blockers.length !== 0) {
    fail("release evidence contains external blockers");
  }
  if (evidence.auditCronLatestStatus !== "succeeded") {
    fail("release evidence does not contain a successful latest Audit Cron run");
  }
  return evidence;
}

export function validateCandidateEvidence(evidence, {
  nowMilliseconds = Date.now(),
  maxAgeMinutes = defaultMaxAgeMinutes,
} = {}) {
  verifyExactKeys(evidence, topLevelKeys, "candidate evidence");
  if (evidence.schemaVersion !== 2) fail("candidate evidence schemaVersion is unsupported");
  if (evidence.mode !== "check" || evidence.outcome !== "tooling_ready") {
    fail("candidate evidence was not produced by the local readiness gate");
  }
  if (evidence.projectRef !== null || evidence.auditCronLatestStatus !== null) {
    fail("candidate evidence contains remote environment state");
  }
  verifyExactKeys(evidence.checks, candidateCheckKeys, "candidate evidence checks");
  if (evidence.checks.localInputs !== true) fail("candidate local inputs are not ready");
  if (!Array.isArray(evidence.blockers)
    || evidence.blockers.some((value) => ![".env.cloud", "supabase link"].includes(value))) {
    fail("candidate evidence contains an unsupported blocker");
  }
  const remoteShape = {
    ...evidence,
    mode: "remote",
    outcome: "release_ready",
    projectRef: "candidateproject0001",
    checks: Object.fromEntries(checkKeys.map((key) => [key, true])),
    blockers: [],
    auditCronLatestStatus: "succeeded",
  };
  validateReleaseEvidence(remoteShape, { nowMilliseconds, maxAgeMinutes });
  return evidence;
}

function verifyMatchingSources(left, right, label) {
  if (JSON.stringify(left.migrationVersions) !== JSON.stringify(right.migrationVersions)) {
    fail(`${label} migration versions do not match`);
  }
  if (digestKeys.some((key) => left.sourceDigests[key] !== right.sourceDigests[key])) {
    fail(`${label} source digests do not match`);
  }
}

export function prepareReleaseCandidate(staging, candidate, options = {}) {
  const checkedStaging = validateReleaseEvidence(staging, options);
  const checkedCandidate = validateCandidateEvidence(candidate, options);
  verifyMatchingSources(checkedStaging, checkedCandidate, "Staging and candidate");
  return {
    schemaVersion: 2,
    generatedAt: new Date(options.nowMilliseconds ?? Date.now()).toISOString(),
    outcome: "deployment_candidate_ready",
    stagingProjectRef: checkedStaging.projectRef,
    migrationVersions: [...checkedStaging.migrationVersions],
    sourceDigests: { ...checkedStaging.sourceDigests },
    evidenceGeneratedAt: {
      staging: checkedStaging.generatedAt,
      candidate: checkedCandidate.generatedAt,
    },
  };
}

export function compareReleaseEvidence(staging, production, options = {}) {
  const checkedStaging = validateReleaseEvidence(staging, options);
  const checkedProduction = validateReleaseEvidence(production, options);
  if (checkedStaging.projectRef === checkedProduction.projectRef) {
    fail("Staging and Production must use different Supabase projects");
  }
  verifyMatchingSources(checkedStaging, checkedProduction, "Staging and Production");

  return {
    schemaVersion: 2,
    generatedAt: new Date(options.nowMilliseconds ?? Date.now()).toISOString(),
    outcome: "production_verified",
    stagingProjectRef: checkedStaging.projectRef,
    productionProjectRef: checkedProduction.projectRef,
    migrationVersions: [...checkedStaging.migrationVersions],
    sourceDigests: { ...checkedStaging.sourceDigests },
    evidenceGeneratedAt: {
      staging: checkedStaging.generatedAt,
      production: checkedProduction.generatedAt,
    },
  };
}

function readEvidence(path) {
  const content = readFileSync(resolve(root, path));
  if (content.byteLength > maxEvidenceBytes) fail("release evidence exceeds 64 KiB");
  return JSON.parse(content.toString("utf8"));
}

function optionValue(args, name) {
  const index = args.indexOf(name);
  if (index < 0 || !args[index + 1]) fail(`${name} is required`);
  return args[index + 1];
}

function main() {
  const args = process.argv.slice(2);
  const mode = args[0];
  if (!["--prepare", "--finalize"].includes(mode)) {
    fail("use --prepare or --finalize before evidence file options");
  }
  const options = args.slice(1);
  const environmentOption = mode === "--prepare" ? "--candidate" : "--production";
  const allowed = new Set(["--staging", environmentOption, "--max-age-minutes"]);
  const seen = new Set();
  if (![4, 6].includes(options.length)) {
    fail(`use ${mode} --staging <json> ${environmentOption} <json> [--max-age-minutes <minutes>]`);
  }
  for (let index = 0; index < options.length; index += 2) {
    if (!allowed.has(options[index]) || seen.has(options[index]) || options[index + 1] === undefined) {
      fail(`use ${mode} --staging <json> ${environmentOption} <json> [--max-age-minutes <minutes>]`);
    }
    seen.add(options[index]);
  }
  if (!seen.has("--staging") || !seen.has(environmentOption)) {
    fail(`use ${mode} --staging <json> ${environmentOption} <json> [--max-age-minutes <minutes>]`);
  }
  const maxAgeOption = options.includes("--max-age-minutes")
    ? Number(optionValue(options, "--max-age-minutes"))
    : defaultMaxAgeMinutes;
  if (!Number.isFinite(maxAgeOption) || maxAgeOption < 5 || maxAgeOption > 1440) {
    fail("--max-age-minutes must be between 5 and 1440");
  }
  const staging = readEvidence(optionValue(options, "--staging"));
  const target = readEvidence(optionValue(options, environmentOption));
  const report = mode === "--prepare"
    ? prepareReleaseCandidate(staging, target, { maxAgeMinutes: maxAgeOption })
    : compareReleaseEvidence(staging, target, { maxAgeMinutes: maxAgeOption });
  process.stdout.write(`${JSON.stringify(report)}\n`);
}

const entry = process.argv[1] ? pathToFileURL(resolve(process.argv[1])).href : "";
if (import.meta.url === entry) {
  try {
    main();
  } catch (error) {
    process.stderr.write(
      `Promotion gate failed: ${error instanceof Error ? error.message : "unknown error"}\n`,
    );
    process.exitCode = 1;
  }
}
