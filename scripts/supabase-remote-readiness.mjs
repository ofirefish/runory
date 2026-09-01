import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { existsSync, readFileSync, readdirSync } from "node:fs";
import { relative, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const root = fileURLToPath(new URL("../", import.meta.url));
const migrationsDirectory = resolve(root, "supabase/migrations");
const linkedProjectPath = resolve(root, "supabase/.temp/project-ref");
const readinessSqlPath = resolve(root, "scripts/sql/supabase-remote-readiness.sql");
const authTemplatesDirectory = resolve(root, "supabase/templates");
const policyFunctionPath = resolve(root, "supabase/functions/evaluate-access-policy/index.ts");
const databaseTestPath = resolve(root, "supabase/tests/cloud_security.test.sql");
const requiredSecretNames = new Set([
  "RUNORY_POLICY_ACTIVE_SIGNING_KEY_ID",
  "RUNORY_POLICY_SIGNING_KEYS_JSON",
]);

function fail(message) {
  throw new Error(message);
}

function required(name) {
  const value = process.env[name]?.trim();
  if (!value) fail(`${name} is required`);
  return value;
}

function run(command, args) {
  try {
    return execFileSync(command, args, {
      cwd: root,
      encoding: "utf8",
      env: process.env,
      stdio: ["ignore", "pipe", "pipe"],
    }).trim();
  } catch {
    fail(`${command} ${args[0] ?? ""} failed; inspect the command directly for details`);
  }
}

export function parseCliJson(output) {
  const value = JSON.parse(output);
  if (Array.isArray(value)) return value;
  for (const key of ["data", "result", "functions", "secrets"]) {
    if (Array.isArray(value?.[key])) return value[key];
  }
  fail("Supabase CLI returned an unsupported JSON shape");
}

function digestFiles(paths) {
  const hash = createHash("sha256");
  for (const path of [...paths].sort()) {
    hash.update(relative(root, path).replaceAll("\\", "/"));
    hash.update("\0");
    hash.update(readFileSync(path));
    hash.update("\0");
  }
  return hash.digest("hex");
}

function collectFiles(directory, predicate) {
  const paths = [];
  for (const entry of readdirSync(directory, { withFileTypes: true })) {
    const path = resolve(directory, entry.name);
    if (entry.isDirectory()) paths.push(...collectFiles(path, predicate));
    if (entry.isFile() && predicate(path)) paths.push(path);
  }
  return paths;
}

function sourceDigests() {
  const migrations = readdirSync(migrationsDirectory)
    .filter((name) => /^\d{14}_.+\.sql$/u.test(name))
    .map((name) => resolve(migrationsDirectory, name));
  const templates = readdirSync(authTemplatesDirectory)
    .filter((name) => name.endsWith(".html"))
    .map((name) => resolve(authTemplatesDirectory, name));
  const applicationFiles = [
    ...collectFiles(resolve(root, "src"), (path) => /\.(css|ts|tsx)$/u.test(path)),
    ...collectFiles(resolve(root, "src-tauri/src"), (path) => path.endsWith(".rs")),
    ...collectFiles(resolve(root, "src-tauri/capabilities"), (path) => path.endsWith(".json")),
    ...[
      "package.json",
      "pnpm-lock.yaml",
      "eslint.config.js",
      "postcss.config.js",
      "tailwind.config.ts",
      "tsconfig.app.json",
      "tsconfig.json",
      "tsconfig.node.json",
      "vite.config.ts",
      "src-tauri/build.rs",
      "src-tauri/Cargo.lock",
      "src-tauri/Cargo.toml",
      "src-tauri/tauri.conf.json",
      "src-tauri/tauri.android.conf.json",
      "src-tauri/tauri.ios.conf.json",
    ].map((path) => resolve(root, path)).filter(existsSync),
  ];
  return {
    applicationSourceSha256: digestFiles(applicationFiles),
    migrationsSha256: digestFiles(migrations),
    policyFunctionSha256: digestFiles([policyFunctionPath]),
    authTemplatesSha256: digestFiles(templates),
    readinessSqlSha256: digestFiles([readinessSqlPath]),
    databaseTestsSha256: digestFiles([databaseTestPath]),
  };
}

export function buildReadinessReport({
  mode,
  migrations,
  blockers = [],
  projectRef = null,
  cronStatus = null,
  digests = sourceDigests(),
}) {
  return {
    schemaVersion: 2,
    generatedAt: new Date().toISOString(),
    outcome: mode === "remote" ? "release_ready" : "tooling_ready",
    mode,
    projectRef,
    migrationCount: migrations.length,
    migrationVersions: [...migrations],
    sourceDigests: { ...digests },
    checks: mode === "remote"
      ? {
          localInputs: true,
          linkedProject: true,
          edgeFunctionJwtVerification: true,
          requiredSecretNames: true,
          databaseReadiness: true,
          authConfiguration: true,
        }
      : {
          localInputs: true,
        },
    blockers: [...blockers],
    auditCronLatestStatus: cronStatus,
  };
}

function emitReport(report, json) {
  if (json) {
    process.stdout.write(`${JSON.stringify(report)}\n`);
    return;
  }
  if (report.mode === "remote") {
    process.stdout.write(
      `Remote Supabase readiness verified; audit Cron latest status: ${report.auditCronLatestStatus ?? "no-run-history"}.\n`,
    );
    return;
  }
  process.stdout.write(
    `Remote readiness tooling validated with ${report.migrationCount} migrations; external blockers: ${report.blockers.join(", ") || "none"}.\n`,
  );
}

function localMigrationVersions() {
  return readdirSync(migrationsDirectory)
    .filter((name) => /^\d{14}_.+\.sql$/u.test(name))
    .map((name) => name.slice(0, 14))
    .sort();
}

function verifyLocalFiles() {
  const config = readFileSync(resolve(root, "supabase/config.toml"), "utf8");
  if (!/\[functions\.evaluate-access-policy\][\s\S]*?verify_jwt\s*=\s*true/u.test(config)) {
    fail("evaluate-access-policy must keep verify_jwt = true");
  }
  const example = readFileSync(resolve(root, ".env.cloud.example"), "utf8");
  for (const name of [
    "SUPABASE_ACCESS_TOKEN",
    "SUPABASE_PROJECT_REF",
    "RUNORY_POLICY_ACTIVE_SIGNING_KEY_ID",
    "RUNORY_POLICY_SIGNING_KEYS_JSON",
  ]) {
    if (!example.includes(`${name}=`)) fail(`.env.cloud.example is missing ${name}`);
  }
  if (!existsSync(readinessSqlPath)) fail("remote readiness SQL is missing");
  const migrations = localMigrationVersions();
  if (migrations.length === 0 || new Set(migrations).size !== migrations.length) {
    fail("local migration versions are empty or duplicated");
  }
  run("node", [resolve(root, "scripts/supabase-auth-config.mjs"), "--check"]);
  run("node", [
    resolve(root, "node_modules/typescript/bin/tsc"),
    "-p",
    resolve(root, "supabase/functions/tsconfig.json"),
    "--pretty",
    "false",
  ]);
  return migrations;
}

export function verifyFunctionRows(rows) {
  const policy = rows.find((value) =>
    [value?.name, value?.slug].includes("evaluate-access-policy")
  );
  if (!policy) fail("evaluate-access-policy is not deployed");
  if ((policy.verify_jwt ?? policy.verifyJwt) !== true) {
    fail("evaluate-access-policy does not explicitly report JWT verification enabled");
  }
}

export function verifySecretRows(rows) {
  const names = new Set(rows.map((value) => value?.name).filter((value) => typeof value === "string"));
  for (const name of requiredSecretNames) {
    if (!names.has(name)) fail(`remote Edge Function secret is missing: ${name}`);
  }
}

export function verifyDatabaseRow(row, expectedMigrations) {
  if (!row || typeof row !== "object") fail("remote database readiness row is missing");
  const remoteMigrations = JSON.parse(row.migration_versions_json ?? "[]");
  if (row.migration_count !== expectedMigrations.length
    || JSON.stringify(remoteMigrations) !== JSON.stringify(expectedMigrations)) {
    fail("remote migration history does not exactly match local migrations");
  }
  for (const field of [
    "postgres_17_or_newer",
    "all_business_tables_have_rls",
    "policy_rpc_acl_valid",
    "retention_function_private",
    "retention_index_present",
    "audit_cron_unique",
    "audit_cron_schedule_valid",
    "audit_cron_active",
    "audit_cron_command_valid",
    "audit_cron_has_successful_run",
    "audit_cron_latest_succeeded",
  ]) {
    if (row[field] !== true) fail(`remote database readiness check failed: ${field}`);
  }
  return row.audit_cron_latest_status ?? null;
}

function remoteVerification(localMigrations) {
  const accessToken = required("SUPABASE_ACCESS_TOKEN");
  const projectRef = required("SUPABASE_PROJECT_REF");
  if (!/^[a-z0-9]{20}$/u.test(projectRef)) fail("SUPABASE_PROJECT_REF is invalid");
  if (accessToken.length < 20) fail("SUPABASE_ACCESS_TOKEN is invalid");
  if (!existsSync(linkedProjectPath)) {
    fail("project is not linked; review and run supabase link --project-ref before remote verification");
  }
  const linkedProject = readFileSync(linkedProjectPath, "utf8").trim();
  if (linkedProject !== projectRef) fail("linked Supabase project does not match SUPABASE_PROJECT_REF");

  const functions = parseCliJson(run("supabase", [
    "functions", "list", "--project-ref", projectRef, "-o", "json", "--agent=no",
  ]));
  verifyFunctionRows(functions);
  const secrets = parseCliJson(run("supabase", [
    "secrets", "list", "--project-ref", projectRef, "-o", "json", "--agent=no",
  ]));
  verifySecretRows(secrets);
  const databaseRows = parseCliJson(run("supabase", [
    "db", "query", "--linked", "--file", readinessSqlPath, "-o", "json", "--agent=no",
  ]));
  if (databaseRows.length !== 1) fail("remote readiness query must return exactly one row");
  const cronStatus = verifyDatabaseRow(databaseRows[0], localMigrations);
  run("node", [resolve(root, "scripts/supabase-auth-config.mjs"), "--verify"]);
  return { projectRef, cronStatus };
}

function main() {
  const args = process.argv.slice(2);
  const modes = args.filter((value) => ["--check", "--remote"].includes(value));
  const unknown = args.filter((value) => !["--check", "--remote", "--json"].includes(value));
  if (unknown.length > 0 || modes.length > 1) fail("use --check or --remote with optional --json");
  const mode = modes[0] ?? "--check";
  if (!["--check", "--remote"].includes(mode)) fail("use --check or --remote");
  const migrations = verifyLocalFiles();
  if (mode === "--remote") {
    const result = remoteVerification(migrations);
    emitReport(buildReadinessReport({
      mode: "remote",
      migrations,
      projectRef: result.projectRef,
      cronStatus: result.cronStatus,
    }), args.includes("--json"));
  } else {
    const blockers = [];
    if (!existsSync(resolve(root, ".env.cloud"))) blockers.push(".env.cloud");
    if (!existsSync(linkedProjectPath)) blockers.push("supabase link");
    emitReport(buildReadinessReport({ mode: "check", migrations, blockers }), args.includes("--json"));
  }
}

const entry = process.argv[1] ? pathToFileURL(resolve(process.argv[1])).href : "";
if (import.meta.url === entry) {
  try {
    main();
  } catch (error) {
    process.stderr.write(`Remote readiness failed: ${error instanceof Error ? error.message : "unknown error"}\n`);
    process.exitCode = 1;
  }
}
