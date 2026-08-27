use crate::{Artifact, ArtifactKind, CodegenError};
use appstruct_ir::AppIr;
use appstruct_migrate::{extract, initial_migration, to_json};

pub(crate) fn plan(ir: &AppIr) -> Result<Vec<Artifact>, CodegenError> {
    let schema = extract(ir).map_err(|error| CodegenError::new(error.to_string()))?;
    Ok(vec![
        Artifact::text(
            "database/schema.json",
            to_json(&schema)?,
            ArtifactKind::DatabaseSchema,
        ),
        Artifact::text(
            "database/0001_initial.sql",
            initial_migration(&schema),
            ArtifactKind::Migration,
        ),
    ])
}
