use appstruct_codegen::Artifact;
use std::fs;
use std::io;
use std::path::Path;
use std::process::ExitCode;
use std::time::Instant;

mod cache;
pub(crate) mod ownership;
mod timings;
pub(crate) mod transaction;
mod web_format;

use timings::{GenerationTimings, render_success};
use transaction::GenerationTransaction;

pub(crate) fn run(project: &Path, check: bool) -> ExitCode {
    run_with_output(project, check, true, false)
}

pub(crate) fn run_with_timings(project: &Path, check: bool, timings: bool) -> ExitCode {
    run_with_output(project, check, true, timings)
}

pub(crate) fn run_quiet(project: &Path, check: bool) -> ExitCode {
    run_with_output(project, check, false, false)
}

fn run_with_output(
    project: &Path,
    check: bool,
    emit_success: bool,
    show_timings: bool,
) -> ExitCode {
    let total_started = Instant::now();
    let mut timings = GenerationTimings::default();
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
    let cache_started = Instant::now();
    let cache_hit = cache::load_hit(project, &root);
    timings.cache_lookup = cache_started.elapsed();
    match cache_hit {
        Ok(Some(hit)) => {
            timings.total = total_started.elapsed();
            if emit_success {
                render_success(
                    check,
                    &hit.app_name,
                    hit.artifact_count,
                    0,
                    true,
                    show_timings.then_some(&timings),
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
    let (app_name, artifacts) = match plan_artifacts(project, &mut timings) {
        Ok(planned) => planned,
        Err(status) => return status,
    };
    if check {
        let output_started = Instant::now();
        let status = check_artifacts(&root, &artifacts, &app_name, false);
        timings.output = Some(output_started.elapsed());
        timings.total = total_started.elapsed();
        if status == ExitCode::SUCCESS && emit_success {
            render_success(
                true,
                &app_name,
                artifacts.len(),
                0,
                false,
                show_timings.then_some(&timings),
            );
        }
        return status;
    }
    let output_started = Instant::now();
    match write_artifacts(&transaction, &root, &artifacts) {
        Ok(changed) => {
            if let Err(error) = cache::record(project, &root, &app_name, artifacts.len())
                && emit_success
            {
                crate::report::warning(
                    "AS5007",
                    crate::report::ErrorCategory::Generation,
                    &format!("cannot update generation cache: {error}"),
                );
            }
            timings.output = Some(output_started.elapsed());
            timings.total = total_started.elapsed();
            if emit_success {
                render_success(
                    false,
                    &app_name,
                    artifacts.len(),
                    changed,
                    false,
                    show_timings.then_some(&timings),
                );
            }
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

fn plan_artifacts(
    project: &Path,
    timings: &mut GenerationTimings,
) -> Result<(String, Vec<Artifact>), ExitCode> {
    let compiler_started = Instant::now();
    let ir = appstruct_compiler::compile_project(project).map_err(|diagnostics| {
        crate::report::fail_diagnostics(crate::report::ErrorCategory::Validation, diagnostics)
    })?;
    timings.compiler = Some(compiler_started.elapsed());
    let rustfmt_cache = project.join(".appstruct/cache/rustfmt-v1");
    let options = appstruct_codegen::PlanOptions::default().with_rustfmt_cache(&rustfmt_cache);
    let plan = appstruct_codegen::plan_with_options(&ir, options).map_err(|error| {
        crate::report::fail(
            "AS5001",
            crate::report::ErrorCategory::Generation,
            error.to_string(),
            crate::report::ExitClass::Validation,
        )
    })?;
    let mut artifacts = plan.artifacts;
    timings.codegen = Some(plan.timings);
    timings.web_format = Some(
        web_format::format(project, &mut artifacts).map_err(|error| {
            crate::report::fail(
                "AS5006",
                crate::report::ErrorCategory::Tooling,
                format!("cannot format generated web artifacts: {error}"),
                crate::report::ExitClass::Environment,
            )
        })?,
    );
    Ok((ir.app.name, artifacts))
}

fn check_artifacts(
    root: &Path,
    artifacts: &[Artifact],
    app_name: &str,
    emit_success: bool,
) -> ExitCode {
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
        if emit_success {
            render_success(true, app_name, artifacts.len(), 0, false, None);
        }
        return ExitCode::SUCCESS;
    }
    let paths = stale
        .iter()
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>();
    crate::report::fail(
        "AS5004",
        crate::report::ErrorCategory::Generation,
        format!("stale generated artifacts: {}", paths.join(", ")),
        crate::report::ExitClass::Validation,
    )
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

#[cfg(test)]
mod run_tests {
    use super::*;
    use std::fs;

    #[test]
    fn run_reports_invalid_projects() {
        assert_ne!(
            run(Path::new("/missing-appstruct-project"), false),
            ExitCode::SUCCESS
        );
        assert_ne!(
            run_quiet(Path::new("/missing-appstruct-project"), true),
            ExitCode::SUCCESS
        );
    }

    #[test]
    fn render_success_covers_check_write_and_cache_text() {
        crate::report::set_output_format(crate::report::OutputFormat::Text);
        render_success(true, "demo", 3, 0, true, None);
        render_success(false, "demo", 3, 1, false, None);
        crate::report::set_output_format(crate::report::OutputFormat::Json);
        render_success(true, "demo", 3, 0, false, None);
        crate::report::set_output_format(crate::report::OutputFormat::Text);
    }

    #[test]
    fn check_artifacts_reports_stale_files() {
        let temporary = tempfile::tempdir().unwrap();
        let artifacts = [Artifact {
            relative_path: std::path::PathBuf::from("backend/lib.rs"),
            content: b"fn main() {}".to_vec(),
            executable: false,
            kind: appstruct_codegen::ArtifactKind::RustSource,
        }];
        assert_ne!(
            check_artifacts(temporary.path(), &artifacts, "demo", true),
            ExitCode::SUCCESS
        );
    }

    #[test]
    fn write_artifacts_is_a_noop_when_files_already_match() {
        let temporary = tempfile::tempdir().unwrap();
        let artifacts = [Artifact {
            relative_path: std::path::PathBuf::from("lib.rs"),
            content: b"fn main() {}\n".to_vec(),
            executable: false,
            kind: appstruct_codegen::ArtifactKind::RustSource,
        }];
        let expected = ownership::expected_files(&artifacts).unwrap();
        for (path, content) in &expected {
            let destination = temporary.path().join("generated").join(path);
            fs::create_dir_all(destination.parent().unwrap()).unwrap();
            fs::write(destination, content).unwrap();
        }
        let transaction = GenerationTransaction::acquire(temporary.path()).unwrap();
        assert_eq!(
            write_artifacts(
                &transaction,
                &temporary.path().join("generated"),
                &artifacts
            )
            .unwrap(),
            0
        );
    }
}
