use crate::{Artifact, ArtifactKind, CodegenError};
use appstruct_ir::AppIr;

pub(super) fn plan(ir: &AppIr) -> Result<Vec<Artifact>, CodegenError> {
    if !ir.auth.enabled {
        return Ok(vec![Artifact::text(
            "backend/src/product.rs",
            super::rust_template(
                "use crate::AppState; use axum::Router; pub fn router() -> Router<AppState> { Router::new() }",
            )?,
            ArtifactKind::RustSource,
        )]);
    }
    [
        (
            "mod.rs",
            include_str!("../../templates/backend/product/mod.rs"),
        ),
        (
            "inbox.rs",
            include_str!("../../templates/backend/product/inbox.rs"),
        ),
        (
            "credits.rs",
            include_str!("../../templates/backend/product/credits.rs"),
        ),
        (
            "insights.rs",
            include_str!("../../templates/backend/product/insights.rs"),
        ),
        (
            "team.rs",
            include_str!("../../templates/backend/product/team.rs"),
        ),
        (
            "payments.rs",
            include_str!("../../templates/backend/product/payments.rs"),
        ),
    ]
    .into_iter()
    .map(|(name, template)| {
        let credit_plans = ir
            .billing
            .plans
            .iter()
            .filter(|plan| plan.entitlements.iter().any(|item| item == "credits"))
            .map(|plan| format!("{:?}", plan.id))
            .collect::<Vec<_>>()
            .join(",");
        let seat_plans = ir
            .billing
            .plans
            .iter()
            .filter(|plan| plan.entitlements.iter().any(|item| item == "seats"))
            .map(|plan| format!("{:?}", plan.id))
            .collect::<Vec<_>>()
            .join(",");
        let source = template
            .replace("__SEAT_PLANS__", &seat_plans)
            .replace("__CREDIT_PLANS__", &credit_plans)
            .replace(
                "__TENANT__",
                if ir.tenant.enabled { "true" } else { "false" },
            )
            .replace(
                "__BILLING__",
                if ir.billing.enabled { "true" } else { "false" },
            )
            .replace(
                "__REPORT__",
                if ir.report.enabled { "true" } else { "false" },
            );
        Ok(Artifact::text(
            format!("backend/src/product/{name}"),
            super::rust_template(&source)?,
            ArtifactKind::RustSource,
        ))
    })
    .collect()
}
