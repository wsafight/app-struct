use crate::{Artifact, ArtifactKind, CodegenError};
use appstruct_ir::AppIr;

pub(super) fn plan(ir: &AppIr) -> Result<Vec<Artifact>, CodegenError> {
    let source = if ir.billing.enabled {
        let plans = ir
            .billing
            .plans
            .iter()
            .map(|plan| {
                let entitlements = plan
                    .entitlements
                    .iter()
                    .map(|value| format!("\"{value}\""))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!(
                    "Plan {{ id: \"{}\", price_env: \"{}\", trial_days: {}, entitlements: &[{}] }}",
                    escape(&plan.id),
                    escape(&plan.price_env),
                    plan.trial_days
                        .map_or_else(|| "None".to_owned(), |days| format!("Some({days})")),
                    entitlements
                )
            })
            .collect::<Vec<_>>()
            .join(",\n    ");
        include_str!("../../templates/backend/billing.rs")
            .replace(
                "__PROVIDER__",
                ir.billing.provider.as_deref().unwrap_or("stripe"),
            )
            .replace("__PLANS__", &plans)
            .replace(
                "__PLAN_ID_FROM_SUBSCRIPTION__",
                include_str!("../../templates/backend/billing_plan.rs"),
            )
            .replace("__ACCESS__", access_source(ir))
            .replace(
                "__ALLOWANCES__",
                &ir.report
                    .entitlement
                    .as_ref()
                    .map_or_else(String::new, |feature| {
                        format!(
                            "Allowance {{ entitlement: {:?}, free_limit: {} }}",
                            feature,
                            ir.report.free_runs.unwrap_or(0)
                        )
                    }),
            )
            .replace(
                "__TRIALS__",
                if ir.billing.trials { "true" } else { "false" },
            )
            .replace(
                "__CUSTOMER_PORTAL__",
                if ir.billing.customer_portal {
                    "true"
                } else {
                    "false"
                },
            )
    } else {
        include_str!("../../templates/backend/billing_disabled.rs").to_owned()
    };
    Ok(vec![Artifact::text(
        "backend/src/billing.rs",
        super::rust_template(&source)?,
        ArtifactKind::RustSource,
    )])
}

fn escape(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

fn access_source(ir: &AppIr) -> &'static str {
    let source = include_str!("../../templates/backend/billing_access.rs");
    if ir.report.entitlement.is_some() {
        source
    } else {
        source
            .split_once("pub(crate) async fn reserve_allowance")
            .unwrap()
            .0
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    struct BillingConfig {
        prices: Vec<(&'static str, String)>,
    }

    #[derive(Debug)]
    enum ApiError {
        Internal,
    }

    include!("../../templates/backend/billing_plan.rs");

    fn test_config() -> BillingConfig {
        BillingConfig {
            prices: vec![
                ("pro", "price_pro".to_owned()),
                ("team", "price_team".to_owned()),
            ],
        }
    }

    fn subscription(items: &Value) -> Value {
        json!({
            "metadata": { "appstruct_plan_id": "pro" },
            "items": { "data": items, "has_more": false }
        })
    }

    #[test]
    fn plan_change_uses_current_price_instead_of_stale_metadata() {
        let remote = subscription(&json!([{ "price": { "id": "price_team" } }]));
        assert_eq!(
            plan_id_from_subscription(&test_config(), &remote).unwrap(),
            "team"
        );
    }

    #[test]
    fn unknown_or_missing_price_is_rejected() {
        let cases = [
            subscription(&json!([{ "price": { "id": "price_unknown" } }])),
            subscription(&json!([{}])),
            subscription(&json!([{ "price": {} }])),
            subscription(&json!([])),
            json!({ "metadata": { "appstruct_plan_id": "pro" } }),
        ];
        for remote in cases {
            assert!(plan_id_from_subscription(&test_config(), &remote).is_err());
        }
    }

    #[test]
    fn multiple_or_paginated_items_are_rejected() {
        let multiple = subscription(&json!([
            { "price": { "id": "price_pro" } },
            { "price": { "id": "price_team" } }
        ]));
        assert!(plan_id_from_subscription(&test_config(), &multiple).is_err());

        let paginated = json!({
            "items": {
                "data": [{ "price": { "id": "price_pro" } }],
                "has_more": true
            }
        });
        assert!(plan_id_from_subscription(&test_config(), &paginated).is_err());
    }

    #[test]
    fn ambiguous_configured_price_is_rejected() {
        let mut config = test_config();
        config.prices.push(("team", "price_pro".to_owned()));
        let remote = subscription(&json!([{ "price": { "id": "price_pro" } }]));
        assert!(plan_id_from_subscription(&config, &remote).is_err());
    }
}
