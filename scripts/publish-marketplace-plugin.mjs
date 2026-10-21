#!/usr/bin/env node
/**
 * Publish a signed .runory-plugin to the website Marketplace API.
 *
 * Usage:
 *   node scripts/publish-marketplace-plugin.mjs \
 *     --url https://runory.app \
 *     --token "$MARKETPLACE_ADMIN_TOKEN" \
 *     --package path/to/plugin.runory-plugin \
 *     --plugin-id runory.hello \
 *     --publisher-id runory \
 *     --name "Runory Hello" \
 *     --version 1.0.0 \
 *     [--verified]
 */
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

function arg(name, fallback = undefined) {
  const index = process.argv.indexOf(`--${name}`);
  if (index === -1) return fallback;
  return process.argv[index + 1] ?? fallback;
}

const baseUrl = (arg("url") || "https://runory.app").replace(/\/$/, "");
const token = arg("token") || process.env.MARKETPLACE_ADMIN_TOKEN;
const packagePath = arg("package");
const pluginId = arg("plugin-id");
const publisherId = arg("publisher-id", "runory");
const name = arg("name");
const version = arg("version");
const description = arg("description", "");
const category = arg("category", "Development");
const verified = process.argv.includes("--verified");

if (!token || !packagePath || !pluginId || !name || !version) {
  console.error("Missing required --token/--package/--plugin-id/--name/--version");
  process.exit(1);
}

const bytes = readFileSync(resolve(packagePath));
const packageBase64 = bytes.toString("base64");
const body = {
  pluginId,
  publisherId,
  name,
  description: description || null,
  category: category || null,
  version,
  apiVersion: "1",
  minRunoryVersion: "0.1.0",
  changelog: "Official marketplace listing",
  permissions: [],
  verified,
  packageBase64,
};

const response = await fetch(`${baseUrl}/api/v1/admin/publish`, {
  method: "POST",
  headers: {
    "Content-Type": "application/json",
    "x-runory-marketplace-token": token,
  },
  body: JSON.stringify(body),
});

const text = await response.text();
if (!response.ok) {
  console.error(`publish failed ${response.status}: ${text}`);
  process.exit(1);
}
console.log(text);
