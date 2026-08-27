use crate::{Artifact, ArtifactKind, CodegenError};
use appstruct_ir::{AppIr, ModuleOrigin};
use appstruct_module_sdk::{module_namespace, validate_relative_path};
use sha2::{Digest, Sha256};
use std::path::PathBuf;

pub(crate) fn plan(ir: &AppIr) -> Result<Vec<Artifact>, CodegenError> {
    let mut artifacts = Vec::new();
    for module in &ir.modules {
        if module.origin == ModuleOrigin::Official
            && (module.manifest_path.is_some()
                || module.content_sha256.is_some()
                || !module.artifacts.is_empty())
        {
            return Err(CodegenError::new(format!(
                "official module `{}` cannot carry local provenance or artifacts",
                module.name
            )));
        }
        if let Some(manifest_path) = &module.manifest_path {
            validate_project_source_path(&module.name, "manifest", manifest_path)?;
        }
        if let Some(content_sha256) = &module.content_sha256 {
            validate_sha256(&module.name, "manifest", content_sha256)?;
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
            if let Some(source) = &artifact.source {
                validate_project_source_path(&module.name, "artifact source", source)?;
            }
            validate_sha256(&module.name, "artifact", &artifact.sha256)?;
            let actual_sha256 = format!("sha256:{:x}", Sha256::digest(artifact.content.as_bytes()));
            if artifact.sha256 != actual_sha256 {
                return Err(CodegenError::new(format!(
                    "module `{}` artifact `{}` content does not match its SHA-256",
                    module.name, artifact.path
                )));
            }
            let actual_byte_len = u64::try_from(artifact.content.len()).map_err(|_| {
                CodegenError::new(format!(
                    "module `{}` artifact `{}` byte count overflowed",
                    module.name, artifact.path
                ))
            })?;
            if artifact.byte_len != actual_byte_len {
                return Err(CodegenError::new(format!(
                    "module `{}` artifact `{}` content does not match its byte length",
                    module.name, artifact.path
                )));
            }
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

fn validate_project_source_path(module: &str, label: &str, path: &str) -> Result<(), CodegenError> {
    validate_relative_path(path).map_err(|reason| {
        CodegenError::new(format!("module `{module}` {label} path `{path}` {reason}"))
    })?;
    if !path.starts_with("modules/") {
        return Err(CodegenError::new(format!(
            "module `{module}` {label} path `{path}` is not below `modules/`"
        )));
    }
    Ok(())
}

fn validate_sha256(module: &str, label: &str, value: &str) -> Result<(), CodegenError> {
    let digest = value.strip_prefix("sha256:").unwrap_or_default();
    if digest.len() != 64 || !digest.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(CodegenError::new(format!(
            "module `{module}` {label} has an invalid SHA-256 digest"
        )));
    }
    Ok(())
}
