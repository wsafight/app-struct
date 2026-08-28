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
