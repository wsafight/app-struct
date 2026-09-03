use super::*;
use crate::{ColumnSchema, ForeignKeySchema, IndexSchema, TableSchema, UniqueConstraintSchema};
use appstruct_ir::{DatabaseProvider, GeneratedValueIr, OnDeleteIr};
use std::collections::BTreeMap;

#[test]
fn postgres_worker_partial_index_syntax_does_not_report_drift() {
    let cases: &[(&str, &[&str], &str)] = &[
        ("_appstruct_jobs", &["run_at", "id"], "queued"),
        ("_appstruct_jobs", &["locked_until", "id"], "running"),
        (
            "_appstruct_webhook_deliveries",
            &["next_attempt_at", "id"],
            "pending",
        ),
        (
            "_appstruct_webhook_deliveries",
            &["locked_until", "id"],
            "delivering",
        ),
    ];
    for &(table, columns, status) in cases {
        let expected =
            schema_with_index(table, columns, false, Some(&format!("status = '{status}'")));
        let actual = BTreeSet::from([index_shape(
            table,
            columns,
            false,
            Some(&format!("((status = '{status}'::text))")),
        )]);
        let mut issues = Vec::new();
        compare_indexes(&expected, &actual, &mut issues);
        assert!(issues.is_empty(), "status {status}: {issues:?}");
    }
}

#[test]
fn real_index_shape_changes_still_report_drift() {
    let expected = schema_with_index(
        "_appstruct_jobs",
        &["run_at", "id"],
        false,
        Some("status = 'queued'"),
    );
    let mut cases = Vec::new();

    let mut table = matching_index();
    table.table = "_appstruct_other_jobs".to_owned();
    cases.push(("table", table));

    let mut columns = matching_index();
    columns.columns.reverse();
    cases.push(("column order", columns));

    let mut unique = matching_index();
    unique.unique = true;
    cases.push(("uniqueness", unique));

    let mut predicate = matching_index();
    predicate.predicate = Some("(status = 'running'::text)".to_owned());
    cases.push(("predicate value", predicate));

    let mut missing_predicate = matching_index();
    missing_predicate.predicate = None;
    cases.push(("missing predicate", missing_predicate));

    for (label, actual) in cases {
        let mut issues = Vec::new();
        compare_indexes(&expected, &BTreeSet::from([actual]), &mut issues);
        assert_eq!(issues.len(), 2, "{label}: {issues:?}");
        assert!(
            issues.iter().any(|issue| issue.starts_with("missing ")),
            "{label}: {issues:?}"
        );
        assert!(
            issues.iter().any(|issue| issue.starts_with("unexpected ")),
            "{label}: {issues:?}"
        );
    }
}

fn schema_with_index(
    table: &str,
    columns: &[&str],
    unique: bool,
    predicate: Option<&str>,
) -> DatabaseSchema {
    DatabaseSchema {
        schema_version: crate::SCHEMA_VERSION,
        provider: DatabaseProvider::Postgres,
        tables: Vec::new(),
        unique_constraints: Vec::new(),
        indexes: vec![IndexSchema {
            id: "test::index".to_owned(),
            table: table.to_owned(),
            columns: columns.iter().map(|column| (*column).to_owned()).collect(),
            unique,
            predicate: predicate.map(str::to_owned),
        }],
        seeds: Vec::new(),
        foreign_keys: Vec::new(),
    }
}

fn index_shape(table: &str, columns: &[&str], unique: bool, predicate: Option<&str>) -> IndexShape {
    IndexShape {
        table: table.to_owned(),
        columns: columns.iter().map(|column| (*column).to_owned()).collect(),
        unique,
        predicate: predicate.map(str::to_owned),
    }
}

fn matching_index() -> IndexShape {
    index_shape(
        "_appstruct_jobs",
        &["run_at", "id"],
        false,
        Some("(status = 'queued'::text)"),
    )
}

