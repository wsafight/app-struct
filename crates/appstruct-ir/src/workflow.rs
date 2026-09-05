use crate::{AccessRuleIr, EntityIr, FieldId, FieldIr};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkflowIr {
    pub field: FieldId,
    pub initial: String,
    pub transitions: Vec<WorkflowTransitionIr>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkflowTransitionIr {
    pub name: String,
    pub from: Vec<String>,
    pub to: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input: Option<String>,
    pub access: AccessRuleIr,
}

impl EntityIr {
    #[must_use]
    pub fn workflow_field(&self) -> Option<&FieldIr> {
        let field = &self.workflow.as_ref()?.field;
        self.fields.iter().find(|candidate| candidate.id == *field)
    }

    #[must_use]
    pub fn is_workflow_field(&self, field: &FieldIr) -> bool {
        self.workflow
            .as_ref()
            .is_some_and(|workflow| workflow.field == field.id)
    }
}
