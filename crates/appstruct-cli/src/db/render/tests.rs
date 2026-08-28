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
    assert!(
        first
            .source
            .contains("`bytea` is represented as a JSON placeholder")
    );
    assert!(!first.source.contains("    access:\n"));
    assert!(
        first
            .warnings
            .iter()
            .any(|warning| warning.contains("native PostgreSQL enum"))
    );
    assert!(
        first
            .warnings
            .iter()
            .any(|warning| warning.contains("composite unique constraint"))
    );
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
        nullable,
        default: None,
        identity: false,
        generated: false,
        max_length: None,
        enum_values: Vec::new(),
    }
}
