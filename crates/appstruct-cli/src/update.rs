use crate::generation::ownership;
use std::ffi::OsStr;
use std::fs;
use std::io;
use std::path::Path;
use std::process::{Command, ExitCode};

const ROOT_IGNORED: &[&str] = &[
    ".git",
    ".appstruct",
    "generated",
    "target",
    "node_modules",
    "dist",
    ".vite",
];

pub(crate) fn run(project: &Path) -> ExitCode {
    if let Err(error) = ensure_git_clean(project) {
        return update_error(
            "git worktree check failed",
            &error,
            crate::report::ExitClass::Validation,
        );
    }
    let lock = match appstruct_compiler::updated_project_lock(project) {
        Ok(lock) => lock,
        Err(diagnostics) => {
            return crate::report::fail_diagnostics(
                crate::report::ErrorCategory::Validation,
                diagnostics,
            );
        }
    };
    let staging = match stage_project(project, &lock) {
        Ok(staging) => staging,
        Err(error) => {
            return update_error(
                "cannot stage project update",
                &error,
                crate::report::ExitClass::Environment,
            );
        }
    };
    let ir = match appstruct_compiler::compile_project(staging.path()) {
        Ok(ir) => ir,
        Err(diagnostics) => {
            return crate::report::fail_diagnostics(
                crate::report::ErrorCategory::Validation,
                diagnostics,
            );
        }
    };
    if crate::generation::run_quiet(staging.path(), false) != ExitCode::SUCCESS {
        return ExitCode::from(1);
    }
    if let Err(error) = atomic_replace(project, &staging.path().join("generated"), lock.as_bytes())
    {
        return update_error(
            "cannot apply project update",
            &error,
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

fn ensure_git_clean(project: &Path) -> io::Result<()> {
    let Ok(inside) = Command::new("git")
        .arg("-C")
        .arg(project)
        .args(["rev-parse", "--is-inside-work-tree"])
        .output()
    else {
        return Ok(());
    };
    if !inside.status.success() || String::from_utf8_lossy(&inside.stdout).trim() != "true" {
        return Ok(());
    }
    let status = Command::new("git")
        .arg("-C")
        .arg(project)
        .args(["status", "--porcelain"])
        .output()?;
    if !status.status.success() {
        return Err(io::Error::other("git status failed"));
    }
    if !status.stdout.is_empty() {
        return Err(io::Error::other(
            "git worktree is dirty; commit or stash changes before running `appstruct update`",
        ));
    }
    Ok(())
}

fn stage_project(project: &Path, lock: &str) -> io::Result<tempfile::TempDir> {
    let state = project.join(".appstruct");
    fs::create_dir_all(&state)?;
    let staging = tempfile::Builder::new()
        .prefix("update-staging-")
        .tempdir_in(state)?;
    copy_sources(project, staging.path(), Path::new(""))?;
    fs::write(staging.path().join("appstruct.lock"), lock)?;
    Ok(staging)
}

fn copy_sources(source: &Path, destination: &Path, relative: &Path) -> io::Result<()> {
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let name = entry.file_name();
        let child_relative = relative.join(&name);
        if ignored(&child_relative, &name) {
            continue;
        }
        let file_type = entry.file_type()?;
        let src = entry.path();
        let dst = destination.join(&child_relative);
        if file_type.is_dir() {
            fs::create_dir_all(&dst)?;
            copy_sources(&src, destination, &child_relative)?;
        } else if file_type.is_file() {
            if let Some(parent) = dst.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(&src, &dst)?;
        }
    }
    Ok(())
}

fn ignored(relative: &Path, name: &OsStr) -> bool {
    let name = name.to_string_lossy();
    if relative.components().count() == 1 {
        return ROOT_IGNORED.contains(&name.as_ref());
    }
    (relative.starts_with("app/backend") && name == "target")
        || (relative.starts_with("app/web")
            && matches!(name.as_ref(), "node_modules" | "dist" | ".vite"))
}

fn atomic_replace(project: &Path, staged_generated: &Path, lock: &[u8]) -> io::Result<()> {
    ownership::validate_owned_tree(staged_generated)?;
    let generated = project.join("generated");
    if generated.exists() {
        ownership::validate_owned_tree(&generated)?;
    }

    let lock_tmp = project.join(".appstruct.lock.tmp");
    fs::write(&lock_tmp, lock)?;
    fs::rename(&lock_tmp, project.join("appstruct.lock"))?;

    let backup = project.join(".generated.appstruct-old");
    if backup.exists() {
        fs::remove_dir_all(&backup)?;
    }
    let had_generated = generated.exists();
    if had_generated {
        fs::rename(&generated, &backup)?;
    }
    if let Err(error) = fs::rename(staged_generated, &generated) {
        if had_generated {
            let _ = fs::rename(&backup, &generated);
        }
        return Err(error);
    }
    if had_generated {
        let _ = fs::remove_dir_all(&backup);
    }
    Ok(())
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
