use super::model::SurfaceChart;
use super::value::{
    ensure_known_keys, expect_mapping, expect_string, optional_string, optional_u64, required,
};
use crate::yaml::Node;
use appstruct_ir::Diagnostic;

pub(super) fn decode(node: &Node) -> Result<Vec<SurfaceChart>, Diagnostic> {
    expect_mapping(node, "charts")?
        .iter()
        .map(|(name, entry)| {
            let mapping = expect_mapping(&entry.value, "chart")?;
            ensure_known_keys(
                mapping,
                &["label", "type", "dimension", "measure", "limit"],
                "chart",
            )?;
            let kind = expect_string(
                &required(mapping, "type", &entry.value.span)?.value,
                "chart type",
            )?;
            let measure = expect_string(
                &required(mapping, "measure", &entry.value.span)?.value,
                "chart measure",
            )?;
            let limit = optional_u64(mapping, "limit")?.map_or(20, |value| value.value);
            Ok(SurfaceChart {
                name: name.clone(),
                label: optional_string(mapping, "label", "chart label")?,
                kind,
                dimension: optional_string(mapping, "dimension", "chart dimension")?,
                measure,
                limit: u32::try_from(limit).unwrap_or(u32::MAX),
                span: entry.value.span.clone(),
            })
        })
        .collect()
}