#[test]
fn unique_index_messages_include_the_unique_prefix() {
    let expected = schema_with_index("notes", &["id"], true, None);
    let mut issues = Vec::new();
    compare_indexes(&expected, &BTreeSet::new(), &mut issues);
    assert_eq!(issues, ["missing unique index on `notes` (id)"]);

    issues.clear();
    compare_indexes(
        &empty_schema(),
        &BTreeSet::from([index_shape("notes", &["id"], true, None)]),
        &mut issues,
    );
    assert_eq!(issues, ["unexpected unique index on `notes` (id)"]);
}

#[test]
fn compare_unique_constraints_reports_missing_and_unexpected_shapes() {
    let expected = DatabaseSchema {
        unique_constraints: vec![UniqueConstraintSchema {
            id: "notes::email".to_owned(),
            table: "notes".to_owned(),
            columns: vec!["email".to_owned()],
        }],
        ..empty_schema()
    };
    let mut issues = Vec::new();
    compare_unique_constraints(&expected, &BTreeSet::new(), &mut issues);
    assert_eq!(issues, ["missing unique constraint `notes(email)`"]);

    issues.clear();
    compare_unique_constraints(
        &empty_schema(),
        &BTreeSet::from([UniqueConstraintShape {
            table: "notes".to_owned(),
            columns: vec!["email".to_owned()],
        }]),
        &mut issues,
    );
    assert_eq!(issues, ["unexpected unique constraint `notes(email)`"]);
}

#[test]
fn compare_foreign_keys_covers_on_delete_variants() {
    let expected = DatabaseSchema {
        foreign_keys: vec![
            foreign_key("notes", "author_id", "users", "id", OnDeleteIr::Restrict),
            foreign_key("notes", "org_id", "orgs", "id", OnDeleteIr::Cascade),
            foreign_key("notes", "parent_id", "notes", "id", OnDeleteIr::SetNull),
        ],
        ..empty_schema()
    };
    let actual = BTreeSet::from([
        ForeignKeyShape {
            source_table: "notes".to_owned(),
            source_columns: vec!["author_id".to_owned()],
            target_table: "users".to_owned(),
            target_columns: vec!["id".to_owned()],
            on_delete: "RESTRICT".to_owned(),
        },
        ForeignKeyShape {
            source_table: "notes".to_owned(),
            source_columns: vec!["org_id".to_owned()],
            target_table: "orgs".to_owned(),
            target_columns: vec!["id".to_owned()],
            on_delete: "CASCADE".to_owned(),
        },
        ForeignKeyShape {
            source_table: "notes".to_owned(),
            source_columns: vec!["parent_id".to_owned()],
            target_table: "notes".to_owned(),
            target_columns: vec!["id".to_owned()],
            on_delete: "SET NULL".to_owned(),
        },
    ]);
    let mut issues = Vec::new();
    compare_foreign_keys(&expected, &actual, &mut issues);
    assert!(issues.is_empty(), "{issues:?}");

    issues.clear();
    compare_foreign_keys(&expected, &BTreeSet::new(), &mut issues);
    assert_eq!(issues.len(), 3);
    assert!(issues.iter().all(|issue| issue.starts_with("missing ")));

    issues.clear();
    compare_foreign_keys(&empty_schema(), &actual, &mut issues);
    assert_eq!(issues.len(), 3);
    assert!(issues.iter().all(|issue| issue.starts_with("unexpected ")));
}

#[test]
fn compare_columns_reports_missing_unexpected_and_attribute_drift() {
    let table = TableSchema {
        id: "notes".to_owned(),
        name: "notes".to_owned(),
        columns: vec![
            column("id", DatabaseType::Uuid, true, false, None, None),
            column(
                "status",
                DatabaseType::Enum {
                    values: vec!["draft".to_owned(), "active".to_owned()],
                },
                false,
                false,
                None,
                None,
            ),
        ],
    };
    let mut actual = BTreeMap::new();
    actual.insert(
        ("notes".to_owned(), "id".to_owned()),
        ActualColumn {
            data_type: "uuid".to_owned(),
            nullable: false,
            default: None,
            identity: false,
        },
    );
    actual.insert(
        ("notes".to_owned(), "extra".to_owned()),
        ActualColumn {
            data_type: "text".to_owned(),
            nullable: true,
            default: None,
            identity: false,
        },
    );
    let mut constraints = BTreeMap::new();
    constraints.insert(
        ("notes".to_owned(), "id".to_owned()),
        BTreeSet::from(["PRIMARY KEY".to_owned()]),
    );
    let mut issues = Vec::new();
    compare_columns(&table, &actual, &constraints, &BTreeMap::new(), &mut issues);
    assert!(
        issues
            .iter()
            .any(|issue| issue.contains("missing column `notes.status`")),
        "{issues:?}"
    );
    assert!(
        issues
            .iter()
            .any(|issue| issue.contains("unexpected column `notes.extra`")),
        "{issues:?}"
    );
}

