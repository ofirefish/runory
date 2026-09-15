import type { Metadata } from "next";
import Link from "next/link";
import { ArrowRight, Bot, Check, ChevronRight, CreditCard, Download, GitBranch, Network, Server, ShieldCheck, Terminal } from "lucide-react";
import { notFound } from "next/navigation";
import { DownloadPlatformPanel } from "@/components/landing/download-platform-panel";
import { LandingHeader, releaseUrl } from "@/components/landing/landing-interactions";
import { PricingPlans } from "@/components/landing/pricing-plans";
import { BrandMark } from "@/components/brand-mark";
import { fetchDownloadCatalog } from "@/lib/download-catalog";
import { getLandingCopy } from "@/lib/landing-copy";
import { getMarketingCopy, isMarketingSlug, localizedPath, marketingMetadata, marketingSlugs, siteUrl, type MarketingSlug } from "@/lib/marketing-pages";
import { isLocale, locales, type Locale } from "@/lib/i18n";
import "../landing.css";

const pageIcons = { product: Network, ssh: Terminal, operations: GitBranch, ai: Bot, security: ShieldCheck, pricing: CreditCard, download: Download } satisfies Record<MarketingSlug, typeof Server>;
type PageProps = { params: Promise<{ locale: string; marketingPage: string }> };

export const revalidate = 60;

export function generateStaticParams() {
  return locales.flatMap(locale => marketingSlugs.map(marketingPage => ({ locale, marketingPage })));
}

export async function generateMetadata({ params }: PageProps): Promise<Metadata> {
  const { locale, marketingPage } = await params;
  if (!isLocale(locale) || !isMarketingSlug(marketingPage)) return {};
  return marketingMetadata(locale, marketingPage);
}

function jsonLd(locale: Locale, slug: MarketingSlug) {
  const t = getMarketingCopy(locale);
  const page = t.pages[slug];
  const url = `${siteUrl}${localizedPath(locale, slug)}`;
  const graph: Record<string, unknown>[] = [
    { "@type": "WebPage", "@id": url, url, name: page.meta.title, description: page.meta.description, inLanguage: locale, isPartOf: { "@id": `${siteUrl}/#website` } },
    { "@type": "BreadcrumbList", itemListElement: [{ "@type": "ListItem", position: 1, name: t.common.home, item: `${siteUrl}/${locale}` }, { "@type": "ListItem", position: 2, name: page.label, item: url }] },
    { "@type": "FAQPage", mainEntity: page.faq.map(([question, answer]) => ({ "@type": "Question", name: question, acceptedAnswer: { "@type": "Answer", text: answer } })) },
  ];
  if (slug === "product" || slug === "pricing" || slug === "download") graph.push({ "@type": "SoftwareApplication", name: "Runory", applicationCategory: "DeveloperApplication", operatingSystem: "Windows, macOS, Linux", url: `${siteUrl}/${locale}/download`, description: page.meta.description, softwareHelp: `${siteUrl}/${locale}/product` });
  return { "@context": "https://schema.org", "@graph": graph };
}

