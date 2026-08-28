use super::model::SurfaceSeed;
use super::value::{expect_mapping, expect_scalar_string};
use crate::yaml::Node;
use appstruct_ir::Diagnostic;

pub(super) fn decode_seeds(node: &Node) -> Result<Vec<SurfaceSeed>, Diagnostic> {
    let definitions = expect_mapping(node, "entity `seeds`")?;
    definitions
        .iter()
        .map(|(name, entry)| {
            let values = expect_mapping(&entry.value, "seed definition")?
                .iter()
                .map(|(field, value)| {
                    Ok((
                        super::Located {
                            value: field.clone(),
                            span: value.key_span.clone(),
                        },
                        expect_scalar_string(&value.value, "seed field value")?,
                    ))
                })
                .collect::<Result<Vec<_>, Diagnostic>>()?;
            Ok(SurfaceSeed {
                name: super::Located {
                    value: name.clone(),
                    span: entry.key_span.clone(),
                },
                values,
                span: entry.value.span.clone(),
            })
        })
        .collect()
}
