import type { MetadataRoute } from "next";
import { siteUrl } from "@/lib/marketing-pages";

export default function robots(): MetadataRoute.Robots {
  return {
    rules: {
      userAgent: "*",
      allow: "/",
      disallow: ["/auth/", "/*/auth/", "/*/account", "/*/admin/"],
    },
    sitemap: `${siteUrl}/sitemap.xml`,
    host: siteUrl,
  };
}
