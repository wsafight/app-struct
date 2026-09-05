use appstruct_ir::AppIr;

pub(super) fn source(ir: &AppIr) -> String {
    include_str!("../../templates/web/saved-views-client.ts")
        .replace(
            "__SERVER_ENABLED__",
            if ir.auth.enabled { "true" } else { "false" },
        )
        .replace(
            "__TEAM_ENABLED__",
            if ir.tenant.enabled { "true" } else { "false" },
        )
}
