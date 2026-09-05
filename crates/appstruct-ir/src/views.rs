use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityViewsIr {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub aggregates: Vec<AggregateIr>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_field: Option<crate::FieldId>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub soft_delete: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AggregateIr {
    pub name: String,
    pub child: crate::EntityId,
    pub relation: crate::FieldId,
    pub states: Vec<String>,
    pub max_items: u32,
}

#[allow(clippy::trivially_copy_pass_by_ref)]
fn is_false(value: &bool) -> bool {
    !value
}
