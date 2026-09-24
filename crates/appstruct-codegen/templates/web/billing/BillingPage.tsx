import { useMutation, useQuery } from "@tanstack/react-query";
import { Check, CreditCard, ExternalLink } from "lucide-react";
import { billingApi, type BillingPlan } from "../generated/client";
import { errorMessage } from "../resource";

export function BillingPage() {
  const billing = useQuery({
    queryKey: ["billing"],
    queryFn: ({ signal }) => billingApi.overview({ signal }),
  });
  const checkout = useMutation({
    mutationFn: billingApi.checkout,
    onSuccess: ({ url }) => window.location.assign(url),
  });
  const portal = useMutation({
    mutationFn: billingApi.portal,
    onSuccess: ({ url }) => window.location.assign(url),
  });
  if (billing.isPending) return <main className="page"><div className="auth-loading" aria-label="Loading" /></main>;
  if (billing.error) return <main className="page"><div className="alert" role="alert">{errorMessage(billing.error)}</div></main>;
  const data = billing.data;
  return <main className="page">
    <div className="page-heading"><div><h1>Billing</h1><p>Manage the organization subscription and access.</p></div>
      {data.customer_portal && data.subscription && <button className="button-secondary" type="button" onClick={() => portal.mutate()} disabled={portal.isPending}><ExternalLink size={16} /> Manage billing</button>}
    </div>
    {data.subscription && <section className="panel"><div className="section-heading"><CreditCard size={18} /><h2>Current subscription</h2></div><p>{data.subscription.plan_id} · {data.subscription.status}</p>{data.subscription.current_period_end && <p>Renews {new Date(data.subscription.current_period_end).toLocaleDateString()}</p>}</section>}
    <div className="billing-plans">{data.plans.map((plan) => <PlanCard key={plan.id} plan={plan} active={data.subscription?.plan_id === plan.id} onCheckout={() => checkout.mutate(plan.id)} disabled={checkout.isPending} />)}</div>
    {checkout.error && <div className="alert" role="alert">{errorMessage(checkout.error)}</div>}
  </main>;
}

function PlanCard({ plan, active, onCheckout, disabled }: { plan: BillingPlan; active: boolean; onCheckout: () => void; disabled: boolean }) {
  return <article className={`panel billing-plan${active ? " active" : ""}`}><h2>{plan.id}</h2>{plan.trial_days && <p>{plan.trial_days}-day trial</p>}<ul>{plan.entitlements.map((item) => <li key={item}><Check size={15} /> {item}</li>)}</ul>{!active && <button className="button-primary" type="button" onClick={onCheckout} disabled={disabled}><CreditCard size={16} /> Choose plan</button>}</article>;
}
