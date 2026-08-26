use std::fs;
use std::io;
use std::path::Path;
use std::process::ExitCode;

mod transaction;
mod workspace;

use transaction::UpdateTransaction;
use workspace::CandidateWorkspace;

pub(crate) fn run(project: &Path) -> ExitCode {
    let transaction = match UpdateTransaction::acquire(project) {
        Ok(transaction) => transaction,
        Err(error) => {
            eprintln!("error[AS6008]: cannot start project update: {error}");
            return ExitCode::from(3);
        }
    };
    let candidate = match CandidateWorkspace::prepare(project) {
        Ok(candidate) => candidate,
        Err(error) => return update_error("cannot stage project files", &error, 3),
    };
    let lock = match appstruct_compiler::updated_project_lock(project) {
        Ok(lock) => lock,
        Err(diagnostics) => {
            for diagnostic in &diagnostics {
                crate::render_text_diagnostic(diagnostic);
            }
            return ExitCode::from(1);
        }
    };
    if let Err(error) = fs::write(candidate.path().join("appstruct.lock"), &lock) {
        return update_error("cannot stage project lock", &error, 3);
    }
    let ir = match appstruct_compiler::compile_project(candidate.path()) {
        Ok(ir) => ir,
        Err(diagnostics) => {
            for diagnostic in &diagnostics {
                crate::render_text_diagnostic(diagnostic);
            }
            return ExitCode::from(1);
        }
    };
    if crate::generation::run(candidate.path(), false) != ExitCode::SUCCESS {
        eprintln!("error[AS6008]: staged project generation failed; no project files changed");
        return ExitCode::from(1);
    }
    if let Err(error) = crate::build::verify_update(candidate.path()) {
        let exit = if error.kind() == io::ErrorKind::NotFound {
            3
        } else {
            1
        };
        return update_error("staged build or test failed", &error, exit);
    }
    if let Err(error) = candidate.ensure_source_unchanged(project) {
        return update_error("cannot commit project update", &error, 1);
    }
    if let Err(error) = transaction.commit(&candidate.path().join("generated"), lock.as_bytes()) {
        return update_error("cannot commit project update", &error, 3);
    }
    match ir.preset {
        Some(preset) => println!(
            "Updated project to AppStruct {} with {}@{}",
            env!("CARGO_PKG_VERSION"),
            preset.name,
            preset.version
        ),
        None => println!("Updated project to AppStruct {}", env!("CARGO_PKG_VERSION")),
    }
    ExitCode::SUCCESS
}

fn update_error(context: &str, error: &io::Error, exit: u8) -> ExitCode {
    eprintln!("error[AS6008]: {context}: {error}; no project files changed");
    ExitCode::from(exit)
}

#[cfg(test)]
mod tests;
