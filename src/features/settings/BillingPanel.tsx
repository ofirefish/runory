import { Check, Coins, RefreshCw, WalletCards } from "lucide-react";
import { useCallback, useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { Button } from "../../components/ui/button";
import { cloudConfigured } from "../../lib/supabase/client";
import {
  cloudSession,
  getCreditAccount,
  getOrganizationSubscription,
  listBillingPlans,
  listCreditLedger,
  listOrganizations,
  onCloudAuthStateChange,
  startBillingTrial,
} from "../../lib/supabase/cloud";
import type {
  BillingPlan,
  BillingPlanCode,
  CreditAccount,
  CreditLedgerEntry,
  Organization,
  OrganizationSubscription,
} from "../../types/cloud";

const microcreditsPerCredit = 1000;

function credits(value: number): number {
  return value / microcreditsPerCredit;
}

export function BillingPanel({ onOpenAccount = () => undefined, embedded = false }: { onOpenAccount?: () => void; embedded?: boolean }) {
  const { t, i18n } = useTranslation();
  const [plans, setPlans] = useState<BillingPlan[]>([]);
  const [organizations, setOrganizations] = useState<Organization[]>([]);
  const [organizationId, setOrganizationId] = useState<string | null>(null);
  const [account, setAccount] = useState<CreditAccount | null>(null);
  const [subscription, setSubscription] = useState<OrganizationSubscription | null>(null);
  const [ledger, setLedger] = useState<CreditLedgerEntry[]>([]);
  const [authenticated, setAuthenticated] = useState(false);
  const [annual, setAnnual] = useState(true);
  const [loading, setLoading] = useState(true);
  const [startingPlan, setStartingPlan] = useState<BillingPlanCode | null>(null);
  const selectedOrganization = organizations.find((item) => item.id === organizationId) ?? null;

  const showError = useCallback(() => {
    toast.error(t("billing.error"), { id: "billing-operation-error" });
  }, [t]);

  const loadWorkspaceBilling = useCallback(async (selectedId: string) => {
    const [nextAccount, nextSubscription, nextLedger] = await Promise.all([
      getCreditAccount(selectedId),
      getOrganizationSubscription(selectedId),
      listCreditLedger(selectedId),
    ]);
    setAccount(nextAccount);
    setSubscription(nextSubscription);
    setLedger(nextLedger);
  }, []);

  const bootstrap = useCallback(async () => {
    setLoading(true);
    try {
      const [session, availablePlans] = await Promise.all([cloudSession(), listBillingPlans()]);
      setPlans(availablePlans);
      setAuthenticated(Boolean(session));
      if (!session) {
        setOrganizations([]);
        setOrganizationId(null);
        setAccount(null);
        setSubscription(null);
        setLedger([]);
        return;
      }
      const nextOrganizations = await listOrganizations();
      setOrganizations(nextOrganizations);
      const selectedId = nextOrganizations.find((item) => item.kind === "personal")?.id
        ?? nextOrganizations[0]?.id ?? null;
      setOrganizationId(selectedId);
      if (selectedId) await loadWorkspaceBilling(selectedId);
    } catch {
      showError();
    } finally {
      setLoading(false);
    }
  }, [loadWorkspaceBilling, showError]);

  useEffect(() => {
    if (!cloudConfigured) return;
    void bootstrap();
    const subscription = onCloudAuthStateChange(() => { void bootstrap(); });
    return () => subscription.unsubscribe();
  }, [bootstrap]);

  const chooseOrganization = async (nextId: string) => {
    setOrganizationId(nextId);
    setLoading(true);
    try { await loadWorkspaceBilling(nextId); }
    catch { showError(); }
    finally { setLoading(false); }
  };

  const startTrial = async (planCode: BillingPlanCode) => {
    if (!organizationId) return;
    setStartingPlan(planCode);
    try {
      await startBillingTrial(organizationId, planCode);
      await loadWorkspaceBilling(organizationId);
      toast.success(t("billing.trialStarted"));
    } catch {
      showError();
    } finally {
      setStartingPlan(null);
    }
  };

  const currency = useMemo(() => new Intl.NumberFormat(i18n.language, {
    style: "currency", currency: "USD", maximumFractionDigits: 0,
  }), [i18n.language]);
  const number = useMemo(() => new Intl.NumberFormat(i18n.language, { maximumFractionDigits: 1 }), [i18n.language]);

  if (!cloudConfigured) return <section className="settings-card"><h4>{t("billing.title")}</h4><p>{t("billing.cloudUnavailable")}</p></section>;

  return <section className="billing-panel">
    <div className="billing-toolbar">
      <div>
        {!embedded && <div className="billing-title"><WalletCards size={17} /><h3>{t("billing.title")}</h3></div>}
        <p>{t("billing.description")}</p>
      </div>
      <div className="billing-cycle" role="group" aria-label={t("billing.billingCycle")}>
        <button type="button" data-active={!annual} onClick={() => setAnnual(false)}>{t("billing.monthly")}</button>
        <button type="button" data-active={annual} onClick={() => setAnnual(true)}>{t("billing.annual")}<span>{t("billing.annualSaving")}</span></button>
      </div>
    </div>

    {!authenticated && <div className="settings-card billing-sign-in">
      <div><h4>{t("billing.signInTitle")}</h4><p>{t("billing.signInHint")}</p></div>
      <Button size="sm" onClick={onOpenAccount}>{t("billing.signIn")}</Button>
    </div>}

    {authenticated && <div className="billing-account-row">
      <label>{t("billing.workspace")}
        <select value={organizationId ?? ""} onChange={(event) => void chooseOrganization(event.target.value)}>
          {organizations.map((organization) => <option key={organization.id} value={organization.id}>{organization.name}</option>)}
        </select>
      </label>
      <div className="billing-balance-card">
        <span className="billing-balance-icon"><Coins size={17} /></span>
        <div><span>{t("billing.availableCredits")}</span><strong>{number.format(credits(account?.balance_microcredits ?? 0) - credits(account?.held_microcredits ?? 0))}</strong></div>
        {Boolean(account?.held_microcredits) && <small>{t("billing.heldCredits", { count: number.format(credits(account?.held_microcredits ?? 0)) })}</small>}
      </div>
    </div>}

    <div className="billing-plan-grid" aria-busy={loading}>
      {plans.map((plan) => {
        const onPaidPlan = Boolean(subscription && ["active", "trialing"].includes(subscription.status));
        const current = plan.code === "free" ? !onPaidPlan : onPaidPlan && subscription?.plan_code === plan.code;
        const price = annual ? plan.annual_monthly_price_cents : plan.monthly_price_cents;
        const trialAllowed = authenticated && !onPaidPlan && selectedOrganization?.kind === "team" && plan.trial_days > 0;
        const planBusy = startingPlan === plan.code;
        const isFree = plan.code === "free";
        return <article key={plan.code} className={`billing-plan billing-plan-${plan.code}`} data-current={current} data-recommended={plan.code === "pro"}>
          <header><div><h4>{t(`billing.plan.${plan.code}.name`)}</h4>{plan.code === "pro" && <span>{t("billing.recommended")}</span>}</div>{current && <span className="billing-current">{t("billing.currentPlan")}</span>}</header>
          <div className="billing-price"><strong>{isFree ? t("billing.free") : currency.format(price / 100)}</strong><span>{isFree ? t("billing.forever") : plan.per_seat ? t("billing.perUserMonth") : t("billing.perMonth")}{!isFree && <small>{annual ? t("billing.billedAnnually") : t("billing.billedMonthly")}</small>}</span></div>
          <p className="billing-plan-description">{t(`billing.plan.${plan.code}.description`)}</p>
          <div className="billing-credit-allocation"><Coins size={14} />{isFree || plan.included_monthly_credits === 0 ? t("billing.noManagedCredits") : t("billing.monthlyCredits", { count: number.format(plan.included_monthly_credits) })}</div>
          <ul>{plan.feature_codes.map((feature) => <li key={feature}><Check size={14} /><span>{t(`billing.feature.${feature}`)}</span></li>)}</ul>
          {current
            ? <Button disabled>{t("billing.currentPlan")}</Button>
            : isFree
              ? <Button variant="secondary" disabled>{t("billing.freePlan")}</Button>
              : trialAllowed
                ? <Button disabled={planBusy || loading} onClick={() => void startTrial(plan.code)}>{planBusy && <RefreshCw className="animate-spin" size={14} />}{t("billing.startTrial", { days: plan.trial_days })}</Button>
                : <Button variant={plan.code === "pro" ? "default" : "secondary"} disabled>{authenticated && selectedOrganization?.kind !== "team" && plan.trial_days > 0 ? t("billing.teamWorkspaceRequired") : t("billing.checkoutUnavailable")}</Button>}
        </article>;
      })}
    </div>

    {authenticated && <div className="billing-history settings-card">
      <div className="billing-history-heading"><div><h4>{t("billing.history")}</h4><p>{t("billing.historyHint")}</p></div>{organizationId && <Button size="icon" variant="ghost" aria-label={t("billing.refresh")} onClick={() => void loadWorkspaceBilling(organizationId)}><RefreshCw size={14} /></Button>}</div>
      {ledger.length === 0
        ? <p className="billing-empty">{t("billing.noTransactions")}</p>
        : <div className="billing-ledger-list">{ledger.map((entry) => <div key={entry.id} className="billing-ledger-entry">
          <div><strong>{t(entry.description_code)}</strong><span>{new Intl.DateTimeFormat(i18n.language, { dateStyle: "medium", timeStyle: "short" }).format(new Date(entry.created_at))}</span></div>
          <span data-positive={entry.amount_microcredits > 0}>{entry.amount_microcredits > 0 ? "+" : ""}{number.format(credits(entry.amount_microcredits))}</span>
        </div>)}</div>}
    </div>}
  </section>;
}
