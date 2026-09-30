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
export interface BillingEvent {{ event_id: string; event_type: string; payload: Record<string, unknown>; created_at: string; }}
export interface BillingEventList {{ data: BillingEvent[]; meta: {{ page: number; page_size: number; total: number }}; }}
export const billingFeatures = {{ plans: [{plans}] as const }};
export const billingApi = {{
  overview: (options: RequestOptions = {{}}) => request<BillingOverview>("/api/billing", options),
  checkout: (planId: string) => request<{{ url: string }}>("/api/billing/checkout", {{ method: "POST", body: JSON.stringify({{ plan_id: planId }}) }}),
  portal: () => request<{{ url: string }}>("/api/billing/portal", {{ method: "POST" }}),
}};
export const billingAdminApi = {{
  events: (page = 1, pageSize = 25, options: RequestOptions = {{}}) => request<BillingEventList>(`/api/admin/billing/events?page=${{page}}&page_size=${{pageSize}}`, options),
  replayEvent: (eventId: string) => request<BillingEvent>(`/api/admin/billing/events/${{encodeURIComponent(eventId)}}/replay`, {{ method: "POST" }}),
}};
"#
    )
}
