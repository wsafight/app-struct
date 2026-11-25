use std::path::Path;

#[test]
fn pro_vertical_starters_compile_and_generate_schema_with_rls_masks_computed_and_dashboard() {
    for starter in ["starters/b2b-crm-ticketing", "starters/ai-workbench"] {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join(starter);
        let ir = appstruct_compiler::compile_project(&root)
            .unwrap_or_else(|errors| panic!("{starter} failed to compile: {errors:?}"));
        assert!(
            ir.tenant.enabled && ir.tenant.rls,
            "{starter} should enable RLS"
        );
        assert!(
            ir.dashboard.as_ref().is_some_and(|d| d.widgets.len() == 2),
            "{starter} should declare 2 cross-resource dashboard widgets"
        );
        let schema = appstruct_schema::extract(&ir)
            .unwrap_or_else(|error| panic!("{starter} schema extraction failed: {error}"));
        assert!(
            !schema.rls_tables.is_empty(),
            "{starter} should emit RLS tables"
        );
        let sql = appstruct_schema::initial_migration(&schema);
        assert!(
            sql.contains("ENABLE ROW LEVEL SECURITY"),
            "{starter} initial migration should enable PostgreSQL RLS"
        );
        let artifacts = appstruct_codegen::plan(&ir)
            .unwrap_or_else(|error| panic!("{starter} codegen failed: {error}"));
        assert!(
            !artifacts.is_empty(),
            "{starter} should produce generated artifacts"
        );
        assert!(
            ir.entities
                .iter()
                .any(|entity| !entity.views.computed.is_empty()),
            "{starter} should include at least one computed field"
        );
        assert!(
            ir.entities
                .iter()
                .any(|entity| entity.fields.iter().any(|field| field.mask.is_some())),
            "{starter} should include at least one role-based masked field"
        );
        assert!(
            ir.commands.iter().any(|command| command.http.is_some()),
            "{starter} should include at least one declarative HTTP command"
        );
    }
}
