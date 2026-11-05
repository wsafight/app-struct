import { formatDate } from "../ui";
import { t } from "../ui";
import { useMutation, useQuery } from "@tanstack/react-query";
import { Check, CreditCard, ExternalLink } from "lucide-react";
import { billingApi, type BillingPlan } from "../generated/client";
import { errorMessage } from "../resource";
import { useSearchParams } from "../navigation";

export function BillingPage() {
  const [search] = useSearchParams();
  const billing = useQuery({
    queryKey: ["billing"],
    queryFn: ({ signal }) => billingApi.overview({ signal }),
    refetchInterval: search.get("checkout") === "success" ? 5_000 : false,
  });
  const checkout = useMutation({
    mutationFn: billingApi.checkout,
    onSuccess: ({ url }) => window.location.assign(url),
  });
  const portal = useMutation({
    mutationFn: billingApi.portal,
    onSuccess: ({ url }) => window.location.assign(url),
  });
  if (billing.isPending)
    return (
      <main className="page">
        <div className="auth-loading" aria-label={t("Loading")} />
      </main>
    );
  if (billing.error)
    return (
      <main className="page">
        <div className="alert" role="alert">
          {errorMessage(billing.error)}
        </div>
      </main>
    );
  const data = billing.data;
  return (
    <main className="page">
      <div className="page-heading">
        <div>
          <h1>{t("Billing")}</h1>
          <p>{t("Manage the organization subscription and access.")}</p>
        </div>
        {data.checkout_available &&
          data.customer_portal &&
          data.subscription && (
            <button
              className="button-secondary"
              type="button"
              onClick={() => portal.mutate()}
              disabled={portal.isPending}
            >
              <ExternalLink size={16} /> {t("Manage billing")}
            </button>
          )}
      </div>
      {!data.checkout_available && (
        <p className="alert" role="status">
          {t(
            "Checkout is not configured. Your free allowance is still available.",
          )}
        </p>
      )}
      {search.get("checkout") === "success" && (
        <p role="status">
          {t(
            "Payment is being confirmed. Access updates automatically after the payment event arrives.",
          )}
        </p>
      )}
      {data.usage.map((allowance) => (
        <section className="panel" key={allowance.entitlement}>
          <h2>{allowance.entitlement}</h2>
          <p>
            {allowance.paid
              ? t("Included in your plan")
              : t("{0} of {1} free uses remaining", {
                  "0": allowance.remaining,
                  "1": allowance.free_limit,
                })}
          </p>
        </section>
      ))}
      {data.subscription && (
        <section className="panel">
          <div className="section-heading">
            <CreditCard size={18} />
            <h2>{t("Current subscription")}</h2>
          </div>
          <p>
            {data.subscription.plan_id} · {data.subscription.status}
          </p>
          {data.subscription.current_period_end && (
            <p>
              {t("Renews")} {formatDate(data.subscription.current_period_end)}
            </p>
          )}
        </section>
      )}
      <div className="billing-plans">
        {data.plans.map((plan) => (
          <PlanCard
            key={plan.id}
            plan={plan}
            active={
              data.subscription !== null &&
              data.subscription.plan_id === plan.id &&
              ["active", "trialing", "past_due"].includes(
                data.subscription.status,
              )
            }
            onCheckout={() => checkout.mutate(plan.id)}
            disabled={checkout.isPending || !data.checkout_available}
          />
        ))}
      </div>
      {checkout.error && (
        <div className="alert" role="alert">
          {errorMessage(checkout.error)}
        </div>
      )}
      {portal.error && (
        <div className="alert" role="alert">
          {errorMessage(portal.error)}
        </div>
      )}
    </main>
  );
}

function PlanCard({
  plan,
  active,
  onCheckout,
  disabled,
}: {
  plan: BillingPlan;
  active: boolean;
  onCheckout: () => void;
  disabled: boolean;
}) {
  return (
    <article className={`panel billing-plan${active ? " active" : ""}`}>
      <h2>{plan.id}</h2>
      {plan.trial_days && (
        <p>
          {plan.trial_days} {t("-day trial")}
        </p>
      )}
      <ul>
        {plan.entitlements.map((item) => (
          <li key={item}>
            <Check size={15} /> {item}
          </li>
        ))}
      </ul>
      {!active && (
        <button
          className="button-primary"
          type="button"
          onClick={onCheckout}
          disabled={disabled}
        >
          <CreditCard size={16} /> {t("Choose plan")}
        </button>
      )}
    </article>
  );
}
