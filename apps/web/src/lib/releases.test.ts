import { describe, expect, it } from "vitest";
import { releaseUrl } from "./github-releases";
import { isHttpsUrl, mapReleaseToDownloads, selectDownloadCatalog } from "./releases";

describe("isHttpsUrl", () => {
  it("accepts https URLs", () => {
    expect(isHttpsUrl("https://github.com/ofirefish/runory/releases/download/v0.1.0/app.msi")).toBe(true);
  });

  it("rejects non-https schemes and invalid values", () => {
    expect(isHttpsUrl("http://example.com/app.msi")).toBe(false);
    expect(isHttpsUrl("javascript:alert(1)")).toBe(false);
    expect(isHttpsUrl("data:text/plain,hi")).toBe(false);
    expect(isHttpsUrl("not-a-url")).toBe(false);
  });
});

describe("mapReleaseToDownloads", () => {
  it("maps registered assets into download catalog rows", () => {
    const downloads = mapReleaseToDownloads({
      version: "0.2.0",
      releasePageUrl: "https://github.com/ofirefish/runory/releases/tag/v0.2.0",
      assets: [
        { platform: "windows", format: ".msi", downloadUrl: "https://example.com/win.msi" },
        { platform: "macos_apple", format: ".dmg", downloadUrl: "https://example.com/mac-arm.dmg" },
        { platform: "macos_intel", format: ".dmg", downloadUrl: "https://example.com/mac-x64.dmg" },
        { platform: "linux", format: ".AppImage", downloadUrl: "https://example.com/linux.AppImage" },
      ],
    });

    expect(downloads.version).toBe("0.2.0");
    expect(downloads.releasePageUrl).toBe("https://github.com/ofirefish/runory/releases/tag/v0.2.0");
    expect(downloads.assets).toEqual([
      expect.objectContaining({ id: "windows", href: "https://example.com/win.msi", format: ".msi", direct: true }),
      expect.objectContaining({ id: "macos-apple", href: "https://example.com/mac-arm.dmg", direct: true }),
      expect.objectContaining({ id: "macos-intel", href: "https://example.com/mac-x64.dmg", direct: true }),
      expect.objectContaining({ id: "linux", href: "https://example.com/linux.AppImage", format: ".AppImage", direct: true }),
    ]);
  });

  it("falls back to the release page when an asset URL is missing or unsafe", () => {
    const downloads = mapReleaseToDownloads({
      version: "0.2.0",
      releasePageUrl: null,
      assets: [{ platform: "windows", format: ".msi", downloadUrl: "http://insecure.example/win.msi" }],
    });

    expect(downloads.releasePageUrl).toBe(releaseUrl);
    expect(downloads.assets.find(asset => asset.id === "windows")).toEqual(
      expect.objectContaining({ href: releaseUrl, direct: false }),
    );
    expect(downloads.assets.find(asset => asset.id === "linux")).toEqual(
      expect.objectContaining({ href: releaseUrl, direct: false }),
    );
  });
});

describe("selectDownloadCatalog", () => {
  it("prefers the published catalog when present", () => {
    const published = { version: "0.2.0", assets: [], releasePageUrl: releaseUrl };
    const github = { version: "0.1.0", assets: [], releasePageUrl: releaseUrl };
    expect(selectDownloadCatalog(published, github)).toBe(published);
  });

  it("falls back to GitHub when no published catalog exists", () => {
    const github = { version: "0.1.0", assets: [], releasePageUrl: releaseUrl };
    expect(selectDownloadCatalog(null, github)).toBe(github);
  });
});
