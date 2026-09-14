import { describe, expect, it } from "vitest";
import { mapReleaseAssets, primaryDownloadHref, releaseUrl } from "./github-releases";

describe("mapReleaseAssets", () => {
  it("picks preferred installers per platform and architecture", () => {
    const assets = mapReleaseAssets([
      { name: "Runory_0.1.0_x64-setup.exe", browser_download_url: "https://example.com/win.exe" },
      { name: "Runory_0.1.0_x64_en-US.msi", browser_download_url: "https://example.com/win.msi" },
      { name: "Runory_0.1.0_aarch64.dmg", browser_download_url: "https://example.com/mac-arm.dmg" },
      { name: "Runory_0.1.0_x64.dmg", browser_download_url: "https://example.com/mac-x64.dmg" },
      { name: "Runory_0.1.0_amd64.AppImage", browser_download_url: "https://example.com/linux.AppImage" },
      { name: "Runory_0.1.0_amd64.deb", browser_download_url: "https://example.com/linux.deb" },
      { name: "Runory_0.1.0_x64.msi.sig", browser_download_url: "https://example.com/ignore.sig" },
      { name: "latest.json", browser_download_url: "https://example.com/latest.json" },
    ]);

    expect(assets).toEqual([
      expect.objectContaining({ id: "windows", href: "https://example.com/win.msi", format: ".msi", direct: true }),
      expect.objectContaining({ id: "macos-apple", href: "https://example.com/mac-arm.dmg", direct: true }),
      expect.objectContaining({ id: "macos-intel", href: "https://example.com/mac-x64.dmg", direct: true }),
      expect.objectContaining({ id: "linux", href: "https://example.com/linux.AppImage", format: ".AppImage", direct: true }),
    ]);
  });

  it("falls back to the releases page when assets are missing", () => {
    const assets = mapReleaseAssets([]);
    expect(assets.every(asset => asset.href === releaseUrl && asset.direct === false)).toBe(true);
  });
});

describe("primaryDownloadHref", () => {
  it("prefers Apple Silicon for macOS", () => {
    const downloads = {
      version: "v0.1.0",
      releasePageUrl: releaseUrl,
      assets: mapReleaseAssets([
        { name: "Runory_0.1.0_x64.dmg", browser_download_url: "https://example.com/mac-x64.dmg" },
        { name: "Runory_0.1.0_aarch64.dmg", browser_download_url: "https://example.com/mac-arm.dmg" },
      ]),
    };
    expect(primaryDownloadHref(downloads, "macos")).toBe("https://example.com/mac-arm.dmg");
  });
});
