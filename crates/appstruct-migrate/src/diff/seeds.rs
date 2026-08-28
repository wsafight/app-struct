use super::{destructive, planned, safe};
use crate::{DatabaseSchema, PlannedChange, SchemaChange};
use std::collections::BTreeMap;

pub(super) fn diff_seeds(
    before: &DatabaseSchema,
    after: &DatabaseSchema,
    changes: &mut Vec<PlannedChange>,
) {
    let old = before
        .seeds
        .iter()
        .map(|seed| (seed.id.as_str(), seed))
        .collect::<BTreeMap<_, _>>();
    let new = after
        .seeds
        .iter()
        .map(|seed| (seed.id.as_str(), seed))
        .collect::<BTreeMap<_, _>>();
    for seed in &after.seeds {
        match old.get(seed.id.as_str()) {
            None => changes.push(planned(
                SchemaChange::AddSeed { seed: seed.clone() },
                safe(),
            )),
            Some(previous) if *previous != seed => {
                changes.push(planned(
                    SchemaChange::RemoveSeed {
                        seed: (*previous).clone(),
                    },
                    destructive(),
                ));
                changes.push(planned(
                    SchemaChange::AddSeed { seed: seed.clone() },
                    safe(),
                ));
            }
            Some(_) => {}
        }
    }
    for seed in &before.seeds {
        if !new.contains_key(seed.id.as_str()) {
            changes.push(planned(
                SchemaChange::RemoveSeed { seed: seed.clone() },
                destructive(),
            ));
        }
    }
}
