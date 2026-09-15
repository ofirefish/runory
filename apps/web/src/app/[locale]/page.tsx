import type { Metadata } from "next";
import Link from "next/link";
import { Activity, ArrowRight, ArrowUpRight, Box, Check, ChevronDown, Cloud, Code2, Download, Fingerprint, FolderOpen, GitBranch, Laptop, LockKeyhole, MoveRight, Network, Server, ShieldCheck, Smartphone, Terminal } from "lucide-react";
import { notFound } from "next/navigation";
import { BrandMark } from "@/components/brand-mark";
import { Button } from "@/components/ui/button";
import { AgentWalkthrough, DownloadPicker, LandingHeader } from "@/components/landing/landing-interactions";
import { WorkspaceTour } from "@/components/landing/workspace-tour";
import { isLocale } from "@/lib/i18n";
import { fetchDownloadCatalog } from "@/lib/download-catalog";
import { getLandingCopy } from "@/lib/landing-copy";
import { siteUrl } from "@/lib/marketing-pages";
import "./landing.css";

const featureIcons = [Network, FolderOpen, Activity, Box, GitBranch, Terminal];
const securityIcons = [LockKeyhole, Fingerprint, ShieldCheck];
const sourceUrl = "https://github.com/ofirefish/runory";
type PageProps = { params: Promise<{ locale: string }> };

export async function generateMetadata({ params }: PageProps): Promise<Metadata> {
  const { locale } = await params;
  if (!isLocale(locale)) return {};
  const meta = getLandingCopy(locale).meta;
  return {
    ...meta,
    keywords: locale === "zh-CN"
      ? ["SSH 客户端", "跳板机", "SSH 隧道", "SFTP 客户端", "服务器管理", "AI Terminal", "Incident"]
      : ["SSH client", "jump host", "SSH tunnel", "SFTP client", "server management", "AI terminal", "Incident"],
    alternates: { canonical: `/${locale}`, languages: { "zh-CN": "/zh-CN", "en-US": "/en-US", "x-default": "/en-US" } },
    openGraph: { type: "website", url: `/${locale}`, siteName: "Runory", locale, title: meta.title, description: meta.description },
    twitter: { card: "summary", title: meta.title, description: meta.description },
  };
}

