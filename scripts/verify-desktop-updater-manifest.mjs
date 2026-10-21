import { readFileSync } from "node:fs";
import { fileURLToPath, pathToFileURL } from "node:url";

const root = fileURLToPath(new URL("../", import.meta.url));

/** Platforms produced by `.github/workflows/publish.yml`. */
export const REQUIRED_UPDATER_PLATFORMS = Object.freeze([
  "windows-x86_64",
  "darwin-aarch64",
  "darwin-x86_64",
  "linux-x86_64",
]);

function fail(message) {
  throw new Error(message);
}

function isRecord(value) {
  return value !== null && typeof value === "object" && !Array.isArray(value);
}

function isNonEmptyString(value) {
  return typeof value === "string" && value.trim().length > 0;
}

function normalizeVersion(value) {
  return String(value ?? "")
    .trim()
    .replace(/^v/i, "");
}

/**
 * Validate a Tauri static updater manifest (`latest.json`).
 * @param {unknown} manifest
 * @param {{ expectedVersion?: string }} [options]
 */
export function validateUpdaterManifest(manifest, { expectedVersion } = {}) {
  if (!isRecord(manifest)) fail("updater manifest must be a JSON object");
  if (!isNonEmptyString(manifest.version)) fail("updater manifest.version is required");

  if (expectedVersion) {
    const actual = normalizeVersion(manifest.version);
    const expected = normalizeVersion(expectedVersion);
    if (actual !== expected) {
      fail(`updater manifest.version is ${manifest.version}, expected ${expectedVersion}`);
    }
  }

  if (!isRecord(manifest.platforms)) fail("updater manifest.platforms is required");

  const missing = [];
  const invalid = [];
  for (const platform of REQUIRED_UPDATER_PLATFORMS) {
    const entry = manifest.platforms[platform];
    if (!isRecord(entry)) {
      missing.push(platform);
      continue;
    }
    if (!isNonEmptyString(entry.signature)) {
      invalid.push(`${platform}.signature`);
    }
    if (!isNonEmptyString(entry.url) || !/^https:\/\//i.test(entry.url.trim())) {
      invalid.push(`${platform}.url`);
    }
  }

  if (missing.length > 0) {
    fail(`updater manifest missing platforms: ${missing.join(", ")}`);
  }
  if (invalid.length > 0) {
    fail(`updater manifest invalid fields: ${invalid.join(", ")}`);
  }

  return {
    version: String(manifest.version).trim(),
    platforms: [...REQUIRED_UPDATER_PLATFORMS],
  };
}

function parseRepo(repository) {
  const [owner, repo] = String(repository ?? "").split("/");
  if (!owner || !repo) fail("GITHUB_REPOSITORY must be owner/repo");
  return { owner, repo };
}

function githubHeaders(token) {
  const headers = {
    Accept: "application/vnd.github+json",
    "X-GitHub-Api-Version": "2022-11-28",
    "User-Agent": "runory-verify-desktop-updater-manifest",
  };
  if (token) headers.Authorization = `Bearer ${token}`;
  return headers;
}

async function githubJson(url, token, init = {}) {
  const response = await fetch(url, {
    ...init,
    headers: {
      ...githubHeaders(token),
      ...(init.headers ?? {}),
    },
  });
  const text = await response.text();
  let body = null;
  if (text) {
    try {
      body = JSON.parse(text);
    } catch {
      body = text;
    }
  }
  if (!response.ok) {
    const detail = typeof body === "object" && body && "message" in body ? body.message : text;
    fail(`GitHub API ${response.status} for ${url}: ${detail}`);
  }
  return body;
}

/**
 * Find a release (including drafts) by tag. Requires a token that can read drafts.
 */
export async function findReleaseByTag({ owner, repo, tag, token }) {
  if (!isNonEmptyString(tag)) fail("release tag is required");
  if (!isNonEmptyString(token)) {
    fail("GITHUB_TOKEN is required to inspect draft releases");
  }

  const encoded = encodeURIComponent(tag);
  try {
    return await githubJson(
      `https://api.github.com/repos/${owner}/${repo}/releases/tags/${encoded}`,
      token,
    );
  } catch (error) {
    // Drafts are sometimes absent from /releases/tags; fall back to listing.
    const message = error instanceof Error ? error.message : String(error);
    if (!message.includes("404")) throw error;
  }

  let page = 1;
  while (page <= 10) {
    const releases = await githubJson(
      `https://api.github.com/repos/${owner}/${repo}/releases?per_page=100&page=${page}`,
      token,
    );
    if (!Array.isArray(releases) || releases.length === 0) break;
    const match = releases.find((release) => release?.tag_name === tag);
    if (match) return match;
    if (releases.length < 100) break;
    page += 1;
  }

  fail(`no GitHub release found for tag ${tag} (including drafts)`);
}

