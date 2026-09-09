import { beforeEach, describe, expect, it, vi } from "vitest";

const { from, rpc } = vi.hoisted(() => ({ from: vi.fn(), rpc: vi.fn() }));

vi.mock("./client", () => ({
  supabase: { from, rpc },
}));

import { createOrganization, writeEncryptedInventory } from "./cloud";

describe("encrypted inventory writes", () => {
  beforeEach(() => {
    from.mockReset();
    rpc.mockReset();
  });

  it("binds the write to the revision observed before conflict review", async () => {
    const written = { id: "sync-object", revision: 8 };
    rpc.mockResolvedValue({ data: written, error: null });

    await expect(writeEncryptedInventory("organization", {
      version: 3,
      salt: [1],
      nonce: [2],
      ciphertext: [3],
    }, 7)).resolves.toBe(written);

    expect(rpc).toHaveBeenCalledWith("write_sync_object", expect.objectContaining({
      target_organization_id: "organization",
      expected_revision: 7,
    }));
  });
});

describe("team workspace creation", () => {
  beforeEach(() => {
    from.mockReset();
    rpc.mockReset();
  });

  it("waits for the owner membership trigger before selecting the workspace", async () => {
    const id = "10000000-0000-4000-8000-000000000001";
    const organization = {
      id,
      name: "Operations",
      kind: "team",
      owner_id: "user-1",
      created_at: "2026-09-06T00:00:00Z",
      updated_at: "2026-09-06T00:00:00Z",
    };
    const insert = vi.fn().mockResolvedValue({ error: null });
    const single = vi.fn().mockResolvedValue({ data: organization, error: null });
    const eq = vi.fn(() => ({ single }));
    const select = vi.fn(() => ({ eq }));
    from.mockReturnValueOnce({ insert }).mockReturnValueOnce({ select });
    vi.spyOn(crypto, "randomUUID").mockReturnValue(id);

    await expect(createOrganization("Operations", "user-1")).resolves.toEqual(organization);

    expect(insert).toHaveBeenCalledExactlyOnceWith({
      id,
      name: "Operations",
      kind: "team",
      owner_id: "user-1",
    });
    expect(select).toHaveBeenCalledExactlyOnceWith("id,name,kind,owner_id,created_at,updated_at");
    expect(eq).toHaveBeenCalledExactlyOnceWith("id", id);
  });
});
