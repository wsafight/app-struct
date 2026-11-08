use appstruct_ir::AppIr;

pub(super) fn files(ir: &AppIr) -> Vec<(&'static str, String)> {
    let mut files = Vec::new();
    if ir.auth.enabled {
        files.push((
            "web/src/auth/return-to.test.ts",
            include_str!("../../templates/web/auth/return-to.test.ts").to_owned(),
        ));
        files.push((
            "web/src/auth-interaction.test.tsx",
            include_str!("../../templates/web/auth-interaction.test.tsx").replace(
                "\n\n__TENANT_MOCK__;\n\n",
                if ir.tenant.enabled {
                    "\n\nrs.mock(\"./tenant/Tenant\", () => ({ TenantSwitcher: () => null }));\n\n"
                } else {
                    "\n\n"
                },
            ),
        ));
    }
    if ir.tenant.enabled {
        files.push((
            "web/src/tenant.test.ts",
            include_str!("../../templates/web/tenant.test.ts").to_owned(),
        ));
    }
    if ir.report.entitlement.is_none()
        && let Some(template) = ir.report.templates.first()
    {
        files.push((
            "web/src/report/report-interaction.test.tsx",
            include_str!("../../templates/web/report/report-interaction.test.tsx")
                .replace("__REPORT_TEMPLATE_NAME__", &format!("{:?}", template.name)),
        ));
    }
    if let Some(resource) = ir.activity.resources.first() {
        files.push((
            "web/src/activity/activity-client.test.ts",
            include_str!("../../templates/web/activity/activity-client.test.ts")
                .replace("__ACTIVITY_RESOURCE__", &format!("{:?}", resource.resource)),
        ));
    }
    files
}
