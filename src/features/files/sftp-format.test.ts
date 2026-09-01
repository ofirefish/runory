import { describe, expect, it } from "vitest";
import { breadcrumbPaths, formatFileSize, formatPermissions } from "./sftp-format";

describe("SFTP presentation formatting", () => {
  it("builds absolute POSIX breadcrumb targets", () => {
    expect(breadcrumbPaths("/home/runory/projects")).toEqual([
      { label: "/", path: "/" },
      { label: "home", path: "/home" },
      { label: "runory", path: "/home/runory" },
      { label: "projects", path: "/home/runory/projects" },
    ]);
  });

  it("formats raw SFTP metadata without changing it", () => {
    expect(formatPermissions(0o100755)).toBe("0755");
    expect(formatPermissions(null)).toBe("—");
    expect(formatFileSize(1536, "en-US")).toBe("1.5 KB");
  });
});
