mod indexes;
mod seeds;
use self::indexes::diff_indexes;
use self::seeds::diff_seeds;
use crate::{
    ColumnSchema, DatabaseSchema, ForeignKeySchema, IndexSchema, TableSchema,
    UniqueConstraintSchema,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SchemaRisk {
    NonDestructive,
    RequiresInput,
    Destructive,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionRisk {
    Online,
    MayLock,
    ManualReview,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChangeRisk {
    pub schema: SchemaRisk,
    pub execution: ExecutionRisk,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SchemaChange {
    AddTable {
        table: TableSchema,
    },
    RemoveTable {
        table: TableSchema,
    },
    RenameTable {
        before: TableSchema,
        after: TableSchema,
    },
    AddColumn {
        table: String,
        column: ColumnSchema,
    },
    RemoveColumn {
        table: String,
        column: ColumnSchema,
    },
    AlterColumn {
        table: String,
        before: ColumnSchema,
        after: ColumnSchema,
    },
    AddUniqueConstraint {
        constraint: UniqueConstraintSchema,
    },
    RemoveUniqueConstraint {
        constraint: UniqueConstraintSchema,
    },
    AddIndex {
        index: IndexSchema,
    },
    RemoveIndex {
        index: IndexSchema,
    },
    AddSeed {
        seed: crate::SeedSchema,
    },
    RemoveSeed {
        seed: crate::SeedSchema,
    },
    AddForeignKey {
        foreign_key: ForeignKeySchema,
    },
    RemoveForeignKey {
        foreign_key: ForeignKeySchema,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlannedChange {
    pub change: SchemaChange,
    pub risk: ChangeRisk,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MigrationPlan {
    pub changes: Vec<PlannedChange>,
}

impl MigrationPlan {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.changes.is_empty()
    }

    #[must_use]
    pub fn is_blocked(&self) -> bool {
        self.changes.iter().any(|change| {
            change.risk.schema != SchemaRisk::NonDestructive
                || change.risk.execution != ExecutionRisk::Online
        })
    }
}

#[must_use]
pub fn diff(before: &DatabaseSchema, after: &DatabaseSchema) -> MigrationPlan {
    let mut changes = Vec::new();
    let old_tables = by_id(&before.tables, |table| table.id.as_str());
    let new_tables = by_id(&after.tables, |table| table.id.as_str());
    for table in &after.tables {
        match old_tables.get(table.id.as_str()) {
            None => changes.push(planned(
                SchemaChange::AddTable {
                    table: table.clone(),
                },
                safe(),
            )),
            Some(previous) => {
                if previous.name != table.name {
                    changes.push(planned(
                        SchemaChange::RenameTable {
                            before: (*previous).clone(),
                            after: table.clone(),
                        },
                        requires_input(),
                    ));
                }
                diff_columns(previous, table, &mut changes);
            }
        }
    }
    for table in &before.tables {
        if !new_tables.contains_key(table.id.as_str()) {
            changes.push(planned(
                SchemaChange::RemoveTable {
                    table: table.clone(),
                },
                destructive(),
            ));
        }
    }
    diff_unique_constraints(before, after, &old_tables, &mut changes);
    diff_indexes(before, after, &old_tables, &mut changes);
    diff_seeds(before, after, &mut changes);
    diff_foreign_keys(before, after, &old_tables, &mut changes);
    MigrationPlan { changes }
}

fn diff_unique_constraints(
    before: &DatabaseSchema,
    after: &DatabaseSchema,
    old_tables: &BTreeMap<&str, &TableSchema>,
    changes: &mut Vec<PlannedChange>,
) {
    let old = by_id(&before.unique_constraints, |constraint| {
        constraint.id.as_str()
    });
    let new = by_id(&after.unique_constraints, |constraint| {
        constraint.id.as_str()
    });
    for constraint in &after.unique_constraints {
        match old.get(constraint.id.as_str()) {
            None => changes.push(planned(
                SchemaChange::AddUniqueConstraint {
                    constraint: constraint.clone(),
                },
                if after.tables.iter().any(|table| {
                    table.name == constraint.table && !old_tables.contains_key(table.id.as_str())
                }) {
                    safe()
                } else {
                    may_lock()
                },
            )),
            Some(previous) if *previous != constraint => {
                changes.push(planned(
                    SchemaChange::RemoveUniqueConstraint {
                        constraint: (*previous).clone(),
                    },
                    destructive(),
                ));
                changes.push(planned(
                    SchemaChange::AddUniqueConstraint {
                        constraint: constraint.clone(),
                    },
                    may_lock(),
                ));
            }
            Some(_) => {}
        }
    }
    for constraint in &before.unique_constraints {
        if !new.contains_key(constraint.id.as_str()) {
            changes.push(planned(
                SchemaChange::RemoveUniqueConstraint {
                    constraint: constraint.clone(),
                },
                destructive(),
            ));
        }
    }
}

fn diff_columns(before: &TableSchema, after: &TableSchema, changes: &mut Vec<PlannedChange>) {
    let old_columns = by_id(&before.columns, |column| column.id.as_str());
    let new_columns = by_id(&after.columns, |column| column.id.as_str());
    for column in &after.columns {
        match old_columns.get(column.id.as_str()) {
            None => changes.push(planned(
                SchemaChange::AddColumn {
                    table: after.name.clone(),
                    column: column.clone(),
                },
                added_column_risk(column),
            )),
            Some(previous) if *previous != column => changes.push(planned(
                SchemaChange::AlterColumn {
                    table: after.name.clone(),
                    before: (*previous).clone(),
                    after: column.clone(),
                },
                altered_column_risk(previous, column),
            )),
            Some(_) => {}
        }
    }
    for column in &before.columns {
        if !new_columns.contains_key(column.id.as_str()) {
            changes.push(planned(
                SchemaChange::RemoveColumn {
                    table: before.name.clone(),
                    column: column.clone(),
                },
                destructive(),
            ));
        }
    }
}

fn diff_foreign_keys(
    before: &DatabaseSchema,
    after: &DatabaseSchema,
    old_tables: &BTreeMap<&str, &TableSchema>,
    changes: &mut Vec<PlannedChange>,
) {
    let old = by_id(&before.foreign_keys, |foreign_key| foreign_key.id.as_str());
    let new = by_id(&after.foreign_keys, |foreign_key| foreign_key.id.as_str());
    for foreign_key in &after.foreign_keys {
        match old.get(foreign_key.id.as_str()) {
            None => changes.push(planned(
                SchemaChange::AddForeignKey {
                    foreign_key: foreign_key.clone(),
                },
                if after.tables.iter().any(|table| {
                    table.name == foreign_key.source_table
                        && !old_tables.contains_key(table.id.as_str())
                }) {
                    safe()
                } else {
                    may_lock()
                },
            )),
            Some(previous) if *previous != foreign_key => {
                changes.push(planned(
                    SchemaChange::RemoveForeignKey {
                        foreign_key: (*previous).clone(),
                    },
                    destructive(),
                ));
                changes.push(planned(
                    SchemaChange::AddForeignKey {
                        foreign_key: foreign_key.clone(),
                    },
                    safe(),
                ));
            }
            Some(_) => {}
        }
    }
    for foreign_key in &before.foreign_keys {
        if !new.contains_key(foreign_key.id.as_str()) {
            changes.push(planned(
                SchemaChange::RemoveForeignKey {
                    foreign_key: foreign_key.clone(),
                },
                destructive(),
            ));
        }
    }
}

fn added_column_risk(column: &ColumnSchema) -> ChangeRisk {
    if column.unique {
        return ChangeRisk {
            schema: SchemaRisk::NonDestructive,
            execution: ExecutionRisk::ManualReview,
        };
    }
    if column.nullable {
        safe()
    } else {
        ChangeRisk {
            schema: SchemaRisk::RequiresInput,
            execution: ExecutionRisk::MayLock,
        }
    }
}

fn altered_column_risk(before: &ColumnSchema, after: &ColumnSchema) -> ChangeRisk {
    if before.name != after.name || before.nullable && !after.nullable {
        return requires_input();
    }
    if before.data_type != after.data_type
        || before.primary_key != after.primary_key
        || before.unique != after.unique
    {
        return destructive();
    }
    if before.generated != after.generated {
        return ChangeRisk {
            schema: SchemaRisk::RequiresInput,
            execution: ExecutionRisk::ManualReview,
        };
    }
    safe()
}

pub(super) fn by_id<T, F>(items: &[T], id: F) -> BTreeMap<&str, &T>
where
    F: Fn(&T) -> &str,
{
    items.iter().map(|item| (id(item), item)).collect()
}

pub(super) fn planned(change: SchemaChange, risk: ChangeRisk) -> PlannedChange {
    PlannedChange { change, risk }
}

pub(super) const fn safe() -> ChangeRisk {
    ChangeRisk {
        schema: SchemaRisk::NonDestructive,
        execution: ExecutionRisk::Online,
    }
}

pub(super) const fn may_lock() -> ChangeRisk {
    ChangeRisk {
        schema: SchemaRisk::NonDestructive,
        execution: ExecutionRisk::MayLock,
    }
}

pub(super) const fn requires_input() -> ChangeRisk {
    ChangeRisk {
        schema: SchemaRisk::RequiresInput,
        execution: ExecutionRisk::MayLock,
    }
}

pub(super) const fn destructive() -> ChangeRisk {
    ChangeRisk {
        schema: SchemaRisk::Destructive,
        execution: ExecutionRisk::ManualReview,
    }
}
