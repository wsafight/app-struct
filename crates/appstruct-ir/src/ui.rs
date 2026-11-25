use crate::FieldId;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FieldSemanticIr {
    Money {
        currency_field: FieldId,
        fraction_digits: u8,
    },
    File,
    Files,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MaskStrategyIr {
    Email,
    Phone,
    Last4,
    Redact,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FieldMaskIr {
    pub strategy: MaskStrategyIr,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub except_roles: Vec<String>,
}
