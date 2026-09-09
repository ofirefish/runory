"use client";

import Link from "next/link";
import { useRef, useState, type KeyboardEvent } from "react";
import { ArrowRight, Check, ChevronRight, Download, Globe2, Menu, Monitor, ShieldCheck, Sparkles, X } from "lucide-react";
import { BrandMark } from "@/components/brand-mark";
import { Button } from "@/components/ui/button";
import type { LandingCopy } from "@/lib/landing-copy";
import type { Locale } from "@/lib/i18n";

export const releaseUrl = "https://github.com/ofirefish/runory/releases";
export const sourceUrl = "https://github.com/ofirefish/runory";

export function LandingHeader({ locale, copy: t, page }: { locale: Locale; copy: LandingCopy["nav"]; page?: string }) {
  const [open, setOpen] = useState(false);
  const toggle = useRef<HTMLButtonElement>(null);
  const alternateLocale = locale === "zh-CN" ? "en-US" : "zh-CN";
  const links = [[`/${locale}/product`, t.product], [`/${locale}/ai`, t.workflow], [`/${locale}/security`, t.security], [`/${locale}/pricing`, t.pricing], [`/${locale}#faq`, t.faq]];
  return <header className="landing-header" onKeyDown={event => { if (event.key === "Escape") { setOpen(false); toggle.current?.focus(); } }}>
    <div className="landing-container header-inner"><Link href={`/${locale}`} className="landing-brand" aria-label="Runory"><BrandMark />Runory<span className="brand-period">.</span></Link>
      <nav className="desktop-nav" aria-label={t.label}>{links.map(([href, label]) => <Link key={href} href={href}>{label}</Link>)}</nav>
      <div className="header-actions"><Link className="language-link" href={`/${alternateLocale}${page ? `/${page}` : ""}`} aria-label={t.language}><Globe2 size={16} /><span>{t.language}</span></Link><Link className="signin-link" href={`/${locale}/auth/sign-in`}>{t.signIn}</Link><Button asChild size="sm" className="landing-primary header-download"><Link href={`/${locale}/download`}>{t.download}<ArrowRight size={14} /></Link></Button><button className="mobile-toggle" ref={toggle} aria-label={open ? t.close : t.menu} aria-expanded={open} aria-controls="mobile-navigation" onClick={() => setOpen(!open)}>{open ? <X size={21} /> : <Menu size={21} />}</button></div>
    </div>
    {open && <nav id="mobile-navigation" className="mobile-navigation" aria-label={t.label}>{links.map(([href, label]) => <Link key={href} href={href} onClick={() => setOpen(false)}>{label}<ChevronRight size={16} /></Link>)}<Link href={`/${locale}/auth/sign-in`}>{t.signIn}<ArrowRight size={16} /></Link></nav>}
  </header>;
}

export function AgentWalkthrough({ copy: t }: { copy: LandingCopy["agent"] }) {
  const [step, setStep] = useState(0);
  return <div className="agent-demo" aria-label={t.label}>
    <div className="agent-demo-header"><span><Sparkles size={17} />Runory AI</span><span>{t.runtime}</span></div>
    <div className="agent-question">{t.question}<span>↵</span></div>
    <div className="agent-step-selector" role="group" aria-label={t.label}>{t.steps.map((label, index) => <button type="button" key={label} aria-pressed={step === index} onClick={() => setStep(index)}><span>{index + 1}</span>{label}</button>)}</div>
    <div className="agent-response" aria-live="polite"><div className="agent-response-icon"><Sparkles size={18} /></div><p>{t.summaries[step]}</p></div>
    <div className={`command-proposal step-${step}`}><div><ShieldCheck size={14} />{t.risk}</div><code>{t.command}</code><div className="proposal-footer">{step === 2 ? <><Check size={15} />{t.result}</> : <><span className="proposal-dot" />{t.steps[step]}<span className="proposal-line" /></>}</div></div>
    <p className="agent-demo-note">{t.note}</p>
  </div>;
}

export function DownloadPicker({ copy: t }: { copy: LandingCopy["downloads"] }) {
  const [platform, setPlatform] = useState(0);
  const buttons = useRef<(HTMLButtonElement | null)[]>([]);
  function navigate(event: KeyboardEvent<HTMLButtonElement>) {
    let next = platform;
    if (event.key === "ArrowRight") next = (platform + 1) % 3;
    else if (event.key === "ArrowLeft") next = (platform + 2) % 3;
    else if (event.key === "Home") next = 0;
    else if (event.key === "End") next = 2;
    else return;
    event.preventDefault(); setPlatform(next); buttons.current[next]?.focus();
  }
  return <div className="download-picker"><div className="platform-tabs" role="tablist" aria-label={t.start}>{t.platforms.map((label, index) => <button key={label} ref={el => { buttons.current[index] = el; }} role="tab" id={`platform-${index}`} aria-controls={`package-${index}`} aria-selected={platform === index} tabIndex={platform === index ? 0 : -1} onKeyDown={navigate} onClick={() => setPlatform(index)}><Monitor size={17} />{label}</button>)}</div><div role="tabpanel" id={`package-${platform}`} aria-labelledby={`platform-${platform}`} tabIndex={0} className="download-package"><p>{t.packages[platform]}</p><Button asChild className="landing-primary"><a href={releaseUrl} target="_blank" rel="noopener noreferrer"><Download size={17} />{t.cta}<ArrowRight size={16} /></a></Button></div><p className="download-availability">{t.availability}</p></div>;
}
