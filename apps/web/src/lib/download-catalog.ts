import "server-only";

import "server-only";

import { fetchLatestReleaseDownloads, type LatestReleaseDownloads } from "./github-releases";
import { getPublishedLatestDownloads } from "./data/releases";

/** Prefer the published admin catalog; fall back to GitHub Releases. */
export async function fetchDownloadCatalog(): Promise<LatestReleaseDownloads> {
  const published = await getPublishedLatestDownloads();
  if (published) return published;
  return fetchLatestReleaseDownloads();
}
