use appstruct_ir::AppIr;

pub(super) fn files(ir: &AppIr) -> Vec<(&'static str, String)> {
    let mut files = Vec::new();
    if ir.auth.enabled {
        files.push((
            "web/src/auth-interaction.test.tsx",
            include_str!("../../templates/web/auth-interaction.test.tsx").to_owned(),
        ));
    }
    if ir.tenant.enabled {
        files.push((
            "web/src/tenant.test.ts",
            include_str!("../../templates/web/tenant.test.ts").to_owned(),
        ));
    }
    files
}
