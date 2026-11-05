use super::{app_display_name, with_app_title};
use crate::{Artifact, ArtifactKind};
use appstruct_ir::AppIr;

#[allow(clippy::too_many_lines)]
pub(super) fn extend_artifacts(ir: &AppIr, artifacts: &mut Vec<Artifact>) {
    let title = app_display_name(&ir.app.name);
    if ir.file.enabled && ir.auth.enabled {
        artifacts.push(Artifact::text(
            "web/src/file/RecordAttachments.tsx",
            include_str!("../../templates/web/file/RecordAttachments.tsx"),
            ArtifactKind::Web,
        ));
        artifacts.push(Artifact::text(
            "web/src/file/FilesPage.tsx",
            include_str!("../../templates/web/file/FilesPage.tsx"),
            ArtifactKind::Web,
        ));
    }
    if ir.auth.enabled {
        artifacts.push(Artifact::text(
            "web/src/product/ProductPages.tsx",
            include_str!("../../templates/web/product/ProductPages.tsx"),
            ArtifactKind::Web,
        ));
        artifacts.push(Artifact::text(
            "web/src/auth/AccountSettings.tsx",
            include_str!("../../templates/web/auth/AccountSettings.tsx"),
            ArtifactKind::Web,
        ));
        artifacts.extend([
            Artifact::text(
                "web/src/business/BusinessPage.tsx",
                include_str!("../../templates/web/business/BusinessPage.tsx"),
                ArtifactKind::Web,
            ),
            Artifact::text(
                "web/src/auth/AccountPage.tsx",
                include_str!("../../templates/web/auth/AccountPage.tsx"),
                ArtifactKind::Web,
            ),
            Artifact::text(
                "web/src/auth/return-to.ts",
                include_str!("../../templates/web/auth/return-to.ts"),
                ArtifactKind::Web,
            ),
            Artifact::text(
                "web/src/auth/Auth.tsx",
                include_str!("../../templates/web/auth/Auth.tsx"),
                ArtifactKind::Web,
            ),
            Artifact::text(
                "web/src/auth/AuthPages.tsx",
                with_app_title(
                    include_str!("../../templates/web/auth/AuthPages.tsx"),
                    &title,
                ),
                ArtifactKind::Web,
            ),
            Artifact::text(
                "web/src/auth/PublicAuthPages.tsx",
                with_app_title(
                    include_str!("../../templates/web/auth/PublicAuthPages.tsx"),
                    &title,
                ),
                ArtifactKind::Web,
            ),
            Artifact::text(
                "web/src/auth/AdminStoragePages.tsx",
                include_str!("../../templates/web/auth/AdminStoragePages.tsx"),
                ArtifactKind::Web,
            ),
            Artifact::text(
                "web/src/auth/AdminSchedulesPage.tsx",
                include_str!("../../templates/web/auth/AdminSchedulesPage.tsx"),
                ArtifactKind::Web,
            ),
        ]);
    }
    if ir.billing.enabled {
        artifacts.extend([
            Artifact::text(
                "web/src/billing/BillingAccess.tsx",
                include_str!("../../templates/web/billing/BillingAccess.tsx"),
                ArtifactKind::Web,
            ),
            Artifact::text(
                "web/src/billing/BillingPage.tsx",
                include_str!("../../templates/web/billing/BillingPage.tsx"),
                ArtifactKind::Web,
            ),
            Artifact::text(
                "web/src/billing/AdminBillingPage.tsx",
                include_str!("../../templates/web/billing/AdminBillingPage.tsx"),
                ArtifactKind::Web,
            ),
        ]);
    }
    if ir.tenant.enabled {
        artifacts.push(Artifact::text(
            "web/src/tenant/Tenant.tsx",
            with_app_title(
                include_str!("../../templates/web/tenant/Tenant.tsx"),
                &title,
            ),
            ArtifactKind::Web,
        ));
    }
    if ir.audit.enabled {
        artifacts.extend([
            Artifact::text(
                "web/src/audit/AuditPage.tsx",
                include_str!("../../templates/web/audit/AuditPage.tsx"),
                ArtifactKind::Web,
            ),
            Artifact::text(
                "web/src/audit/RecordHistory.tsx",
                include_str!("../../templates/web/audit/RecordHistory.tsx"),
                ArtifactKind::Web,
            ),
        ]);
    }
    if ir.report.enabled {
        artifacts.push(Artifact::text(
            "web/src/product/TaskCenterPage.tsx",
            include_str!("../../templates/web/product/TaskCenterPage.tsx"),
            ArtifactKind::Web,
        ));
        artifacts.push(Artifact::text(
            "web/src/report/ReportPage.tsx",
            include_str!("../../templates/web/report/ReportPage.tsx")
                .replace(
                    "__BILLING_IMPORT__",
                    if ir.report.entitlement.is_some() {
                        "import { BillingAccess } from \"../billing/BillingAccess\";"
                    } else {
                        ""
                    },
                )
                .replace(
                    "__BILLING_OPEN__",
                    &ir.report
                        .entitlement
                        .as_ref()
                        .map_or_else(String::new, |feature| {
                            format!("<BillingAccess entitlement={feature:?}>")
                        }),
                )
                .replace(
                    "__BILLING_CLOSE__",
                    if ir.report.entitlement.is_some() {
                        "</BillingAccess>"
                    } else {
                        ""
                    },
                ),
            ArtifactKind::Web,
        ));
    }
    if ir.activity.enabled {
        let activity_realtime = if ir.realtime.enabled {
            include_str!("../../templates/web/activity/useActivityRealtime.ts")
        } else {
            include_str!("../../templates/web/activity/useActivityRealtimeDisabled.ts")
        };
        artifacts.extend([
            Artifact::text(
                "web/src/activity/ActivityTimeline.tsx",
                include_str!("../../templates/web/activity/ActivityTimeline.tsx"),
                ArtifactKind::Web,
            ),
            Artifact::text(
                "web/src/activity/useActivityRealtime.ts",
                activity_realtime,
                ArtifactKind::Web,
            ),
        ]);
    }
}
