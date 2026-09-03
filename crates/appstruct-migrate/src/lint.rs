use crate::{ExecutionRisk, MigrationPlan, SchemaChange, SchemaRisk};
use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LintSeverity {
    Warning,
    Error,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct MigrationLint {
    pub code: &'static str,
    pub severity: LintSeverity,
    pub message: String,
}

#[must_use]
pub fn lint_plan(plan: &MigrationPlan) -> Vec<MigrationLint> {
    let mut issues = Vec::new();
    for planned in &plan.changes {
        let label = change_label(&planned.change);
        if planned.risk.schema == SchemaRisk::Destructive {
            issues.push(MigrationLint {
                code: "AS4201",
                severity: LintSeverity::Error,
                message: format!("{label} is destructive; write and review a manual migration"),
            });
        } else if planned.risk.execution == ExecutionRisk::MayLock {
            issues.push(MigrationLint {
                code: "AS4202",
                severity: LintSeverity::Warning,
                message: format!(
                    "{label} may lock existing rows; schedule it outside peak traffic"
                ),
            });
        } else if planned.risk.execution == ExecutionRisk::ManualReview {
            issues.push(MigrationLint {
                code: "AS4203",
                severity: LintSeverity::Error,
                message: format!("{label} requires manual SQL review before applying"),
            });
        }
        if let SchemaChange::AddColumn { table, column } = &planned.change
            && !column.nullable
            && column.default.is_none()
            && column.generated.is_none()
        {
            issues.push(MigrationLint {
                code: "AS4204",
                severity: LintSeverity::Error,
                message: format!(
                    "adding non-null column `{}.{}` without a default can fail on existing rows",
                    table, column.name
                ),
            });
        }
    }
    issues.sort_by(|left, right| {
        left.code
            .cmp(right.code)
            .then(left.message.cmp(&right.message))
    });
    issues
}

fn change_label(change: &SchemaChange) -> String {
    match change {
        SchemaChange::AddTable { table } => format!("add table `{}`", table.name),
        SchemaChange::RemoveTable { table } => format!("remove table `{}`", table.name),
        SchemaChange::RenameTable { before, after } => {
            format!("rename table `{}` to `{}`", before.name, after.name)
        }
        SchemaChange::AddColumn { table, column } => {
            format!("add column `{table}.{}`", column.name)
        }
        SchemaChange::RemoveColumn { table, column } => {
            format!("remove column `{table}.{}`", column.name)
        }
        SchemaChange::AlterColumn { table, after, .. } => {
            format!("alter column `{table}.{}`", after.name)
        }
        SchemaChange::AddUniqueConstraint { constraint } => {
            format!("add unique constraint `{}`", constraint.id)
        }
        SchemaChange::RemoveUniqueConstraint { constraint } => {
            format!("remove unique constraint `{}`", constraint.id)
        }
        SchemaChange::AddIndex { index } => format!("add index `{}`", index.id),
        SchemaChange::RemoveIndex { index } => format!("remove index `{}`", index.id),
        SchemaChange::AddSeed { seed } => format!("add seed `{}`", seed.id),
        SchemaChange::RemoveSeed { seed } => format!("remove seed `{}`", seed.id),
        SchemaChange::AddForeignKey { foreign_key } => {
            format!("add foreign key `{}`", foreign_key.id)
        }
        SchemaChange::RemoveForeignKey { foreign_key } => {
            format!("remove foreign key `{}`", foreign_key.id)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ChangeRisk, ColumnSchema, DatabaseType, ExecutionRisk, ForeignKeySchema, IndexSchema,
        PlannedChange, SchemaRisk, SeedSchema, TableSchema, UniqueConstraintSchema,
    };
    use appstruct_ir::OnDeleteIr;

    fn table() -> TableSchema {
        TableSchema {
            id: "notes".to_owned(),
            name: "notes".to_owned(),
            columns: Vec::new(),
        }
    }

    fn column() -> ColumnSchema {
        ColumnSchema {
            id: "notes.id".to_owned(),
            name: "id".to_owned(),
            data_type: DatabaseType::Uuid,
            nullable: false,
            primary_key: true,
            unique: false,
            default: None,
            generated: None,
        }
    }

    #[test]
    fn labels_and_risk_classes_cover_every_change_kind() {
        let table = table();
        let column = column();
        let constraint = UniqueConstraintSchema {
            id: "notes.email".to_owned(),
            table: "notes".to_owned(),
            columns: vec!["email".to_owned()],
        };
        let index = IndexSchema {
            id: "notes.email".to_owned(),
            table: "notes".to_owned(),
            columns: vec!["email".to_owned()],
            unique: false,
            predicate: None,
        };
        let seed = SeedSchema {
            id: "notes.demo".to_owned(),
            table: "notes".to_owned(),
            values: Vec::new(),
        };
        let foreign_key = ForeignKeySchema {
            id: "notes.author".to_owned(),
            source_table: "notes".to_owned(),
            source_columns: vec!["author_id".to_owned()],
            target_table: "users".to_owned(),
            target_columns: vec!["id".to_owned()],
            unique: false,
            on_delete: OnDeleteIr::Restrict,
        };
        let changes = [
            SchemaChange::AddTable {
                table: table.clone(),
            },
            SchemaChange::RemoveTable {
                table: table.clone(),
            },
            SchemaChange::RenameTable {
                before: table.clone(),
                after: table,
            },
            SchemaChange::AddColumn {
                table: "notes".to_owned(),
                column: column.clone(),
            },
            SchemaChange::RemoveColumn {
                table: "notes".to_owned(),
                column: column.clone(),
            },
            SchemaChange::AlterColumn {
                table: "notes".to_owned(),
                before: column.clone(),
                after: column,
            },
            SchemaChange::AddUniqueConstraint {
                constraint: constraint.clone(),
            },
            SchemaChange::RemoveUniqueConstraint { constraint },
            SchemaChange::AddIndex {
                index: index.clone(),
            },
            SchemaChange::RemoveIndex { index },
            SchemaChange::AddSeed { seed: seed.clone() },
            SchemaChange::RemoveSeed { seed },
            SchemaChange::AddForeignKey {
                foreign_key: foreign_key.clone(),
            },
            SchemaChange::RemoveForeignKey { foreign_key },
        ];
        let plan = MigrationPlan {
            changes: changes
                .into_iter()
                .enumerate()
                .map(|(index, change)| PlannedChange {
                    change,
                    risk: ChangeRisk {
                        schema: if index == 0 {
                            SchemaRisk::Destructive
                        } else if index == 1 {
                            SchemaRisk::RequiresInput
                        } else {
                            SchemaRisk::NonDestructive
                        },
                        execution: if index == 1 {
                            ExecutionRisk::ManualReview
                        } else if index == 2 {
                            ExecutionRisk::MayLock
                        } else {
                            ExecutionRisk::Online
                        },
                    },
                })
                .collect(),
        };
        let issues = lint_plan(&plan);
        assert!(issues.iter().any(|issue| issue.code == "AS4201"));
        assert!(issues.iter().any(|issue| issue.code == "AS4202"));
        assert!(issues.iter().any(|issue| issue.code == "AS4203"));
        for change in &plan.changes {
            assert!(!change_label(&change.change).is_empty());
        }
    }
}
