import assert from "node:assert/strict";
import test from "node:test";

import {
  buildReadinessReport,
  parseCliJson,
  verifyDatabaseRow,
  verifyFunctionRows,
  verifySecretRows,
} from "./supabase-remote-readiness.mjs";

test("machine-readable report has a stable shape and excludes supplied credentials", () => {
  const report = buildReadinessReport({
    mode: "remote",
    migrations: ["20260828090557"],
    projectRef: "abcdefghijklmnopqrst",
    cronStatus: "succeeded",
    accessToken: "should-never-be-serialized",
    smtpPassword: "should-never-be-serialized",
    digests: {
      applicationSourceSha256: "f".repeat(64),
      migrationsSha256: "a".repeat(64),
      policyFunctionSha256: "b".repeat(64),
      managedAiFunctionSha256: "1".repeat(64),
      authTemplatesSha256: "c".repeat(64),
      readinessSqlSha256: "d".repeat(64),
      databaseTestsSha256: "e".repeat(64),
    },
  });
  assert.equal(report.schemaVersion, 2);
  assert.equal(report.outcome, "release_ready");
  assert.equal(report.checks.requiredSecretNames, true);
  assert.equal(report.auditCronLatestStatus, "succeeded");
  assert.doesNotMatch(JSON.stringify(report), /should-never-be-serialized/u);
});

test("CLI JSON adapters accept raw and wrapped arrays", () => {
  assert.deepEqual(parseCliJson('[{"name":"one"}]'), [{ name: "one" }]);
  assert.deepEqual(parseCliJson('{"result":[{"name":"two"}]}'), [{ name: "two" }]);
  assert.throws(() => parseCliJson('{"unexpected":[]}'));
});

test("function and secret checks require the signed policy deployment", () => {
  assert.doesNotThrow(() => verifyFunctionRows([
    { name: "evaluate-access-policy", verify_jwt: true },
    { name: "agent-turn", verify_jwt: true },
  ]));
  assert.throws(() => verifyFunctionRows([]));
  assert.throws(() => verifyFunctionRows([
    { name: "evaluate-access-policy", verify_jwt: false },
    { name: "agent-turn", verify_jwt: true },
  ]));
  assert.doesNotThrow(() => verifySecretRows([
    { name: "RUNORY_POLICY_ACTIVE_SIGNING_KEY_ID" },
    { name: "RUNORY_POLICY_SIGNING_KEYS_JSON" },
    { name: "RUNORY_DEEPSEEK_API_KEY" },
    { name: "RUNORY_GLM_API_KEY" },
  ]));
  assert.throws(() => verifySecretRows([
    { name: "RUNORY_POLICY_ACTIVE_SIGNING_KEY_ID" },
  ]));
});

test("database readiness requires exact migrations and every security invariant", () => {
  const migrations = ["20260828090557", "20260829055451"];
  const row = {
    migration_versions_json: JSON.stringify(migrations),
    postgres_17_or_newer: true,
    all_business_tables_have_rls: true,
    policy_rpc_acl_valid: true,
    billing_service_rpc_acl_valid: true,
    billing_trial_rpc_acl_valid: true,
    retention_function_private: true,
    retention_index_present: true,
    audit_cron_unique: true,
    audit_cron_schedule_valid: true,
    audit_cron_active: true,
    audit_cron_command_valid: true,
    audit_cron_has_successful_run: true,
    audit_cron_latest_succeeded: true,
    audit_cron_latest_status: "succeeded",
    migration_count: migrations.length,
  };
  assert.equal(verifyDatabaseRow(row, migrations), "succeeded");
  assert.throws(() => verifyDatabaseRow({ ...row, policy_rpc_acl_valid: false }, migrations));
  assert.throws(() => verifyDatabaseRow(row, [migrations[0]]));
});
