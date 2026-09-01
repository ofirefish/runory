import { describe, expect, it } from "vitest";
import { clearLastRemotePath, readLastRemotePath, saveLastRemotePath } from "./remote-path-history";

function memoryStorage() {
  const values = new Map<string, string>();
  return {
    getItem: (key: string) => values.get(key) ?? null,
    setItem: (key: string, value: string) => { values.set(key, value); },
    removeItem: (key: string) => { values.delete(key); },
  };
}

describe("remote path history", () => {
  it("stores paths independently for each profile", () => {
    const storage = memoryStorage();
    saveLastRemotePath("profile-a", "/srv/app", storage);
    saveLastRemotePath("profile-b", "/home/runory", storage);

    expect(readLastRemotePath("profile-a", storage)).toBe("/srv/app");
    expect(readLastRemotePath("profile-b", storage)).toBe("/home/runory");
  });

  it("ignores invalid paths and clears stale history", () => {
    const storage = memoryStorage();
    saveLastRemotePath("profile-a", "relative/path", storage);
    expect(readLastRemotePath("profile-a", storage)).toBeNull();

    saveLastRemotePath("profile-a", "/srv/app", storage);
    clearLastRemotePath("profile-a", storage);
    expect(readLastRemotePath("profile-a", storage)).toBeNull();
  });
});
