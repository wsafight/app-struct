use appstruct_codegen::Artifact;
use std::fs;
use std::io;
use std::path::Path;
use std::process::ExitCode;

mod ownership;
mod transaction;
mod web_format;

use transaction::GenerationTransaction;

pub(crate) fn run(project: &Path, check: bool) -> ExitCode {
    let transaction = match GenerationTransaction::acquire(project) {
        Ok(transaction) => transaction,
        Err(error) => {
            eprintln!("error[AS5005]: cannot start generated directory transaction: {error}");
            return ExitCode::from(3);
        }
    };
    let ir = match appstruct_compiler::compile_project(project) {
        Ok(ir) => ir,
        Err(diagnostics) => {
            for diagnostic in &diagnostics {
                super::render_text_diagnostic(diagnostic);
            }
            return ExitCode::from(1);
        }
    };
    let mut artifacts = match appstruct_codegen::plan(&ir) {
        Ok(artifacts) => artifacts,
        Err(error) => {
            eprintln!("error[AS5001]: {error}");
            return ExitCode::from(1);
        }
    };
    if let Err(error) = web_format::format(project, &mut artifacts) {
        eprintln!("error[AS5006]: cannot format generated web artifacts: {error}");
        return ExitCode::from(3);
    }
    let root = project.join("generated");
    if check {
        return check_artifacts(&root, &artifacts);
    }
    match write_artifacts(&transaction, &root, &artifacts) {
        Ok(changed) => {
            println!(
                "Generated {} artifacts for {} ({changed} changed)",
                artifacts.len(),
                ir.app.name
            );
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("error[AS5002]: failed to write generated artifacts: {error}");
            ExitCode::from(3)
        }
    }
}

fn check_artifacts(root: &Path, artifacts: &[Artifact]) -> ExitCode {
    let expected = match ownership::expected_files(artifacts) {
        Ok(expected) => expected,
        Err(error) => {
            eprintln!("error[AS5002]: cannot plan ownership manifest: {error}");
            return ExitCode::from(1);
        }
    };
    if let Err(error) = ownership::validate_existing(root, &expected) {
        eprintln!("error[AS5004]: generated ownership check failed: {error}");
        return ExitCode::from(1);
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
    transaction.replace(&expected)?;
    Ok(changed)
}

#[cfg(test)]
mod tests;
