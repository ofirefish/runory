import type { MetadataRoute } from "next";
import { locales } from "@/lib/i18n";
import { localizedPath, marketingSlugs, siteUrl } from "@/lib/marketing-pages";

export default function sitemap(): MetadataRoute.Sitemap {
  const now = new Date();
  const entries: MetadataRoute.Sitemap = [];
  for (const locale of locales) {
    entries.push({
      url: `${siteUrl}/${locale}`,
      lastModified: now,
      changeFrequency: "weekly",
      priority: 1,
      alternates: { languages: { "zh-CN": `${siteUrl}/zh-CN`, "en-US": `${siteUrl}/en-US` } },
    });
    for (const slug of marketingSlugs) entries.push({
      url: `${siteUrl}${localizedPath(locale, slug)}`,
      lastModified: now,
      changeFrequency: "monthly",
      priority: slug === "product" ? 0.9 : 0.8,
      alternates: { languages: { "zh-CN": `${siteUrl}/zh-CN/${slug}`, "en-US": `${siteUrl}/en-US/${slug}` } },
    });
  }
  return entries;
}
