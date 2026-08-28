use super::{by_id, destructive, may_lock, planned, safe};
use crate::{DatabaseSchema, PlannedChange, SchemaChange, TableSchema};
use std::collections::BTreeMap;

pub(super) fn diff_indexes(
    before: &DatabaseSchema,
    after: &DatabaseSchema,
    old_tables: &BTreeMap<&str, &TableSchema>,
    changes: &mut Vec<PlannedChange>,
) {
    let old = by_id(&before.indexes, |index| index.id.as_str());
    let new = by_id(&after.indexes, |index| index.id.as_str());
    for index in &after.indexes {
        match old.get(index.id.as_str()) {
            None => changes.push(planned(
                SchemaChange::AddIndex {
                    index: index.clone(),
                },
                if after.tables.iter().any(|table| {
                    table.name == index.table && !old_tables.contains_key(table.id.as_str())
                }) {
                    safe()
                } else {
                    may_lock()
                },
            )),
            Some(previous) if *previous != index => {
                changes.push(planned(
                    SchemaChange::RemoveIndex {
                        index: (*previous).clone(),
                    },
                    destructive(),
                ));
                changes.push(planned(
                    SchemaChange::AddIndex {
                        index: index.clone(),
                    },
                    may_lock(),
                ));
            }
            Some(_) => {}
        }
    }
    for index in &before.indexes {
        if !new.contains_key(index.id.as_str()) {
            changes.push(planned(
                SchemaChange::RemoveIndex {
                    index: index.clone(),
                },
                destructive(),
            ));
        }
    }
}
