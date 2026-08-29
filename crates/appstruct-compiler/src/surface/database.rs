use super::Located;
use super::value::{ensure_known_keys, expect_mapping, expect_string, required};
use crate::yaml::{MappingEntry, Node};
use appstruct_ir::Diagnostic;
use std::collections::BTreeMap;

pub(super) struct DecodedDatabase {
    pub provider: Located<String>,
    pub mode: Located<String>,
    pub migration: Located<String>,
}

pub(super) fn decode(
    mapping: &BTreeMap<String, MappingEntry>,
    root: &Node,
) -> Result<DecodedDatabase, Diagnostic> {
    let node = required(mapping, "database", &root.span)?;
    let database = expect_mapping(&node.value, "`database`")?;
    ensure_known_keys(database, &["provider", "dev"], "`database`")?;
    let provider_node = required(database, "provider", &node.value.span)?;
    let provider = expect_string(&provider_node.value, "`database.provider`")?;
    let (mode, migration) = if let Some(dev_node) = database.get("dev") {
        let dev = expect_mapping(&dev_node.value, "`database.dev`")?;
        ensure_known_keys(dev, &["mode", "migration"], "`database.dev`")?;
        let mode = dev.get("mode").map_or_else(
            || {
                Ok(Located {
                    value: "managed".to_owned(),
                    span: dev_node.value.span.clone(),
                })
            },
            |entry| expect_string(&entry.value, "`database.dev.mode`"),
        )?;
        let migration = dev.get("migration").map_or_else(
            || {
                Ok(Located {
                    value: if mode.value == "external" {
                        "unmanaged"
                    } else {
                        "prompt"
                    }
                    .to_owned(),
                    span: dev_node.value.span.clone(),
                })
            },
            |entry| expect_string(&entry.value, "`database.dev.migration`"),
        )?;
        (mode, migration)
    } else {
        (
            Located {
                value: "managed".to_owned(),
                span: node.value.span.clone(),
            },
            Located {
                value: "prompt".to_owned(),
                span: node.value.span.clone(),
            },
        )
    };
    Ok(DecodedDatabase {
        provider,
        mode,
        migration,
    })
}