#[test]
fn compare_column_reports_type_nullability_key_identity_and_default_drift() {
    let table = TableSchema {
        id: "items".to_owned(),
        name: "items".to_owned(),
        columns: Vec::new(),
    };
    let expected = column(
        "count",
        DatabaseType::Integer,
        true,
        false,
        Some("0"),
        Some(GeneratedValueIr::AutoIncrement),
    );
    let actual = ActualColumn {
        data_type: "bigint".to_owned(),
        nullable: true,
        default: Some("'1'::integer".to_owned()),
        identity: false,
    };
    let mut issues = Vec::new();
    compare_column(&table, &expected, &actual, None, &mut issues);
    assert!(issues.iter().any(|issue| issue.contains("has type")));
    assert!(issues.iter().any(|issue| issue.contains("nullability")));
    assert!(issues.iter().any(|issue| issue.contains("primary-key")));
    assert!(issues.iter().any(|issue| issue.contains("identity")));
    assert!(issues.iter().any(|issue| issue.contains("default")));

    issues.clear();
    let unique = column("email", DatabaseType::Text, false, true, None, None);
    compare_column(
        &table,
        &unique,
        &ActualColumn {
            data_type: "text".to_owned(),
            nullable: false,
            default: None,
            identity: false,
        },
        None,
        &mut issues,
    );
    assert!(issues.iter().any(|issue| issue.contains("unique status")));
}

#[test]
fn compare_enum_check_accepts_matching_literals_and_rejects_drift() {
    let mut checks = BTreeMap::new();
    checks.insert(
        "notes".to_owned(),
        vec!["CHECK ((status = ANY (ARRAY['draft'::text, 'active'::text])))".to_owned()],
    );
    let mut issues = Vec::new();
    compare_enum_check(
        "notes",
        "status",
        &["draft".to_owned(), "active".to_owned()],
        &checks,
        &mut issues,
    );
    assert!(issues.is_empty(), "{issues:?}");

    compare_enum_check(
        "notes",
        "status",
        &["draft".to_owned(), "archived".to_owned()],
        &checks,
        &mut issues,
    );
    assert!(
        issues
            .iter()
            .any(|issue| issue.contains("enum check for `notes.status`")),
        "{issues:?}"
    );
}

fn empty_schema() -> DatabaseSchema {
    DatabaseSchema {
        schema_version: crate::SCHEMA_VERSION,
        provider: DatabaseProvider::Postgres,
        tables: Vec::new(),
        unique_constraints: Vec::new(),
        indexes: Vec::new(),
        seeds: Vec::new(),
        foreign_keys: Vec::new(),
    }
}

fn column(
    name: &str,
    data_type: DatabaseType,
    primary_key: bool,
    unique: bool,
    default: Option<&str>,
    generated: Option<GeneratedValueIr>,
) -> ColumnSchema {
    ColumnSchema {
        id: format!("test::{name}"),
        name: name.to_owned(),
        data_type,
        nullable: false,
        primary_key,
        unique,
        default: default.map(str::to_owned),
        generated,
    }
}

fn foreign_key(
    source_table: &str,
    source_column: &str,
    target_table: &str,
    target_column: &str,
    on_delete: OnDeleteIr,
) -> ForeignKeySchema {
    ForeignKeySchema {
        id: format!("{source_table}::{source_column}"),
        source_table: source_table.to_owned(),
        source_columns: vec![source_column.to_owned()],
        target_table: target_table.to_owned(),
        target_columns: vec![target_column.to_owned()],
        unique: false,
        on_delete,
    }
}
