import { describe, expect, it } from "vitest";
import { resolveFileTypeIcon } from "./file-type-icon";

describe("resolveFileTypeIcon", () => {
  it.each([
    ["photo.JPG", "image"],
    ["release.tar.gz", "archive"],
    ["report.xlsx", "spreadsheet"],
    ["slides.pptx", "powerpoint"],
    ["component.tsx", "react"],
    ["script.sh", "shell"],
    ["database.sqlite3", "database"],
    ["server.log", "log"],
    ["id_ed25519.pub", "key"],
  ] as const)("maps %s to %s", (fileName, expected) => {
    expect(resolveFileTypeIcon(fileName)).toBe(expected);
  });

  it.each([
    ["Dockerfile", "docker"],
    ["docker-compose.yml", "docker"],
    [".env.production", "settings"],
    ["Cargo.toml", "rust"],
    ["LICENSE", "license"],
    [".gitignore", "git"],
  ] as const)("recognizes special file name %s", (fileName, expected) => {
    expect(resolveFileTypeIcon(fileName)).toBe(expected);
  });

  it("uses the document icon for unknown or extensionless files", () => {
    expect(resolveFileTypeIcon("README.unknown")).toBe("document");
    expect(resolveFileTypeIcon("README")).toBe("document");
  });
});
