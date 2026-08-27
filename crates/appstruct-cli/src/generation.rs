use appstruct_codegen::Artifact;
use std::fs;
use std::io;
use std::path::Path;
use std::process::ExitCode;

mod cache;
pub(crate) mod ownership;
pub(crate) mod transaction;
mod web_format;

use transaction::GenerationTransaction;

pub(crate) fn run(project: &Path, check: bool) -> ExitCode {
    let transaction = match GenerationTransaction::acquire(project) {
        Ok(transaction) => transaction,
        Err(error) => {
            return crate::report::fail(
                "AS5005",
                crate::report::ErrorCategory::Transaction,
                format!("cannot start generated directory transaction: {error}"),
                crate::report::ExitClass::Environment,
            );
        }
    };
    let root = project.join("generated");
    match cache::load_hit(project, &root) {
        Ok(Some(hit)) => {
            if check {
                println!(
                    "Generated artifacts are current ({} files; cache hit)",
                    hit.artifact_count
                );
            } else {
                println!(
                    "Generated {} artifacts for {} (0 changed; cache hit)",
                    hit.artifact_count, hit.app_name
                );
            }
            return ExitCode::SUCCESS;
        }
        Ok(None) => {}
        Err(error) => {
            return crate::report::fail(
                "AS5004",
                crate::report::ErrorCategory::Generation,
                format!("generated ownership check failed: {error}"),
                crate::report::ExitClass::Validation,
            );
        }
    }
    let ir = match appstruct_compiler::compile_project(project) {
        Ok(ir) => ir,
        Err(diagnostics) => {
            return crate::report::fail_diagnostics(
                crate::report::ErrorCategory::Validation,
                diagnostics,
            );
        }
    };
    let mut artifacts = match appstruct_codegen::plan(&ir) {
        Ok(artifacts) => artifacts,
        Err(error) => {
            return crate::report::fail(
                "AS5001",
                crate::report::ErrorCategory::Generation,
                error.to_string(),
                crate::report::ExitClass::Validation,
            );
        }
    };
    if let Err(error) = web_format::format(project, &mut artifacts) {
        return crate::report::fail(
            "AS5006",
            crate::report::ErrorCategory::Tooling,
            format!("cannot format generated web artifacts: {error}"),
            crate::report::ExitClass::Environment,
        );
    }
    if check {
        return check_artifacts(&root, &artifacts);
    }
    match write_artifacts(&transaction, &root, &artifacts) {
        Ok(changed) => {
            if let Err(error) = cache::record(project, &root, &ir.app.name, artifacts.len()) {
                crate::report::warning(
                    "AS5007",
                    crate::report::ErrorCategory::Generation,
                    &format!("cannot update generation cache: {error}"),
                );
            }
            println!(
                "Generated {} artifacts for {} ({changed} changed)",
                artifacts.len(),
                ir.app.name
            );
            ExitCode::SUCCESS
        }
        Err(error) => crate::report::fail(
            "AS5002",
            crate::report::ErrorCategory::Transaction,
            format!("failed to write generated artifacts: {error}"),
            crate::report::ExitClass::Environment,
        ),
    }
}

fn check_artifacts(root: &Path, artifacts: &[Artifact]) -> ExitCode {
    let expected = match ownership::expected_files(artifacts) {
        Ok(expected) => expected,
        Err(error) => {
            return crate::report::fail(
                "AS5002",
                crate::report::ErrorCategory::Generation,
                format!("cannot plan ownership manifest: {error}"),
                crate::report::ExitClass::Validation,
            );
        }
    };
    if let Err(error) = ownership::validate_existing(root, &expected) {
        return crate::report::fail(
            "AS5004",
            crate::report::ErrorCategory::Generation,
            format!("generated ownership check failed: {error}"),
            crate::report::ExitClass::Validation,
        );
    }
    let stale = expected
        .iter()
        .filter(|(path, content)| {
            !fs::read(root.join(path)).is_ok_and(|actual| actual == **content)
        })
        .map(|(path, _)| root.join(path))
        .collect::<Vec<_>>();
    if stale.is_empty() {
        println!(
            "Generated artifacts are current ({} files)",
            artifacts.len()
        );
        return ExitCode::SUCCESS;
    }
    for path in stale {
        eprintln!("stale generated artifact: {}", path.display());
    }
    ExitCode::from(1)
}

fn write_artifacts(
    transaction: &GenerationTransaction,
    root: &Path,
    artifacts: &[Artifact],
) -> io::Result<usize> {
    let expected = ownership::expected_files(artifacts)?;
    ownership::validate_existing(root, &expected)?;
    let changed = artifacts
        .iter()
        .filter(|artifact| {
            !fs::read(root.join(&artifact.relative_path))
                .is_ok_and(|content| content == artifact.content)
        })
        .count();
    if expected
        .iter()
        .all(|(path, content)| fs::read(root.join(path)).is_ok_and(|actual| actual == *content))
    {
        return Ok(changed);
    }
    transaction.replace(&expected)?;
    Ok(changed)
}

#[cfg(test)]
mod tests;
