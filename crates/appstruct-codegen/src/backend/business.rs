use crate::{Artifact, ArtifactKind, CodegenError};
use appstruct_ir::AppIr;

pub(super) fn artifact(ir: &AppIr) -> Result<Artifact, CodegenError> {
    let source = if ir.auth.enabled {
        include_str!("../../templates/backend/business.rs")
            .replace("__ORGANIZATIONS__", if ir.tenant.enabled { "Some(\"SELECT COUNT(*) AS total FROM \\\"_appstruct_tenant_organizations\\\"\")" } else { "None" })
            .replace("__REPORTS__", if ir.report.enabled { "Some(\"SELECT COUNT(*) AS total FROM \\\"_appstruct_report_runs\\\" WHERE stage = 'succeeded'\")" } else { "None" })
            .replace("__FIRST_SUCCESS__", if ir.report.enabled { "Some(\"SELECT COUNT(DISTINCT actor_id) AS total FROM \\\"_appstruct_report_runs\\\" WHERE stage = 'succeeded'\")" } else { "None" })
            .replace("__PAYING__", if ir.billing.enabled { "Some(\"SELECT COUNT(*) AS total FROM (SELECT DISTINCT ON (organization_id) status, current_period_end FROM \\\"_appstruct_billing_subscriptions\\\" ORDER BY organization_id, updated_at DESC, id DESC) current WHERE status = 'active' AND (current_period_end IS NULL OR current_period_end > CURRENT_TIMESTAMP)\")" } else { "None" })
    } else {
        "use crate::AppState;\nuse axum::Router;\npub fn router() -> Router<AppState> { Router::new() }".to_owned()
    };
    Ok(Artifact::text(
        "backend/src/business.rs",
        super::rust_template(&source)?,
        ArtifactKind::RustSource,
    ))
}
