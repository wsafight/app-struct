use appstruct_ir::AppIr;

pub(super) fn source(ir: &AppIr) -> String {
    let plans = ir
        .billing
        .plans
        .iter()
        .map(|plan| format!("\"{}\"", plan.id))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        r#"export interface BillingPlan {{ id: string; trial_days: number | null; entitlements: string[]; }}
export interface BillingSubscription {{ plan_id: string; status: string; current_period_end: string | null; cancel_at_period_end: boolean; }}
export interface BillingOverview {{ plans: BillingPlan[]; subscription: BillingSubscription | null; entitlements: string[]; customer_portal: boolean; }}
export const billingFeatures = {{ plans: [{plans}] as const }};
export const billingApi = {{
  overview: (options: RequestOptions = {{}}) => request<BillingOverview>("/api/billing", options),
  checkout: (planId: string) => request<{{ url: string }}>("/api/billing/checkout", {{ method: "POST", body: JSON.stringify({{ plan_id: planId }}) }}),
  portal: () => request<{{ url: string }}>("/api/billing/portal", {{ method: "POST" }}),
}};
"#
    )
}