export default async function MarketingPage({ params }: PageProps) {
  const { locale, marketingPage } = await params;
  if (!isLocale(locale) || !isMarketingSlug(marketingPage)) notFound();
  const landing = getLandingCopy(locale);
  const copy = getMarketingCopy(locale);
  const page = copy.pages[marketingPage];
  const Icon = pageIcons[marketingPage];
  const related = marketingSlugs.filter(slug => slug !== marketingPage).slice(0, 3);
  const isDownload = marketingPage === "download";
  const downloads = isDownload ? await fetchDownloadCatalog() : null;
  return <div className={`landing marketing-page marketing-${marketingPage}`} id="top">
    <a className="skip-link" href="#main-content">{landing.nav.skip}</a>
    <LandingHeader locale={locale} copy={landing.nav} page={marketingPage} />
    <main id="main-content">
      <section className="subpage-hero"><div className="hero-grid" aria-hidden="true" /><div className="landing-container">
        <nav className="breadcrumbs" aria-label={copy.common.breadcrumb}><Link href={`/${locale}`}>{copy.common.home}</Link><ChevronRight size={14} /><span>{page.label}</span></nav>
        <div className="subpage-hero-grid"><div><p className="landing-eyebrow">{page.eyebrow}</p><h1>{page.title}</h1><p className="subpage-lead">{page.lead}</p><div className="subpage-actions">{isDownload ? <a className="landing-primary" href="#download-platforms">{copy.common.ctaPrimary}<ArrowRight size={16} /></a> : <Link className="landing-primary" href={`/${locale}/download`}>{copy.common.ctaPrimary}<ArrowRight size={16} /></Link>}{marketingPage !== "product" && <Link className="landing-secondary" href={`/${locale}/product`}>{copy.common.ctaSecondary}</Link>}</div></div>
          {isDownload && downloads
            ? <DownloadPlatformPanel downloads={downloads} copy={copy.pages.download.panel} />
            : <div className="page-signal" aria-label={copy.common.capabilities}><div className="page-signal-icon"><Icon size={29} strokeWidth={1.5} /></div>{page.signal.map(([label, value], index) => <div key={label} className="signal-row"><span>0{index + 1}</span><p>{label}<strong>{value}</strong></p><i /></div>)}</div>}
        </div>
      </div></section>

      {marketingPage === "pricing" && <PricingPlans locale={locale} copy={copy.pages.pricing.plans} />}

      <section className="landing-container subpage-highlights" aria-labelledby="capabilities-title"><div className="subpage-section-label"><span>{marketingPage === "pricing" ? "02" : "01"}</span><p id="capabilities-title">{copy.common.capabilities}</p></div><div className="highlight-grid">{page.highlights.map(([title, description], index) => <article key={title}><span><Check size={16} />0{index + 1}</span><h2>{title}</h2><p>{description}</p></article>)}</div></section>

      <section className="subpage-body"><div className="landing-container"><div className="subpage-section-label"><span>{marketingPage === "pricing" ? "03" : "02"}</span><p>{copy.common.details}</p></div><div className="content-sections">{page.sections.map((section, index) => <article key={section.title}><div className="content-index">0{index + 1}</div><div><h2>{section.title}</h2>{section.body.map(paragraph => <p key={paragraph}>{paragraph}</p>)}{"bullets" in section && section.bullets && <ul>{section.bullets.map(item => <li key={item}><Check size={15} />{item}</li>)}</ul>}</div></article>)}</div></div></section>

      <section className="landing-container subpage-faq"><div><p className="landing-eyebrow">{copy.common.faq}</p><h2>{page.label}</h2></div><div className="faq-list">{page.faq.map(([question, answer], index) => <details key={question} name={`${marketingPage}-faq`} open={index === 0}><summary>{question}<span>+</span></summary><p>{answer}</p></details>)}</div></section>

      <section className="related-section"><div className="landing-container"><div className="related-heading"><p className="landing-eyebrow">{copy.common.explore}</p><h2>{copy.common.related}</h2></div><div className="related-grid">{related.map(slug => { const relatedPage = copy.pages[slug]; const RelatedIcon = pageIcons[slug]; return <Link key={slug} href={localizedPath(locale, slug)}><RelatedIcon size={21} /><h3>{relatedPage.label}</h3><p>{relatedPage.lead}</p><span>{copy.common.details}<ArrowRight size={15} /></span></Link>; })}</div><div className="subpage-final-cta"><div><BrandMark className="size-10" /><h2>{landing.downloads.title}</h2></div><a className="landing-primary" href={isDownload ? "#download-platforms" : releaseUrl} {...(isDownload ? {} : { target: "_blank", rel: "noopener noreferrer" })}>{landing.downloads.cta}<ArrowRight size={16} /></a></div></div></section>
    </main>
    <footer className="landing-footer landing-container"><div className="footer-main"><div><Link href={`/${locale}`} className="landing-brand"><BrandMark />Runory<span className="brand-period">.</span></Link><p>{landing.footer.tagline}</p></div>{marketingSlugs.slice(0, 3).map(slug => <div key={slug}><h3>{copy.pages[slug].label}</h3><Link href={localizedPath(locale, slug)}>{copy.common.details}</Link></div>)}</div><div className="footer-bottom"><span>© {new Date().getFullYear()} {landing.footer.copyright}</span><a href="#top">{landing.footer.top}</a></div></footer>
    <script type="application/ld+json" dangerouslySetInnerHTML={{ __html: JSON.stringify(jsonLd(locale, marketingPage)).replace(/</g, "\\u003c") }} />
  </div>;
}
