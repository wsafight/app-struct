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
            .replace("__PLANS__", &plans)
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
