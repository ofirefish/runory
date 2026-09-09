// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import type { Session } from "@supabase/supabase-js";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import i18n from "../../i18n";
import {
  cloudSession,
  getCreditAccount,
  getOrganizationSubscription,
  listBillingPlans,
  listCreditLedger,
  listOrganizations,
  startBillingTrial,
} from "../../lib/supabase/cloud";
import { BillingPanel } from "./BillingPanel";
import type { BillingPlan } from "../../types/cloud";

vi.mock("../../lib/supabase/client", () => ({ cloudConfigured: true }));
vi.mock("sonner", () => ({ toast: { error: vi.fn(), success: vi.fn() } }));
vi.mock("../../lib/supabase/cloud", () => ({
  cloudSession: vi.fn(),
  getCreditAccount: vi.fn(),
  getOrganizationSubscription: vi.fn(),
  listBillingPlans: vi.fn(),
  listCreditLedger: vi.fn(),
  listOrganizations: vi.fn(),
  onCloudAuthStateChange: vi.fn(() => ({ unsubscribe: vi.fn() })),
  startBillingTrial: vi.fn(),
}));

let container: HTMLDivElement;
let root: Root;

const plans: BillingPlan[] = [
  { code: "free", monthly_price_cents: 0, annual_monthly_price_cents: 0, currency: "USD", per_seat: false, trial_days: 0, included_monthly_credits: 0, feature_codes: ["local-ssh", "personal-vault", "files-sftp"], sort_order: 0, active: true, created_at: "2026-01-01T00:00:00Z", updated_at: "2026-01-01T00:00:00Z" },
  { code: "pro", monthly_price_cents: 1200, annual_monthly_price_cents: 1000, currency: "USD", per_seat: false, trial_days: 0, included_monthly_credits: 10000, feature_codes: ["cloud-sync", "managed-ai"], sort_order: 1, active: true, created_at: "2026-01-01T00:00:00Z", updated_at: "2026-01-01T00:00:00Z" },
  { code: "team", monthly_price_cents: 2400, annual_monthly_price_cents: 2000, currency: "USD", per_seat: true, trial_days: 14, included_monthly_credits: 25000, feature_codes: ["team-workspaces", "consolidated-billing"], sort_order: 2, active: true, created_at: "2026-01-01T00:00:00Z", updated_at: "2026-01-01T00:00:00Z" },
  { code: "business", monthly_price_cents: 3600, annual_monthly_price_cents: 3000, currency: "USD", per_seat: true, trial_days: 14, included_monthly_credits: 50000, feature_codes: ["access-policies", "audit-history"], sort_order: 3, active: true, created_at: "2026-01-01T00:00:00Z", updated_at: "2026-01-01T00:00:00Z" },
];

beforeEach(async () => {
  vi.clearAllMocks();
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  await i18n.changeLanguage("zh-CN");
  vi.mocked(listBillingPlans).mockResolvedValue([...plans]);
  vi.mocked(getOrganizationSubscription).mockResolvedValue(null);
  vi.mocked(getCreditAccount).mockResolvedValue({
    organization_id: "organization-1", balance_microcredits: 2500000, held_microcredits: 500000,
    lifetime_granted_microcredits: 2500000, lifetime_spent_microcredits: 0, version: 1, updated_at: "2026-01-01T00:00:00Z",
  });
  vi.mocked(listCreditLedger).mockResolvedValue([]);
  container = document.createElement("div");
  document.body.append(container);
  root = createRoot(container);
});

afterEach(async () => {
  await act(async () => { root.unmount(); });
  container.remove();
  vi.unstubAllGlobals();
});

describe("billing settings", () => {
  it("shows the server pricing catalog without claiming unavailable enterprise features", async () => {
    vi.mocked(cloudSession).mockResolvedValue(null);
    await act(async () => { root.render(<BillingPanel />); await Promise.resolve(); });
    expect(container.textContent).toContain("Free");
    expect(container.textContent).toContain("Pro");
    expect(container.textContent).toContain("Team");
    expect(container.textContent).toContain("Business");
    expect(container.textContent).toContain("SSH 连接与交互式终端");
    expect(container.textContent).toContain("$10");
    expect(container.textContent).not.toContain("SAML");
    expect(container.textContent).not.toContain("SOC 2");
  });

  it("starts a real one-time team trial through the constrained billing RPC", async () => {
    vi.mocked(cloudSession).mockResolvedValue({ user: { id: "user-1" } } as unknown as Session);
    vi.mocked(listOrganizations).mockResolvedValue([{
      id: "organization-1", name: "Ops", kind: "team", owner_id: "user-1",
      created_at: "2026-01-01T00:00:00Z", updated_at: "2026-01-01T00:00:00Z",
    }]);
    vi.mocked(startBillingTrial).mockResolvedValue({
      organization_id: "organization-1", plan_code: "team", status: "trialing", seat_quantity: 1,
      billing_cycle: "annual", current_period_start: "2026-01-01T00:00:00Z", current_period_end: "2026-01-15T00:00:00Z",
      trial_ends_at: "2026-01-15T00:00:00Z", trial_started_at: "2026-01-01T00:00:00Z", cancel_at_period_end: false,
      provider_customer_ref: null, provider_subscription_ref: null, created_at: "2026-01-01T00:00:00Z", updated_at: "2026-01-01T00:00:00Z",
    });
    await act(async () => { root.render(<BillingPanel />); await Promise.resolve(); await Promise.resolve(); });
    const trial = Array.from(container.querySelectorAll("button")).find((button) => button.textContent?.includes("免费试用 14 天"));
    expect(trial).toBeTruthy();
    await act(async () => { trial?.click(); await Promise.resolve(); await Promise.resolve(); });
    expect(startBillingTrial).toHaveBeenCalledExactlyOnceWith("organization-1", "team");
  });
});
