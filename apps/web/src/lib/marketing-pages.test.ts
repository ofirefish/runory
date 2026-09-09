import { describe, expect, it } from "vitest";
import robots from "@/app/robots";
import sitemap from "@/app/sitemap";
import { locales } from "./i18n";
import { getMarketingCopy, localizedPath, marketingMetadata, marketingSlugs, siteUrl } from "./marketing-pages";

describe("marketing SEO", () => {
  it("provides complete localized pages with unique canonical URLs", () => {
    const canonical = new Set<string>();
    for (const locale of locales) {
      const copy = getMarketingCopy(locale);
      for (const slug of marketingSlugs) {
        const page = copy.pages[slug];
        const metadata = marketingMetadata(locale, slug);
        expect(page.meta.title.length).toBeGreaterThan(20);
        expect(page.meta.description.length).toBeGreaterThan(60);
        expect(page.sections).toHaveLength(3);
        expect(page.faq).toHaveLength(3);
        expect(metadata.alternates?.canonical).toBe(localizedPath(locale, slug));
        expect(metadata.alternates?.languages).toMatchObject({ "zh-CN": `/zh-CN/${slug}`, "en-US": `/en-US/${slug}` });
        canonical.add(String(metadata.alternates?.canonical));
      }
    }
    expect(canonical.size).toBe(locales.length * marketingSlugs.length);
  });

  it("publishes only public marketing routes in the sitemap", () => {
    const entries = sitemap();
    const urls = entries.map(entry => entry.url);
    expect(entries).toHaveLength(locales.length * (marketingSlugs.length + 1));
    expect(new Set(urls).size).toBe(urls.length);
    expect(urls).toContain(`${siteUrl}/zh-CN/security`);
    expect(urls).toContain(`${siteUrl}/en-US/download`);
    expect(urls).toContain(`${siteUrl}/zh-CN/pricing`);
    expect(urls.some(url => url.includes("/auth/") || url.includes("/account") || url.includes("/admin"))).toBe(false);
  });

  it("keeps pricing localized and honest about checkout availability", () => {
    for (const locale of locales) {
      const pricing = getMarketingCopy(locale).pages.pricing;
      expect(pricing.plans.items.map(plan => plan.code)).toEqual(["free", "pro", "team", "business"]);
      expect(pricing.plans.items[0]).toMatchObject({ monthlyPrice: "$0", annualPrice: "$0", credits: "0" });
      expect(pricing.plans.items[1]).toMatchObject({ monthlyPrice: "$12", annualPrice: "$10", credits: "10,000" });
      expect(pricing.plans.unavailable.length).toBeGreaterThanOrEqual(8);
      expect(pricing.faq[2][1].toLowerCase()).toMatch(/not yet|尚未/);
    }
  });

  it("keeps private account surfaces out of crawler access", () => {
    const rules = robots().rules;
    expect(Array.isArray(rules)).toBe(false);
    expect((rules as { disallow?: string[] }).disallow).toEqual(expect.arrayContaining(["/auth/", "/*/account", "/*/admin/"]));
  });
});
