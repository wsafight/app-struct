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
