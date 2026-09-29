use crate::surface::{SurfaceChart, SurfaceEntity};
use appstruct_ir::{
    ChartDimensionIr, ChartIr, ChartKindIr, ChartMeasureIr, Diagnostic, FieldIr, FieldTypeIr,
};

pub(super) fn build_charts(
    entity: &SurfaceEntity,
    fields: &[FieldIr],
    diagnostics: &mut Vec<Diagnostic>,
) -> Vec<ChartIr> {
    entity
        .charts
        .iter()
        .filter_map(|chart| lower_chart(chart, fields, diagnostics))
        .collect()
}

fn lower_chart(
    chart: &SurfaceChart,
    fields: &[FieldIr],
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<ChartIr> {
    let kind = match chart.kind.value.as_str() {
        "kpi" => ChartKindIr::Kpi,
        "bar" => ChartKindIr::Bar,
        "horizontal_bar" => ChartKindIr::HorizontalBar,
        "donut" => ChartKindIr::Donut,
        _ => {
            diagnostics.push(Diagnostic::error(
                "AS2044",
                "chart type must be `kpi`, `bar`, `horizontal_bar`, or `donut`",
                chart.kind.span.clone(),
            ));
            return None;
        }
    };
    let dimension = chart
        .dimension
        .as_ref()
        .map(|dimension| lower_dimension(dimension.value.as_str(), fields))
        .transpose()
        .unwrap_or_else(|message| {
            diagnostics.push(Diagnostic::error(
                "AS2044",
                message,
                chart
                    .dimension
                    .as_ref()
                    .map_or_else(|| chart.span.clone(), |value| value.span.clone()),
            ));
            None
        });
    let measure = match lower_measure(&chart.measure.value, fields) {
        Ok(measure) => measure,
        Err(message) => {
            diagnostics.push(Diagnostic::error(
                "AS2044",
                message,
                chart.measure.span.clone(),
            ));
            return None;
        }
    };
    Some(ChartIr {
        name: chart.name.clone(),
        label: chart
            .label
            .as_ref()
            .map_or_else(|| humanize(&chart.name), |label| label.value.clone()),
        kind,
        dimension,
        measure,
        limit: chart.limit,
    })
}

fn lower_dimension(value: &str, fields: &[FieldIr]) -> Result<ChartDimensionIr, String> {
    let parts = value.split('.').collect::<Vec<_>>();
    match parts.as_slice() {
        [field_name] => find_field(fields, field_name)
            .map(|field| ChartDimensionIr::Field {
                field: field.id.clone(),
            })
            .ok_or_else(|| format!("chart dimension references unknown field `{field_name}`")),
        [relation_name, target_field] => {
            let relation = find_field(fields, relation_name).ok_or_else(|| {
                format!("chart dimension references unknown relation `{relation_name}`")
            })?;
            let FieldTypeIr::Relation { target } = &relation.ty else {
                return Err(format!(
                    "chart dimension `{value}` must start with a relation field"
                ));
            };
            Ok(ChartDimensionIr::RelationField {
                relation: relation.id.clone(),
                field: appstruct_ir::FieldId(format!("{}.{target_field}", target.0)),
            })
        }
        _ => Err("chart dimensions support at most one relation hop".to_owned()),
    }
}

fn lower_measure(value: &str, fields: &[FieldIr]) -> Result<ChartMeasureIr, String> {
    if value == "count" {
        return Ok(ChartMeasureIr::Count);
    }
    let (operation, field_name) = value.split_once(':').ok_or_else(|| {
        "chart measure must be `count` or an operation such as `sum:amount`".to_owned()
    })?;
    if field_name.contains(':') {
        return Err("chart measure contains too many `:` separators".to_owned());
    }
    let field = find_field(fields, field_name)
        .ok_or_else(|| format!("chart measure references unknown field `{field_name}`"))?;
    match operation {
        "sum" => Ok(ChartMeasureIr::Sum {
            field: field.id.clone(),
        }),
        "avg" => Ok(ChartMeasureIr::Avg {
            field: field.id.clone(),
        }),
        "min" => Ok(ChartMeasureIr::Min {
            field: field.id.clone(),
        }),
        "max" => Ok(ChartMeasureIr::Max {
            field: field.id.clone(),
        }),
        _ => Err(format!("unsupported chart measure operation `{operation}`")),
    }
}

fn find_field<'fields>(fields: &'fields [FieldIr], name: &str) -> Option<&'fields FieldIr> {
    fields
        .iter()
        .find(|field| field.api_name == name || field.rust_name == name)
}

fn humanize(value: &str) -> String {
    let mut output = String::new();
    for (index, word) in value.split('_').filter(|word| !word.is_empty()).enumerate() {
        if index > 0 {
            output.push(' ');
        }
        let mut characters = word.chars();
        if let Some(first) = characters.next() {
            output.extend(first.to_uppercase());
            output.extend(characters);
        }
    }
    output
}
