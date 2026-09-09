import { describe, expect, it } from "vitest";
import { AVATAR_MAX_SOURCE_BYTES, validateAvatarFile } from "./cloud-account";

describe("cloud account avatar validation", () => {
  it("accepts bounded raster image formats", () => {
    expect(validateAvatarFile({ type: "image/webp", size: 1024 })).toBeNull();
    expect(validateAvatarFile({ type: "image/png", size: AVATAR_MAX_SOURCE_BYTES })).toBeNull();
  });

  it("rejects active and unsupported content", () => {
    expect(validateAvatarFile({ type: "image/svg+xml", size: 1024 })).toBe("AVATAR_TYPE_UNSUPPORTED");
    expect(validateAvatarFile({ type: "text/html", size: 1024 })).toBe("AVATAR_TYPE_UNSUPPORTED");
  });

  it("rejects empty and oversized files", () => {
    expect(validateAvatarFile({ type: "image/png", size: 0 })).toBe("AVATAR_TOO_LARGE");
    expect(validateAvatarFile({ type: "image/jpeg", size: AVATAR_MAX_SOURCE_BYTES + 1 })).toBe("AVATAR_TOO_LARGE");
  });
});

