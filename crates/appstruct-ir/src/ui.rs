use crate::FieldId;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FieldSemanticIr {
    Money {
        currency_field: FieldId,
        fraction_digits: u8,
    },
}
