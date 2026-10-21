import assert from "node:assert/strict";
import test from "node:test";

import {
  REQUIRED_UPDATER_PLATFORMS,
  validateUpdaterManifest,
} from "./verify-desktop-updater-manifest.mjs";

function platformEntry(suffix) {
  return {
    signature: `sig-${suffix}`,
    url: `https://github.com/ofirefish/runory/releases/download/v0.1.1/Runory_${suffix}.tar.gz`,
  };
}

function completeManifest(overrides = {}) {
  return {
    version: "0.1.1",
    notes: "test",
    pub_date: "2026-09-15T00:00:00Z",
    platforms: Object.fromEntries(
      REQUIRED_UPDATER_PLATFORMS.map((platform) => [platform, platformEntry(platform)]),
    ),
    ...overrides,
  };
}

test("accepts a complete Tauri updater manifest", () => {
  const result = validateUpdaterManifest(completeManifest(), { expectedVersion: "v0.1.1" });
  assert.equal(result.version, "0.1.1");
  assert.deepEqual(result.platforms, [...REQUIRED_UPDATER_PLATFORMS]);
});

test("rejects missing platforms", () => {
  const manifest = completeManifest();
  delete manifest.platforms["linux-x86_64"];
  assert.throws(
    () => validateUpdaterManifest(manifest),
    /missing platforms: linux-x86_64/,
  );
});

test("rejects blank signatures and non-https urls", () => {
  const manifest = completeManifest({
    platforms: {
      ...completeManifest().platforms,
      "windows-x86_64": { signature: " ", url: "http://example.com/a.zip" },
    },
  });
  assert.throws(
    () => validateUpdaterManifest(manifest),
    /invalid fields: windows-x86_64\.signature, windows-x86_64\.url/,
  );
});

test("rejects version mismatches", () => {
  assert.throws(
    () => validateUpdaterManifest(completeManifest(), { expectedVersion: "0.2.0" }),
    /expected 0\.2\.0/,
  );
});