export default async function LandingPage({ params }: PageProps) {
  const { locale } = await params;
  if (!isLocale(locale)) notFound();
  const t = getLandingCopy(locale);
  const downloads = await fetchDownloadCatalog();
  return <div className="landing" id="top">
    <a className="skip-link" href="#main-content">{t.nav.skip}</a>
    <LandingHeader locale={locale} copy={t.nav} />
    <main id="main-content">
      <section className="landing-hero"><div className="hero-grid" aria-hidden="true" /><div className="landing-container hero-content"><div className="hero-badge"><span className="live-dot" />{t.hero.badge}<ArrowUpRight size={13} /></div><h1>{t.hero.title}<br /><span>{t.hero.accent}</span></h1><p className="hero-description">{t.hero.description}</p><div className="hero-actions"><Button asChild className="landing-primary"><Link href={`/${locale}/download`}><Download size={17} />{t.hero.primary}<ArrowRight size={17} /></Link></Button><Button asChild variant="secondary" className="landing-secondary"><a href="#workspace">{t.hero.secondary}<ChevronDown size={17} /></a></Button></div><div className="hero-assurances">{[t.hero.note, t.hero.note2, t.hero.note3].map(text => <span key={text}><Check size={14} />{text}</span>)}</div><WorkspaceTour copy={t.demo} /></div></section>
      <div className="tool-strip landing-container"><p>{t.strip.title}</p><div>{t.strip.items.map(item => <span key={item}>{item}</span>)}</div></div>
      <section className="landing-section landing-container" id="product"><div className="section-intro"><div><p className="landing-eyebrow">{t.features.eyebrow}</p><h2>{t.features.title}</h2></div><p>{t.features.description}</p></div><div className="features-grid">{t.features.items.map((item, index) => { const Icon = featureIcons[index]; return <article className="capability-card" key={item.title}><div className="capability-top"><span className="capability-icon"><Icon size={23} strokeWidth={1.6} /></span><span className="capability-number">0{index + 1}</span></div><h3>{item.title}</h3><p>{item.description}</p><div className="capability-tags">{item.tags.map(tag => <span key={tag}>{tag}</span>)}</div></article>; })}</div></section>
      <section className="agent-section" id="workflow"><div className="landing-container split-section"><div className="section-copy"><p className="landing-eyebrow">{t.agent.eyebrow}</p><h2>{t.agent.title}</h2><p className="section-description">{t.agent.description}</p><ul className="agent-points">{t.agent.points.map(point => <li key={point}><Check size={17} />{point}</li>)}</ul><Link className="text-link" href={`/${locale}/security`}>{t.nav.security}<ArrowRight size={17} /></Link></div><AgentWalkthrough copy={t.agent} /></div></section>
      <section className="landing-section landing-container security-section" id="security"><div className="section-intro"><div><p className="landing-eyebrow">{t.security.eyebrow}</p><h2>{t.security.title}</h2></div><p>{t.security.description}</p></div><div className="security-layout"><div className="security-diagram"><div className="security-node"><Laptop size={37} strokeWidth={1.3} /><strong>{t.security.local}</strong><span><LockKeyhole size={13} />{t.security.vault}</span></div><div className="secure-connection"><span><LockKeyhole size={15} />{t.security.ssh}</span><div><i /><i /><i /><MoveRight size={25} /></div></div><div className="security-node"><Server size={37} strokeWidth={1.3} /><strong>{t.security.remote}</strong><span>SSH / SFTP</span></div><p className="security-boundary"><ShieldCheck size={15} />{t.security.boundary}</p></div><div className="security-principles">{t.security.items.map((item, index) => { const Icon = securityIcons[index]; return <article key={item.title}><Icon size={21} /><div><h3>{item.title}</h3><p>{item.description}</p></div></article>; })}</div></div></section>
      <section className="sync-section landing-container" id="sync"><div className="sync-copy"><p className="landing-eyebrow">{t.sync.eyebrow}</p><h2>{t.sync.title}</h2><p className="section-description">{t.sync.description}</p><div className="sync-chips">{t.sync.chips.map(chip => <span key={chip}><Check size={13} />{chip}</span>)}</div><Link className="text-link" href={`/${locale}/auth/sign-in`}>{t.sync.cta}<ArrowUpRight size={17} /></Link></div><div className="cloud-diagram"><div className="cloud-node"><Laptop size={31} strokeWidth={1.4} /><span>{t.sync.local}</span></div><div className="encrypted-route"><div className="encrypted-packet"><LockKeyhole size={17} /></div><span>{t.sync.encrypted}</span></div><div className="cloud-node"><Cloud size={35} strokeWidth={1.4} /><span>{t.sync.cloud}</span></div><p>{t.sync.note}</p></div></section>
      <section className="landing-section landing-container faq-section" id="faq"><div><p className="landing-eyebrow">{t.faq.eyebrow}</p><h2>{t.faq.title}</h2><span className="faq-symbol" aria-hidden="true">?</span></div><div className="faq-list">{t.faq.items.map((item, index) => <details key={item.question} name="runory-faq" open={index === 0}><summary>{item.question}<span><ChevronDown size={18} /></span></summary><p>{item.answer}</p></details>)}</div></section>
      <section className="download-section" id="download"><div className="landing-container"><p className="landing-eyebrow">{t.downloads.eyebrow}</p><h2>{t.downloads.title}</h2><p className="download-description">{t.downloads.description}</p><DownloadPicker copy={t.downloads} downloads={downloads} /><div className="getting-started" aria-label={t.downloads.start}>{t.downloads.instructions.map((instruction, index) => <div key={instruction}><span>0{index + 1}</span><p>{instruction}</p></div>)}</div><div className="mobile-note"><Smartphone size={22} /><div><h3>{t.downloads.mobile}</h3><p>{t.downloads.mobileDescription}</p></div></div></div></section>
    </main>
    <footer className="landing-footer landing-container"><div className="footer-main"><div><Link href={`/${locale}`} className="landing-brand"><BrandMark />Runory<span className="brand-period">.</span></Link><p>{t.footer.tagline}</p></div><div><h3>{t.footer.product}</h3><Link href={`/${locale}/product`}>{t.nav.product}</Link><Link href={`/${locale}/ai`}>{t.nav.workflow}</Link><Link href={`/${locale}/security`}>{t.nav.security}</Link></div><div><h3>{t.footer.resources}</h3><Link href={`/${locale}/download`}>{t.nav.download}</Link><a href="#faq">{t.nav.faq}</a><a href={sourceUrl} target="_blank" rel="noopener noreferrer">{t.downloads.source}<Code2 size={13} /></a></div><div><h3>{t.footer.account}</h3><Link href={`/${locale}/auth/sign-in`}>{t.nav.signIn}</Link><a href="#sync">{t.sync.eyebrow}</a><Link href={`/${locale === "zh-CN" ? "en-US" : "zh-CN"}`}>{t.nav.language}</Link></div></div><div className="footer-bottom"><span>© {new Date().getFullYear()} {t.footer.copyright}</span><a href="#top">{t.footer.top}<ArrowUpRight size={14} /></a></div></footer>
    <script type="application/ld+json" dangerouslySetInnerHTML={{ __html: JSON.stringify({ "@context": "https://schema.org", "@graph": [{ "@type": "WebSite", "@id": `${siteUrl}/#website`, url: siteUrl, name: "Runory", inLanguage: ["zh-CN", "en-US"] }, { "@type": "Organization", "@id": `${siteUrl}/#organization`, name: "Runory", url: siteUrl, logo: `${siteUrl}/runory-logo.png`, sameAs: [sourceUrl] }, { "@type": "SoftwareApplication", name: "Runory", applicationCategory: "DeveloperApplication", operatingSystem: "Windows, macOS, Linux", url: `${siteUrl}/${locale}/product`, description: t.meta.description }, { "@type": "FAQPage", mainEntity: t.faq.items.map(item => ({ "@type": "Question", name: item.question, acceptedAnswer: { "@type": "Answer", text: item.answer } })) }] }).replace(/</g, "\\u003c") }} />
  </div>;
}
