use crate::EntityId;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SeedIr {
    pub id: String,
    pub entity: EntityId,
    pub values: BTreeMap<String, String>,
}
