import { describe, expect, it } from "vitest";
import { avatarInitials } from "./avatar-initials";

describe("avatarInitials", () => {
  it("uses one leading character for a single name", () => {
    expect(avatarInitials("lichao")).toBe("L");
    expect(avatarInitials("33219778")).toBe("3");
    expect(avatarInitials("张三")).toBe("张");
  });

  it("uses first and last initials for multi-word names", () => {
    expect(avatarInitials("Alice Wonder")).toBe("AW");
    expect(avatarInitials("  bob   smith ")).toBe("BS");
  });

  it("falls back for empty values", () => {
    expect(avatarInitials("")).toBe("?");
    expect(avatarInitials("   ")).toBe("?");
  });
});
