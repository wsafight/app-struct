use super::{app_display_name, with_app_title};
use crate::{Artifact, ArtifactKind};
use appstruct_ir::AppIr;

pub(super) fn extend_artifacts(ir: &AppIr, artifacts: &mut Vec<Artifact>) {
    let title = app_display_name(&ir.app.name);
    if ir.auth.enabled {
        artifacts.extend([
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
        artifacts.push(Artifact::text(
            "web/src/billing/BillingPage.tsx",
            include_str!("../../templates/web/billing/BillingPage.tsx"),
            ArtifactKind::Web,
        ));
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
            "web/src/report/ReportPage.tsx",
            include_str!("../../templates/web/report/ReportPage.tsx"),
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
