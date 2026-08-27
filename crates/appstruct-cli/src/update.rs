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
            return crate::report::fail(
                "AS6008",
                crate::report::ErrorCategory::Transaction,
                format!("cannot start project update: {error}"),
                crate::report::ExitClass::Environment,
            );
        }
    };
    let candidate = match CandidateWorkspace::prepare(project) {
        Ok(candidate) => candidate,
        Err(error) => {
            return update_error(
                "cannot stage project files",
                &error,
                crate::report::ExitClass::Environment,
            );
        }
    };
    let lock = match appstruct_compiler::updated_project_lock(project) {
        Ok(lock) => lock,
        Err(diagnostics) => {
            return crate::report::fail_diagnostics(
                crate::report::ErrorCategory::Validation,
                diagnostics,
            );
        }
    };
    if let Err(error) = fs::write(candidate.path().join("appstruct.lock"), &lock) {
        return update_error(
            "cannot stage project lock",
            &error,
            crate::report::ExitClass::Environment,
        );
    }
    let ir = match appstruct_compiler::compile_project(candidate.path()) {
        Ok(ir) => ir,
        Err(diagnostics) => {
            return crate::report::fail_diagnostics(
                crate::report::ErrorCategory::Validation,
                diagnostics,
            );
        }
    };
    if crate::generation::run_quiet(candidate.path(), false) != ExitCode::SUCCESS {
        return ExitCode::from(1);
    }
    if let Err(error) = crate::build::verify_update(candidate.path()) {
        let exit = if error.kind() == io::ErrorKind::NotFound {
            crate::report::ExitClass::Environment
        } else {
            crate::report::ExitClass::Validation
        };
        return update_error("staged build or test failed", &error, exit);
    }
    if let Err(error) = candidate.ensure_source_unchanged(project) {
        return update_error(
            "cannot commit project update",
            &error,
            crate::report::ExitClass::Validation,
        );
    }
    if let Err(error) = transaction.commit(&candidate.path().join("generated"), lock.as_bytes()) {
        return crate::report::fail(
            "AS6008",
            crate::report::ErrorCategory::Transaction,
            format!("cannot commit project update: {error}; preserve update recovery paths"),
            crate::report::ExitClass::Environment,
        );
    }
    if crate::report::is_json() {
        crate::report::success(&serde_json::json!({
            "command": "update",
            "appstruct_version": env!("CARGO_PKG_VERSION"),
            "preset": ir.preset.as_ref().map(|preset| serde_json::json!({
                "name": preset.name,
                "version": preset.version,
            })),
        }));
    } else {
        match ir.preset {
            Some(preset) => println!(
                "Updated project to AppStruct {} with {}@{}",
                env!("CARGO_PKG_VERSION"),
                preset.name,
                preset.version
            ),
            None => println!("Updated project to AppStruct {}", env!("CARGO_PKG_VERSION")),
        }
    }
    ExitCode::SUCCESS
}

fn update_error(context: &str, error: &io::Error, exit: crate::report::ExitClass) -> ExitCode {
    crate::report::fail(
        "AS6008",
        crate::report::ErrorCategory::Transaction,
        format!("{context}: {error}; no project files changed"),
        exit,
    )
}

#[cfg(test)]
mod tests;
