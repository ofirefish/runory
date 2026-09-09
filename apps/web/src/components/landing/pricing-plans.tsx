"use client";

import Link from "next/link";
import { useState } from "react";
import { ArrowRight, Check, Coins } from "lucide-react";
import type { Locale } from "@/lib/i18n";
import type { PricingPage } from "@/lib/marketing-pages";

export function PricingPlans({ locale, copy }: { locale: Locale; copy: PricingPage["plans"] }) {
  const [annual, setAnnual] = useState(true);

  return <section className="pricing-section" aria-labelledby="pricing-plans-title">
    <div className="landing-container">
      <div className="pricing-heading">
        <div><span>01</span><p id="pricing-plans-title">{copy.included}</p></div>
        <div className="pricing-cycle" role="group" aria-label={`${copy.monthly} / ${copy.annual}`}>
          <button type="button" aria-pressed={!annual} onClick={() => setAnnual(false)}>{copy.monthly}</button>
          <button type="button" aria-pressed={annual} onClick={() => setAnnual(true)}>{copy.annual}</button>
        </div>
      </div>
      <div className="pricing-grid">
        {copy.items.map(plan => <article key={plan.code} className="pricing-card" data-featured={plan.featured}>
          <header><div><h2>{plan.name}</h2><p>{plan.description}</p></div>{plan.featured && <span>{copy.recommended}</span>}</header>
          <div className="pricing-price"><strong>{annual ? plan.annualPrice : plan.monthlyPrice}</strong><span>{plan.code === "free" ? copy.freeForever : plan.perSeat ? copy.perUserMonth : copy.perMonth}</span></div>
          <div className="pricing-credits"><Coins size={15} />{plan.credits === "0" ? copy.noCredits : copy.credits.replace("{{count}}", plan.credits)}</div>
          <p className="pricing-included">{copy.included}</p>
          <ul>{plan.features.map(feature => <li key={feature}><Check size={15} />{feature}</li>)}</ul>
          {plan.code === "free"
            ? <Link className="pricing-cta pricing-cta-primary" href={`/${locale}/download`}>{copy.download}<ArrowRight size={15} /></Link>
            : <span className="pricing-cta pricing-cta-disabled" aria-disabled="true">{copy.unavailable}</span>}
        </article>)}
      </div>
      <p className="pricing-note">{copy.note}</p>
    </div>
  </section>;
}
