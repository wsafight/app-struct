use crate::{AccessRuleIr, EntityId, FieldTypeIr};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValueObjectIr {
    pub id: String,
    pub rust_name: String,
    pub fields: Vec<ValueFieldIr>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValueFieldIr {
    pub rust_name: String,
    pub ty: FieldTypeIr,
    pub required: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum OperationTypeIr {
    Entity { entity: EntityId },
    ValueObject { value_object: String },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandIr {
    pub id: String,
    pub rust_name: String,
    pub input: OperationTypeIr,
    pub output: OperationTypeIr,
    pub access: AccessRuleIr,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueryIr {
    pub id: String,
    pub rust_name: String,
    pub input: Option<OperationTypeIr>,
    pub output: OperationTypeIr,
    pub access: AccessRuleIr,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PageIr {
    pub id: String,
    pub rust_name: String,
    pub label: String,
    pub path: String,
    pub component: String,
}