export async function downloadReleaseAssetText({ owner, repo, asset, token }) {
  if (!asset?.id) fail("release asset id is required");
  const response = await fetch(
    `https://api.github.com/repos/${owner}/${repo}/releases/assets/${asset.id}`,
    {
      headers: {
        ...githubHeaders(token),
        Accept: "application/octet-stream",
      },
      redirect: "follow",
    },
  );
  const text = await response.text();
  if (!response.ok) {
    fail(`failed to download release asset ${asset.name}: HTTP ${response.status}`);
  }
  return text;
}

export async function publishGithubRelease({ owner, repo, releaseId, token }) {
  if (!releaseId) fail("release id is required");
  return githubJson(
    `https://api.github.com/repos/${owner}/${repo}/releases/${releaseId}`,
    token,
    {
      method: "PATCH",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ draft: false }),
    },
  );
}

/**
 * Verify that a GitHub Release (draft or published) has a complete latest.json.
 * Optionally publish the release so `/releases/latest/download/latest.json` resolves.
 */
export async function verifyDesktopUpdaterRelease({
  repository,
  tag,
  token,
  publish = false,
  expectedVersion,
} = {}) {
  const { owner, repo } = parseRepo(repository);
  const release = await findReleaseByTag({ owner, repo, tag, token });
  const assets = Array.isArray(release.assets) ? release.assets : [];
  const latestAsset = assets.find((asset) => asset?.name === "latest.json");
  if (!latestAsset) {
    fail(
      `release ${tag} has no latest.json asset yet. Wait for all publish-tauri matrix jobs to finish, or re-run publish.`,
    );
  }

  const raw = await downloadReleaseAssetText({
    owner,
    repo,
    asset: latestAsset,
    token,
  });
  let manifest;
  try {
    manifest = JSON.parse(raw);
  } catch {
    fail("latest.json is not valid JSON");
  }

  const versionHint = expectedVersion ?? normalizeVersion(tag);
  const summary = validateUpdaterManifest(manifest, { expectedVersion: versionHint });

  let published = !release.draft;
  if (publish && release.draft) {
    const updated = await publishGithubRelease({
      owner,
      repo,
      releaseId: release.id,
      token,
    });
    published = updated?.draft === false;
  }

  return {
    tag: release.tag_name,
    releaseId: release.id,
    draft: release.draft === true && !published,
    published,
    htmlUrl: release.html_url,
    version: summary.version,
    platforms: summary.platforms,
    latestJsonUrl: published
      ? `https://github.com/${owner}/${repo}/releases/latest/download/latest.json`
      : latestAsset.browser_download_url,
  };
}

function readPackageVersion() {
  const packageJson = JSON.parse(readFileSync(`${root}/package.json`, "utf8"));
  return normalizeVersion(packageJson.version);
}

function parseArgs(argv) {
  const options = {
    tag: null,
    publish: false,
    expectedVersion: null,
    repository: process.env.GITHUB_REPOSITORY ?? null,
    token: process.env.GITHUB_TOKEN ?? process.env.GH_TOKEN ?? null,
  };

  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    if (arg === "--publish") {
      options.publish = true;
      continue;
    }
    if (arg === "--tag") {
      options.tag = argv[index + 1] ?? null;
      index += 1;
      continue;
    }
    if (arg === "--expected-version") {
      options.expectedVersion = argv[index + 1] ?? null;
      index += 1;
      continue;
    }
    if (arg === "--repository") {
      options.repository = argv[index + 1] ?? null;
      index += 1;
      continue;
    }
    fail(`unknown argument: ${arg}`);
  }

  return options;
}

async function main(argv = process.argv.slice(2)) {
  const options = parseArgs(argv);
  const version = options.expectedVersion ?? readPackageVersion();
  const tag = options.tag ?? `v${version}`;

  const result = await verifyDesktopUpdaterRelease({
    repository: options.repository,
    tag,
    token: options.token,
    publish: options.publish,
    expectedVersion: version,
  });

  console.log(
    JSON.stringify(
      {
        ok: true,
        ...result,
      },
      null,
      2,
    ),
  );

  if (result.draft) {
    console.error(
      "::warning::Release is still a draft. Desktop updater checks will 404 until it is published.",
    );
  }
}

const isDirectRun =
  process.argv[1] && pathToFileURL(process.argv[1]).href === import.meta.url;

if (isDirectRun) {
  main().catch((error) => {
    console.error(error instanceof Error ? error.message : error);
    process.exitCode = 1;
  });
}
