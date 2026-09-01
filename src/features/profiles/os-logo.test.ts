import { describe, expect, it } from "vitest";
import { osLogoDictionary } from "./os-logo-data";
import type { OsDistribution } from "../../types/domain";

describe("OS logo dictionary", () => {
  it("contains a local logo definition for every detected distribution", () => {
    const distributions: OsDistribution[] = ["ubuntu", "debian", "fedora", "centos", "red-hat", "rocky-linux", "alma-linux", "arch-linux", "manjaro", "open-suse", "alpine-linux", "amazon-linux", "oracle-linux", "linux-mint", "kali-linux", "gentoo", "void-linux", "nix-os", "mac-os", "free-bsd", "linux"];
    expect(Object.keys(osLogoDictionary).sort()).toEqual([...distributions].sort());
    expect(Object.values(osLogoDictionary).every((logo) => logo.label && logo.asset.length > 0 && logo.color)).toBe(true);
  });
});
