use crate::surface::{SurfaceBilling, SurfaceBillingPlan};
use appstruct_ir::{BillingIr, BillingPlanIr, Diagnostic, SourceSpan};
use std::collections::BTreeSet;

pub(crate) fn lower_billing(
    billing: &SurfaceBilling,
    auth_enabled: bool,
    tenant_enabled: bool,
    fallback: &SourceSpan,
    diagnostics: &mut Vec<Diagnostic>,
) -> BillingIr {
    if !billing.enabled {
        if billing.provider.is_some()
            || billing.subscriptions
            || billing.trials
            || billing.customer_portal
            || billing.metered_usage
            || !billing.plans.is_empty()
        {
            diagnostics.push(Diagnostic::error(
                "AS3071",
                "billing settings require `modules.billing.enabled: true`",
                billing.span.clone().unwrap_or_else(|| fallback.clone()),
            ));
        }
        return BillingIr::default();
    }
    if !auth_enabled || !tenant_enabled {
        diagnostics.push(Diagnostic::error(
            "AS3072",
            "enabled billing requires enabled auth and tenant modules",
            billing.span.clone().unwrap_or_else(|| fallback.clone()),
        ));
    }
    let provider = billing.provider.as_ref().map(|value| value.value.clone());
    if provider.as_deref() != Some("stripe") {
        diagnostics.push(Diagnostic::error(
            "AS3073",
            "billing currently supports only `provider: stripe`",
            billing.provider.as_ref().map_or_else(
                || billing.span.clone().unwrap_or_else(|| fallback.clone()),
                |value| value.span.clone(),
            ),
        ));
    }
    if !billing.subscriptions {
        diagnostics.push(Diagnostic::error(
            "AS3074",
            "Stripe Billing v1 requires `capabilities.subscriptions: true`",
            billing.span.clone().unwrap_or_else(|| fallback.clone()),
        ));
    }
    if billing.metered_usage {
        diagnostics.push(Diagnostic::error(
            "AS3075",
            "Stripe Billing v1 does not support `capabilities.metered_usage` yet",
            billing.span.clone().unwrap_or_else(|| fallback.clone()),
        ));
    }
    if billing.plans.is_empty() {
        diagnostics.push(Diagnostic::error(
            "AS3076",
            "enabled billing requires at least one plan",
            billing.span.clone().unwrap_or_else(|| fallback.clone()),
        ));
    }
    let mut ids = BTreeSet::new();
    let mut plans = Vec::with_capacity(billing.plans.len());
    for plan in &billing.plans {
        validate_plan(plan, &mut ids, diagnostics);
        if let Some(days) = &plan.trial_days
            && !billing.trials
        {
            diagnostics.push(Diagnostic::error(
                "AS3083",
                "billing plan trial_days requires `capabilities.trials: true`",
                days.span.clone(),
            ));
        }
        plans.push(BillingPlanIr {
            id: plan.id.value.clone(),
            price_env: plan.price_env.value.clone(),
            trial_days: plan
                .trial_days
                .as_ref()
                .and_then(|value| u16::try_from(value.value).ok()),
            entitlements: plan
                .entitlements
                .iter()
                .map(|value| value.value.clone())
                .collect(),
        });
    }
    plans.sort_by(|left, right| left.id.cmp(&right.id));
    BillingIr {
        enabled: true,
        provider,
        subscriptions: billing.subscriptions,
        trials: billing.trials,
        customer_portal: billing.customer_portal,
        metered_usage: false,
        plans,
    }
}

fn validate_plan(
    plan: &SurfaceBillingPlan,
    ids: &mut BTreeSet<String>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if !valid_identifier(&plan.id.value) {
        diagnostics.push(Diagnostic::error(
            "AS3077",
            format!("invalid billing plan id `{}`", plan.id.value),
            plan.id.span.clone(),
        ));
    }
    if !ids.insert(plan.id.value.clone()) {
        diagnostics.push(Diagnostic::error(
            "AS3078",
            format!(
                "billing plan `{}` is declared more than once",
                plan.id.value
            ),
            plan.id.span.clone(),
        ));
    }
    if !valid_env_name(&plan.price_env.value) {
        diagnostics.push(Diagnostic::error(
            "AS3079",
            format!(
                "invalid billing price environment variable `{}`",
                plan.price_env.value
            ),
            plan.price_env.span.clone(),
        ));
    }
    if let Some(days) = &plan.trial_days
        && !billing_trial_days_valid(days.value)
    {
        diagnostics.push(Diagnostic::error(
            "AS3082",
            "billing trial_days must be between 1 and 730",
            days.span.clone(),
        ));
    }
    let mut entitlements = BTreeSet::new();
    for entitlement in &plan.entitlements {
        if !valid_identifier(&entitlement.value) {
            diagnostics.push(Diagnostic::error(
                "AS3080",
                format!("invalid billing entitlement `{}`", entitlement.value),
                entitlement.span.clone(),
            ));
        }
        if !entitlements.insert(entitlement.value.clone()) {
            diagnostics.push(Diagnostic::error(
                "AS3081",
                format!(
                    "billing entitlement `{}` is declared more than once",
                    entitlement.value
                ),
                entitlement.span.clone(),
            ));
        }
    }
}

fn billing_trial_days_valid(value: u64) -> bool {
    (1..=730).contains(&value)
}

fn valid_identifier(value: &str) -> bool {
    let mut chars = value.chars();
    matches!(chars.next(), Some(first) if first.is_ascii_lowercase() || first == '_')
        && chars.all(|character| {
            character.is_ascii_lowercase()
                || character.is_ascii_digit()
                || character == '_'
                || character == '-'
        })
}

fn valid_env_name(value: &str) -> bool {
    let mut chars = value.chars();
    matches!(chars.next(), Some(first) if first.is_ascii_uppercase() || first == '_')
        && chars.all(|character| {
            character.is_ascii_uppercase() || character.is_ascii_digit() || character == '_'
        })
}
