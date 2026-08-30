use super::*;

#[test]
fn renders_deterministic_entities_relations_and_review_warnings() {
    let schema = fixture();
    let first = render(&schema);
    let second = render(&schema);

    assert_eq!(first.source, second.source);
    assert_eq!(first.warnings, second.warnings);
    assert_eq!(first.entity_count, 2);
    assert!(first.source.contains("  Project:\n"));
    assert!(first.source.contains("    table: \"projects\"\n"));
    assert!(first.source.contains("        type: enum\n"));
    assert!(
        first
            .source
            .contains("        values: [\"planned\", \"active\"]\n")
    );
    assert!(first.source.contains("        default: \"planned\"\n"));
    assert!(first.source.contains("  Task:\n"));
    assert!(first.source.contains(
        "      project:\n        type: relation\n        target: Project\n        column: \"project_id\"\n        on_delete: cascade\n"
    ));
    assert!(first.source.contains("        generated: auto_increment\n"));
    assert!(!first.source.contains("      payload:\n"));
    assert!(first.warnings.iter().any(|warning| {
        warning.contains("tasks.payload")
            && warning.contains("bytea")
            && warning.contains("omitted")
    }));
    assert!(!first.source.contains("    access:\n"));
    assert!(
        first
            .warnings
            .iter()
            .any(|warning| warning.contains("native PostgreSQL enum"))
    );
    assert!(first.source.contains("    indexes:\n"));
    assert!(first.source.contains("unique: true"));
    assert!(
        first
            .warnings
            .iter()
            .any(|warning| warning.contains("audit_entries") && warning.contains("omitted"))
    );
}

#[test]
fn normalizes_quoted_identifiers_but_preserves_database_names() {
    let schema = IntrospectedSchema {
        name: "public".to_owned(),
        tables: vec![IntrospectedTable {
            name: "CRM Contacts".to_owned(),
            columns: vec![
                column("Contact ID", "uuid", false),
                column("Display Name", "character varying", false),
            ],
            primary_key: vec!["Contact ID".to_owned()],
            unique_constraints: Vec::new(),
            indexes: Vec::new(),
        }],
        foreign_keys: Vec::new(),
    };

    let draft = render(&schema);
    assert!(draft.source.contains("  CrmContact:\n"));
    assert!(draft.source.contains("    table: \"CRM Contacts\"\n"));
    assert!(draft.source.contains("      contact_id:\n"));
    assert!(draft.source.contains("        column: \"Contact ID\"\n"));
    assert!(draft.source.contains("      display_name:\n"));
    assert!(draft.source.contains("        column: \"Display Name\"\n"));
}

#[test]
fn imports_domain_base_types_and_omits_incompatible_columns() {
    let mut code = column("code", "character varying", false);
    code.domain_schema = Some("public".to_owned());
    code.domain_name = Some("short_code".to_owned());
    code.max_length = Some(12);
    let mut generated = column("search_name", "text", true);
    generated.generated = true;
    let schema = IntrospectedSchema {
        name: "public".to_owned(),
        tables: vec![IntrospectedTable {
            name: "projects".to_owned(),
            columns: vec![
                column("id", "uuid", false),
                code,
                generated,
                column("tags", "ARRAY", true),
            ],
            primary_key: vec!["id".to_owned()],
            unique_constraints: Vec::new(),
            indexes: vec![IntrospectedIndex {
                name: "idx_projects_tags".to_owned(),
                columns: vec!["tags".to_owned()],
                unique: false,
                predicate: None,
            }],
        }],
        foreign_keys: Vec::new(),
    };

    let draft = render(&schema);
    assert!(draft.source.contains(
        "      code:\n        type: string\n        required: true\n        max_length: 12\n"
    ));
    assert!(!draft.source.contains("      search_name:\n"));
    assert!(!draft.source.contains("      tags:\n"));
    assert!(!draft.source.contains("    indexes:\n"));
    assert!(
        draft
            .warnings
            .iter()
            .any(|warning| warning.contains("public.short_code"))
    );
    assert!(draft.warnings.iter().any(|warning| {
        warning.contains("projects.search_name") && warning.contains("omitted")
    }));
    assert!(
        draft
            .warnings
            .iter()
            .any(|warning| { warning.contains("projects.tags") && warning.contains("ARRAY") })
    );
}

fn fixture() -> IntrospectedSchema {
    let mut state = column("state", "USER-DEFINED", false);
    state.udt_schema = "public".to_owned();
    state.udt_name = "project_state".to_owned();
    state.enum_values = vec!["planned".to_owned(), "active".to_owned()];
    state.default = Some("'planned'::project_state".to_owned());
    let mut created = column("created_at", "timestamp with time zone", false);
    created.default = Some("CURRENT_TIMESTAMP".to_owned());
    let mut task_id = column("id", "bigint", false);
    task_id.identity = true;
    IntrospectedSchema {
        name: "public".to_owned(),
        tables: vec![
            IntrospectedTable {
                name: "audit_entries".to_owned(),
                columns: vec![column("message", "text", false)],
                primary_key: Vec::new(),
                unique_constraints: Vec::new(),
                indexes: Vec::new(),
            },
            IntrospectedTable {
                name: "projects".to_owned(),
                columns: vec![
                    column("id", "uuid", false),
                    column("name", "character varying", false),
                    state,
                    created,
                ],
                primary_key: vec!["id".to_owned()],
                unique_constraints: vec![vec!["name".to_owned()]],
                indexes: Vec::new(),
            },
            IntrospectedTable {
                name: "tasks".to_owned(),
                columns: vec![
                    task_id,
                    column("project_id", "uuid", false),
                    column("title", "text", false),
                    column("payload", "bytea", true),
                ],
                primary_key: vec!["id".to_owned()],
                unique_constraints: vec![vec!["project_id".to_owned(), "title".to_owned()]],
                indexes: Vec::new(),
            },
        ],
        foreign_keys: vec![IntrospectedForeignKey {
            name: "tasks_project_id_fkey".to_owned(),
            source_table: "tasks".to_owned(),
            source_columns: vec!["project_id".to_owned()],
            target_schema: "public".to_owned(),
            target_table: "projects".to_owned(),
            target_columns: vec!["id".to_owned()],
            on_delete: "cascade".to_owned(),
        }],
    }
}

fn column(name: &str, data_type: &str, nullable: bool) -> IntrospectedColumn {
    IntrospectedColumn {
        name: name.to_owned(),
        data_type: data_type.to_owned(),
        udt_schema: "pg_catalog".to_owned(),
        udt_name: data_type.to_owned(),
        domain_schema: None,
        domain_name: None,
        nullable,
        default: None,
        identity: false,
        generated: false,
        max_length: None,
        enum_values: Vec::new(),
    }
}
