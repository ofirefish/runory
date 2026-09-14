export const releaseUrl = "https://github.com/ofirefish/runory/releases";
export const githubRepo = "ofirefish/runory";

export type DownloadOs = "windows" | "macos" | "linux";
export type DownloadArch = "x64" | "arm64" | "universal";

export type DownloadAsset = {
  id: string;
  os: DownloadOs;
  arch: DownloadArch;
  labelKey: "windows" | "macosApple" | "macosIntel" | "linux";
  format: string;
  href: string;
  direct: boolean;
};

export type LatestReleaseDownloads = {
  version: string | null;
  assets: DownloadAsset[];
  releasePageUrl: string;
};

type GithubAsset = {
  name: string;
  browser_download_url: string;
};

type GithubRelease = {
  tag_name?: string;
  name?: string;
  assets?: GithubAsset[];
};

function isInstallerName(name: string): boolean {
  const lower = name.toLowerCase();
  if (lower.endsWith(".sig") || lower.endsWith(".json") || lower.includes(".tar.")) return false;
  if (lower.endsWith(".zip") || lower.endsWith(".app")) return false;
  return (
    lower.endsWith(".msi") ||
    lower.endsWith(".exe") ||
    lower.endsWith(".dmg") ||
    lower.endsWith(".appimage") ||
    lower.endsWith(".deb")
  );
}

function detectArch(name: string): DownloadArch {
  const lower = name.toLowerCase();
  if (/(aarch64|arm64|apple.?silicon)/.test(lower)) return "arm64";
  if (/(x86_64|amd64|x64|win64|x86-64)/.test(lower)) return "x64";
  return "universal";
}

function formatLabel(name: string): string {
  const lower = name.toLowerCase();
  if (lower.endsWith(".msi")) return ".msi";
  if (lower.endsWith(".exe")) return ".exe";
  if (lower.endsWith(".dmg")) return ".dmg";
  if (lower.endsWith(".appimage")) return ".AppImage";
  if (lower.endsWith(".deb")) return ".deb";
  return name.slice(name.lastIndexOf("."));
}

function pickPreferred(
  candidates: GithubAsset[],
  prefer: (name: string) => boolean,
): GithubAsset | undefined {
  return candidates.find(asset => prefer(asset.name.toLowerCase())) ?? candidates[0];
}

/** Pure mapper — exported for unit tests. */
export function mapReleaseAssets(assets: GithubAsset[], fallbackUrl = releaseUrl): DownloadAsset[] {
  const installers = assets.filter(asset => isInstallerName(asset.name));
  const windows = installers.filter(asset => {
    const lower = asset.name.toLowerCase();
    return lower.endsWith(".msi") || lower.endsWith(".exe");
  });
  const macos = installers.filter(asset => asset.name.toLowerCase().endsWith(".dmg"));
  const linux = installers.filter(asset => {
    const lower = asset.name.toLowerCase();
    return lower.endsWith(".appimage") || lower.endsWith(".deb");
  });

  const rows: DownloadAsset[] = [];

  const windowsPick =
    windows.find(asset => asset.name.toLowerCase().endsWith(".msi")) ??
    windows.find(asset => /[_-]setup\.exe$/i.test(asset.name)) ??
    windows[0];
  rows.push({
    id: "windows",
    os: "windows",
    arch: windowsPick ? detectArch(windowsPick.name) : "x64",
    labelKey: "windows",
    format: windowsPick ? formatLabel(windowsPick.name) : ".msi / .exe",
    href: windowsPick?.browser_download_url ?? fallbackUrl,
    direct: Boolean(windowsPick),
  });

  const appleSilicon = macos.find(asset => detectArch(asset.name) === "arm64");
  const intelMac = macos.find(asset => detectArch(asset.name) === "x64");
  const macFallback = !appleSilicon && !intelMac ? macos[0] : undefined;
  const appleAsset = appleSilicon ?? macFallback;
  const intelAsset = intelMac;

  rows.push({
    id: "macos-apple",
    os: "macos",
    arch: "arm64",
    labelKey: "macosApple",
    format: appleAsset ? formatLabel(appleAsset.name) : ".dmg",
    href: appleAsset?.browser_download_url ?? fallbackUrl,
    direct: Boolean(appleAsset),
  });

  rows.push({
    id: "macos-intel",
    os: "macos",
    arch: "x64",
    labelKey: "macosIntel",
    format: intelAsset ? formatLabel(intelAsset.name) : ".dmg",
    href: intelAsset?.browser_download_url ?? fallbackUrl,
    direct: Boolean(intelAsset),
  });

  const linuxPick = pickPreferred(linux, name => name.endsWith(".appimage"));
  rows.push({
    id: "linux",
    os: "linux",
    arch: linuxPick ? detectArch(linuxPick.name) : "x64",
    labelKey: "linux",
    format: linuxPick ? formatLabel(linuxPick.name) : ".AppImage / .deb",
    href: linuxPick?.browser_download_url ?? fallbackUrl,
    direct: Boolean(linuxPick),
  });

  return rows;
}

export async function fetchLatestReleaseDownloads(): Promise<LatestReleaseDownloads> {
  const headers: HeadersInit = {
    Accept: "application/vnd.github+json",
    "User-Agent": "runory-website",
  };
  const token = process.env.GITHUB_TOKEN;
  if (token) headers.Authorization = `Bearer ${token}`;

  try {
    const response = await fetch(`https://api.github.com/repos/${githubRepo}/releases/latest`, {
      headers,
      next: { revalidate: 3600 },
    });
    if (!response.ok) {
      return { version: null, assets: mapReleaseAssets([]), releasePageUrl: releaseUrl };
    }
    const release = (await response.json()) as GithubRelease;
    const version = release.tag_name ?? release.name ?? null;
    return {
      version,
      assets: mapReleaseAssets(release.assets ?? []),
      releasePageUrl: releaseUrl,
    };
  } catch {
    return { version: null, assets: mapReleaseAssets([]), releasePageUrl: releaseUrl };
  }
}

export function primaryDownloadHref(downloads: LatestReleaseDownloads, os: DownloadOs): string {
  if (os === "macos") {
    const apple = downloads.assets.find(asset => asset.id === "macos-apple" && asset.direct);
    if (apple) return apple.href;
  }
  const match = downloads.assets.find(asset => asset.os === os && asset.direct);
  return match?.href ?? downloads.releasePageUrl;
}
