use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityViewsIr {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub aggregates: Vec<AggregateIr>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub charts: Vec<ChartIr>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_field: Option<crate::FieldId>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub soft_delete: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChartIr {
    pub name: String,
    pub label: String,
    pub kind: ChartKindIr,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dimension: Option<ChartDimensionIr>,
    pub measure: ChartMeasureIr,
    pub limit: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChartKindIr {
    Kpi,
    Bar,
    HorizontalBar,
    Donut,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ChartDimensionIr {
    Field {
        field: crate::FieldId,
    },
    RelationField {
        relation: crate::FieldId,
        field: crate::FieldId,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case")]
pub enum ChartMeasureIr {
    Count,
    Sum { field: crate::FieldId },
    Avg { field: crate::FieldId },
    Min { field: crate::FieldId },
    Max { field: crate::FieldId },
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
