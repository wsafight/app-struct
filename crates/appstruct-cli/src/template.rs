use clap::Subcommand;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{fs, io, path::Path, process::ExitCode};

#[derive(Debug, Subcommand)]
pub(crate) enum TemplateCommand {
    /// Compare user-owned delivery files with this CLI's template. Never writes files.
    Diff,
}

#[derive(Deserialize)]
struct Lock {
    template: Selection,
}
#[derive(Deserialize)]
struct Selection {
    name: String,
    version: String,
}
#[derive(Serialize)]
struct FileDiff {
    path: String,
    status: &'static str,
    current_sha256: Option<String>,
    template_sha256: String,
    patch: String,
}

pub(crate) fn run(project: &Path, _command: &TemplateCommand) -> ExitCode {
    match compare(project) {
        Ok((selection, files)) => {
            if crate::report::is_json() {
                crate::report::success(&serde_json::json!({
                    "command": "template", "action": "diff", "template": selection.name,
                    "recorded_version": selection.version, "target_version": env!("CARGO_PKG_VERSION"),
                    "files": files,
                }));
            } else {
                println!(
                    "Template {}: recorded {}, CLI {}",
                    selection.name,
                    selection.version,
                    env!("CARGO_PKG_VERSION")
                );
                println!(
                    "Read-only comparison; review patches before applying them to user-owned files."
                );
                for file in files {
                    println!("{}: {}", file.path, file.status);
                    print!("{}", file.patch);
                }
            }
            ExitCode::SUCCESS
        }
        Err(error) => crate::report::fail(
            "AS6020",
            crate::report::ErrorCategory::Project,
            format!("cannot compare template delivery files: {error}"),
            crate::report::ExitClass::Validation,
        ),
    }
}

fn compare(project: &Path) -> io::Result<(Selection, Vec<FileDiff>)> {
    let lock: Lock = toml::from_str(&fs::read_to_string(project.join("appstruct.lock"))?)
        .map_err(io::Error::other)?;
    let files = crate::project_new::delivery_files(&lock.template.name)
        .ok_or_else(|| io::Error::other("project lock must select minimal, dashboard or saas"))?;
    let mut diffs = Vec::new();
    for (path, expected) in files {
        // Resolve each component explicitly so diagnostics never read outside the project via symlinks.
        let mut resolved = project.to_path_buf();
        let mut missing = false;
        for component in Path::new(path).components() {
            resolved.push(component);
            match fs::symlink_metadata(&resolved) {
                Ok(meta) if meta.file_type().is_symlink() => {
                    return Err(io::Error::other(format!("{path} contains a symlink")));
                }
                Ok(_) => (),
                Err(error) if error.kind() == io::ErrorKind::NotFound => {
                    missing = true;
                    break;
                }
                Err(error) => return Err(error),
            }
        }
        let current = if missing {
            None
        } else {
            Some(fs::read_to_string(project.join(path))?)
        };
        let status = match current.as_deref() {
            None => "missing",
            Some(value) if value == expected => "unchanged",
            Some(_) => "modified",
        };
        let header = format!("a/{path}");
        let target = format!("b/{path}");
        let patch = similar::TextDiff::from_lines(current.as_deref().unwrap_or(""), expected)
            .unified_diff()
            .header(if missing { "/dev/null" } else { &header }, &target)
            .to_string();
        diffs.push(FileDiff {
            path: path.to_owned(),
            status,
            current_sha256: current
                .as_ref()
                .map(|value| hex::encode(Sha256::digest(value.as_bytes()))),
            template_sha256: hex::encode(Sha256::digest(expected.as_bytes())),
            patch,
        });
    }
    Ok((lock.template, diffs))
}
