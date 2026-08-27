use crate::{Artifact, ArtifactKind, CodegenError};
use appstruct_ir::{AppIr, ModuleOrigin};
use appstruct_module_sdk::{module_namespace, validate_relative_path};
use std::path::PathBuf;

pub(crate) fn plan(ir: &AppIr) -> Result<Vec<Artifact>, CodegenError> {
    let mut artifacts = Vec::new();
    for module in &ir.modules {
        if module.origin == ModuleOrigin::Official && !module.artifacts.is_empty() {
            return Err(CodegenError::new(format!(
                "official module `{}` cannot carry local artifacts",
                module.name
            )));
        }
        let namespace = module_namespace(&module.name).map_err(|reason| {
            CodegenError::new(format!("module name `{}` {reason}", module.name))
        })?;
        for artifact in &module.artifacts {
            validate_relative_path(&artifact.path).map_err(|reason| {
                CodegenError::new(format!(
                    "module `{}` artifact path `{}` {reason}",
                    module.name, artifact.path
                ))
            })?;
            artifacts.push(Artifact::text(
                PathBuf::from("modules")
                    .join(&namespace)
                    .join(&artifact.path),
                artifact.content.clone(),
                ArtifactKind::Module,
            ));
        }
    }
    Ok(artifacts)
}
