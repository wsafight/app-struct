use super::value::{
    ensure_known_keys, expect_bool, expect_mapping, expect_sequence, expect_string, expect_u64,
};
use super::{SurfaceBilling, SurfaceBillingPlan};
use crate::yaml::MappingEntry;
use appstruct_ir::Diagnostic;

pub(super) fn decode(modules_entry: Option<&MappingEntry>) -> Result<SurfaceBilling, Diagnostic> {
    let Some(modules_entry) = modules_entry else {
        return Ok(SurfaceBilling::default());
    };
    let modules = expect_mapping(&modules_entry.value, "`modules`")?;
    ensure_known_keys(
        modules,
        &[
            "auth", "billing", "rbac", "tenant", "audit", "mail", "jobs", "webhooks", "realtime",
            "file", "report", "activity",
        ],
        "`modules`",
    )?;
    let Some(entry) = modules.get("billing") else {
        return Ok(SurfaceBilling::default());
    };
    let billing = expect_mapping(&entry.value, "`modules.billing`")?;
    ensure_known_keys(
        billing,
        &["enabled", "provider", "capabilities", "plans"],
        "`modules.billing`",
    )?;
    let capabilities = billing
        .get("capabilities")
        .map(|value| expect_mapping(&value.value, "`modules.billing.capabilities`"))
        .transpose()?;
    if let Some(capabilities) = capabilities {
        ensure_known_keys(
            capabilities,
            &[
                "subscriptions",
                "trials",
                "customer_portal",
                "metered_usage",
            ],
            "`modules.billing.capabilities`",
        )?;
    }
    let plans = billing
        .get("plans")
        .map(|value| decode_plans(&value.value))
        .transpose()?
        .unwrap_or_default();
    Ok(SurfaceBilling {
        enabled: optional_bool(billing.get("enabled"), "`modules.billing.enabled`")?,
        provider: billing
            .get("provider")
            .map(|value| expect_string(&value.value, "`modules.billing.provider`"))
            .transpose()?,
        subscriptions: capabilities
            .and_then(|value| value.get("subscriptions"))
            .map(|value| expect_bool(&value.value, "`modules.billing.capabilities.subscriptions`"))
            .transpose()?
            .unwrap_or_default(),
        trials: capabilities
            .and_then(|value| value.get("trials"))
            .map(|value| expect_bool(&value.value, "`modules.billing.capabilities.trials`"))
            .transpose()?
            .unwrap_or_default(),
        customer_portal: capabilities
            .and_then(|value| value.get("customer_portal"))
            .map(|value| {
                expect_bool(
                    &value.value,
                    "`modules.billing.capabilities.customer_portal`",
                )
            })
            .transpose()?
            .unwrap_or_default(),
        metered_usage: capabilities
            .and_then(|value| value.get("metered_usage"))
            .map(|value| expect_bool(&value.value, "`modules.billing.capabilities.metered_usage`"))
            .transpose()?
            .unwrap_or_default(),
        plans,
        span: Some(entry.key_span.clone()),
    })
}

fn decode_plans(node: &crate::yaml::Node) -> Result<Vec<SurfaceBillingPlan>, Diagnostic> {
    expect_sequence(node, "`modules.billing.plans`")?
        .iter()
        .map(|item| {
            let mapping = expect_mapping(item, "billing plan")?;
            ensure_known_keys(
                mapping,
                &["id", "price_env", "trial_days", "entitlements"],
                "billing plan",
            )?;
            let id = mapping.get("id").ok_or_else(|| {
                Diagnostic::error("AS3070", "billing plan requires `id`", item.span.clone())
            })?;
            let price_env = mapping.get("price_env").ok_or_else(|| {
                Diagnostic::error(
                    "AS3070",
                    "billing plan requires `price_env`",
                    item.span.clone(),
                )
            })?;
            let entitlements = mapping
                .get("entitlements")
                .map(|value| {
                    expect_sequence(&value.value, "billing plan entitlements")?
                        .iter()
                        .map(|value| expect_string(value, "billing entitlement"))
                        .collect()
                })
                .transpose()?
                .unwrap_or_default();
            Ok(SurfaceBillingPlan {
                id: expect_string(&id.value, "`billing plan.id`")?,
                price_env: expect_string(&price_env.value, "`billing plan.price_env`")?,
                trial_days: mapping
                    .get("trial_days")
                    .map(|value| expect_u64(&value.value, "`billing plan.trial_days`"))
                    .transpose()?,
                entitlements,
            })
        })
        .collect()
}

fn optional_bool(entry: Option<&MappingEntry>, context: &str) -> Result<bool, Diagnostic> {
    entry
        .map(|value| expect_bool(&value.value, context))
        .transpose()
        .map(Option::unwrap_or_default)
}
