import type { DownloadArch, DownloadAsset, DownloadOs, LatestReleaseDownloads } from "./github-releases";
import { releaseUrl as defaultReleaseUrl } from "./github-releases";

export const releasePlatforms = ["windows", "macos_apple", "macos_intel", "linux"] as const;
export type ReleasePlatform = (typeof releasePlatforms)[number];
export type ReleaseStatus = "draft" | "published" | "archived";

export type ReleaseAssetInput = {
  platform: ReleasePlatform;
  format: string;
  downloadUrl: string;
};

export type ReleaseRecord = {
  id: string;
  version: string;
  status: ReleaseStatus;
  isLatest: boolean;
  notesZh: string | null;
  notesEn: string | null;
  releasePageUrl: string | null;
  publishedAt: string | null;
  createdAt: string;
  updatedAt: string;
  assets: ReleaseAssetInput[];
};

const platformMeta: Record<
  ReleasePlatform,
  { id: string; os: DownloadOs; arch: DownloadArch; labelKey: DownloadAsset["labelKey"]; defaultFormat: string }
> = {
  windows: { id: "windows", os: "windows", arch: "x64", labelKey: "windows", defaultFormat: ".msi" },
  macos_apple: { id: "macos-apple", os: "macos", arch: "arm64", labelKey: "macosApple", defaultFormat: ".dmg" },
  macos_intel: { id: "macos-intel", os: "macos", arch: "x64", labelKey: "macosIntel", defaultFormat: ".dmg" },
  linux: { id: "linux", os: "linux", arch: "x64", labelKey: "linux", defaultFormat: ".AppImage" },
};

export function isHttpsUrl(value: string): boolean {
  try {
    const url = new URL(value);
    return url.protocol === "https:";
  } catch {
    return false;
  }
}

export function normalizeVersion(value: string): string {
  return value.trim();
}

export function mapReleaseToDownloads(release: Pick<ReleaseRecord, "version" | "releasePageUrl" | "assets">): LatestReleaseDownloads {
  const fallback = release.releasePageUrl && isHttpsUrl(release.releasePageUrl) ? release.releasePageUrl : defaultReleaseUrl;
  const byPlatform = new Map(release.assets.map(asset => [asset.platform, asset]));

  const assets: DownloadAsset[] = releasePlatforms.map(platform => {
    const meta = platformMeta[platform];
    const asset = byPlatform.get(platform);
    const href = asset && isHttpsUrl(asset.downloadUrl) ? asset.downloadUrl : fallback;
    return {
      id: meta.id,
      os: meta.os,
      arch: meta.arch,
      labelKey: meta.labelKey,
      format: asset?.format?.trim() || meta.defaultFormat,
      href,
      direct: Boolean(asset && isHttpsUrl(asset.downloadUrl)),
    };
  });

  return {
    version: release.version,
    assets,
    releasePageUrl: fallback,
  };
}

export function selectDownloadCatalog<T>(published: T | null, fallback: T): T {
  return published ?? fallback;
}
