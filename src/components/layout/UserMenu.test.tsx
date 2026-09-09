// @vitest-environment jsdom
import type { Session } from "@supabase/supabase-js";
import { act, type ComponentProps } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import i18n from "../../i18n";
import { loadCloudAvatar } from "../../lib/cloud-avatar";
import { cloudProfileUpdatedEvent, loadMyCloudProfile } from "../../lib/supabase/cloud";
import type { CloudUserProfile } from "../../types/cloud";
import { UserMenu } from "./UserMenu";

const session = {
  access_token: "fixture-access-token",
  user: { id: "user-1", email: "person@example.test" },
} as unknown as Session;

const profile = (avatarPath: string): CloudUserProfile => ({
  id: "user-1",
  display_name: "Person",
  avatar_path: avatarPath,
  avatar_version: 1,
  created_at: "2026-01-01T00:00:00Z",
  updated_at: "2026-01-01T00:00:00Z",
});

vi.mock("../../lib/supabase/client", () => ({ cloudConfigured: true }));
vi.mock("../../lib/supabase/cloud", () => ({
  cloudProfileUpdatedEvent: "runory:cloud-profile-updated",
  cloudSession: vi.fn(),
  cloudSignOut: vi.fn(),
  loadMyCloudProfile: vi.fn(),
  onCloudAuthStateChange: vi.fn(() => ({ unsubscribe: vi.fn() })),
}));
vi.mock("../../lib/cloud-avatar", () => ({
  loadCloudAvatar: vi.fn(),
  loadLatestCachedCloudAvatar: vi.fn().mockResolvedValue(null),
}));
vi.mock("../../lib/tauri/cloud-policy", () => ({ lockCloudPolicy: vi.fn() }));
vi.mock("../ui/popover", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../ui/popover")>();
  return { ...actual, PopoverContent: (props: ComponentProps<typeof actual.PopoverContent>) => <actual.PopoverContent {...props} avoidCollisions={false} /> };
});

let container: HTMLDivElement;
let root: Root;

beforeEach(async () => {
  vi.clearAllMocks();
  const { cloudSession } = await import("../../lib/supabase/cloud");
  vi.mocked(cloudSession).mockResolvedValue(session);
  vi.mocked(loadMyCloudProfile).mockResolvedValue(profile("avatar-1.webp"));
  vi.mocked(loadCloudAvatar).mockImplementation(async (_userId, path) => `data:image/webp;base64,${path}`);
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  vi.stubGlobal("ResizeObserver", class { observe() {} unobserve() {} disconnect() {} });
  container = document.createElement("div");
  document.body.append(container);
  root = createRoot(container);
  await act(async () => {
    root.render(<UserMenu onOpenAccount={() => undefined} onOpenAuth={() => undefined} onOpenSettings={() => undefined} onOpenPricing={() => undefined} />);
    await Promise.resolve();
    await Promise.resolve();
  });
});

afterEach(async () => {
  await act(async () => { root.unmount(); });
  container.remove();
  vi.unstubAllGlobals();
});

it("does not request a new signed avatar URL when the menu is toggled", async () => {
  expect(loadCloudAvatar).toHaveBeenCalledOnce();
  const trigger = container.querySelector<HTMLButtonElement>(`[aria-label="${i18n.t("userMenu.open")}"]`)!;
  await act(async () => { trigger.click(); });
  await act(async () => { trigger.click(); });
  expect(loadCloudAvatar).toHaveBeenCalledOnce();
});

it("shows the subscription plan menu item", async () => {
  const trigger = container.querySelector<HTMLButtonElement>(`[aria-label="${i18n.t("userMenu.open")}"]`)!;
  await act(async () => { trigger.click(); });
  expect(document.body.textContent).toContain(i18n.t("userMenu.subscription"));
  expect(document.body.textContent).toContain(i18n.t("userMenu.subscriptionHint"));
});
it("refreshes the avatar after an actual profile update", async () => {
  vi.mocked(loadMyCloudProfile).mockResolvedValue(profile("avatar-2.webp"));
  await act(async () => {
    window.dispatchEvent(new Event(cloudProfileUpdatedEvent));
    await Promise.resolve();
    await Promise.resolve();
  });
  expect(loadCloudAvatar).toHaveBeenCalledTimes(2);
  expect(container.querySelector<HTMLImageElement>(".rail-avatar img")?.src).toBe("data:image/webp;base64,avatar-2.webp");
});
