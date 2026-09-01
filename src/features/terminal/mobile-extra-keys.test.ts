import { describe, expect, it } from "vitest";
import { MOBILE_EXTRA_KEYS } from "./mobile-extra-keys";

describe("mobile terminal extra keys", () => {
  it("maps navigation and control keys to terminal byte sequences", () => {
    const keys = new Map(MOBILE_EXTRA_KEYS);
    expect(keys.get("Esc")).toBe("\u001b");
    expect(keys.get("Ctrl+C")).toBe("\u0003");
    expect(keys.get("↑")).toBe("\u001b[A");
    expect(keys.get("PgDn")).toBe("\u001b[6~");
  });
});
