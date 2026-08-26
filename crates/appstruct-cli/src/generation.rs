use appstruct_codegen::Artifact;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};
use std::process::ExitCode;

const MANIFEST_NAME: &str = ".appstruct-manifest.json";
const TRANSIENT_DIRECTORIES: &[&str] = &["target", "node_modules", "dist", ".vite"];
const TRANSIENT_FILES: &[&str] = &["Cargo.lock"];

#[derive(Debug, Deserialize, Serialize)]
struct OwnershipManifest {
    manifest_version: u32,
    generator_version: String,
    artifacts: Vec<ManifestEntry>,
}

#[derive(Debug, Deserialize, Serialize)]
struct ManifestEntry {
    path: String,
    kind: String,
    sha256: String,
}

pub(crate) fn run(project: &Path, check: bool) -> ExitCode {
    let ir = match appstruct_compiler::compile_project(project) {
        Ok(ir) => ir,
        Err(diagnostics) => {
            for diagnostic in &diagnostics {
                super::render_text_diagnostic(diagnostic);
            }
            return ExitCode::from(1);
        }
    };
    let artifacts = match appstruct_codegen::plan(&ir) {
        Ok(artifacts) => artifacts,
        Err(error) => {
            eprintln!("error[AS5001]: {error}");
            return ExitCode::from(1);
        }
    };
    let root = project.join("generated");
    if check {
        return check_artifacts(&root, &artifacts);
    }
    match write_artifacts(&root, &artifacts) {
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
    let expected = match expected_files(artifacts) {
        Ok(expected) => expected,
        Err(error) => {
            eprintln!("error[AS5002]: cannot plan ownership manifest: {error}");
            return ExitCode::from(1);
        }
    };
    if let Err(error) = validate_existing(root, &expected) {
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

fn write_artifacts(root: &Path, artifacts: &[Artifact]) -> io::Result<usize> {
    let expected = expected_files(artifacts)?;
    validate_existing(root, &expected)?;
    let changed = artifacts
        .iter()
        .filter(|artifact| {
            !fs::read(root.join(&artifact.relative_path))
                .is_ok_and(|content| content == artifact.content)
        })
        .count();
    let parent = root
        .parent()
        .ok_or_else(|| invalid("generated root has no parent"))?;
    let staging = parent.join(".generated.appstruct-staging");
    let backup = parent.join(".generated.appstruct-backup");
    if staging.exists() || backup.exists() {
        return Err(invalid("unfinished generated directory transaction exists"));
    }
    if let Err(error) = write_staging(&staging, &expected) {
        let _ = fs::remove_dir_all(&staging);
        return Err(error);
    }
    if root.exists() {
        fs::rename(root, &backup)?;
    }
    if let Err(error) = fs::rename(&staging, root) {
        if backup.exists() {
            let _ = fs::rename(&backup, root);
        }
        return Err(error);
    }
    if backup.exists() {
        fs::remove_dir_all(backup)?;
    }
    Ok(changed)
}

fn expected_files(artifacts: &[Artifact]) -> io::Result<BTreeMap<PathBuf, Vec<u8>>> {
    let mut files = BTreeMap::new();
    let mut entries = Vec::with_capacity(artifacts.len());
    for artifact in artifacts {
        validate_relative_path(&artifact.relative_path)?;
        let path = portable_path(&artifact.relative_path)?;
        entries.push(ManifestEntry {
            path,
            kind: artifact.kind.as_str().to_owned(),
            sha256: content_hash(&artifact.content),
        });
        files.insert(artifact.relative_path.clone(), artifact.content.clone());
    }
    let manifest = OwnershipManifest {
        manifest_version: 1,
        generator_version: env!("CARGO_PKG_VERSION").to_owned(),
        artifacts: entries,
    };
    let mut content = serde_json::to_vec_pretty(&manifest).map_err(io::Error::other)?;
    content.push(b'\n');
    files.insert(PathBuf::from(MANIFEST_NAME), content);
    Ok(files)
}

fn validate_existing(root: &Path, expected: &BTreeMap<PathBuf, Vec<u8>>) -> io::Result<()> {
    if !root.exists() {
        return Ok(());
    }
    let manifest_path = root.join(MANIFEST_NAME);
    let owned = if manifest_path.is_file() {
        let source = fs::read(&manifest_path)?;
        let manifest: OwnershipManifest = serde_json::from_slice(&source)
            .map_err(|error| invalid(format!("invalid ownership manifest: {error}")))?;
        let mut owned = BTreeSet::new();
        for entry in manifest.artifacts {
            let path = PathBuf::from(&entry.path);
            validate_relative_path(&path)?;
            if !owned.insert(path.clone()) {
                return Err(invalid(format!(
                    "ownership manifest contains duplicate `{}`",
                    entry.path
                )));
            }
            let content = fs::read(root.join(&path)).map_err(|error| {
                invalid(format!(
                    "owned artifact `{}` cannot be read: {error}",
                    entry.path
                ))
            })?;
            if content_hash(&content) != entry.sha256 {
                return Err(invalid(format!(
                    "owned artifact `{}` was modified outside AppStruct",
                    entry.path
                )));
            }
        }
        Some(owned)
    } else {
        None
    };
    for path in generated_files(root)? {
        let relative = path.strip_prefix(root).expect("walked below root");
        let recognized = owned.as_ref().map_or_else(
            || expected.contains_key(relative),
            |entries| entries.contains(relative),
        );
        if relative != Path::new(MANIFEST_NAME) && !recognized {
            return Err(invalid(format!(
                "unknown file `{}` exists in generated ownership boundary",
                relative.display()
            )));
        }
    }
    Ok(())
}

fn generated_files(root: &Path) -> io::Result<Vec<PathBuf>> {
    let mut pending = vec![root.to_path_buf()];
    let mut files = Vec::new();
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(directory)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                if !TRANSIENT_DIRECTORIES.contains(&entry.file_name().to_string_lossy().as_ref()) {
                    pending.push(path);
                }
            } else if path.is_file()
                && !TRANSIENT_FILES.contains(&entry.file_name().to_string_lossy().as_ref())
            {
                files.push(path);
            }
        }
    }
    Ok(files)
}

fn write_staging(staging: &Path, files: &BTreeMap<PathBuf, Vec<u8>>) -> io::Result<()> {
    fs::create_dir(staging)?;
    for (relative, content) in files {
        let path = staging.join(relative);
        let parent = path
            .parent()
            .ok_or_else(|| invalid("artifact has no parent"))?;
        fs::create_dir_all(parent)?;
        fs::write(path, content)?;
    }
    Ok(())
}

fn validate_relative_path(path: &Path) -> io::Result<()> {
    if path.as_os_str().is_empty()
        || path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(invalid(format!(
            "unsafe artifact path `{}`",
            path.display()
        )));
    }
    Ok(())
}

fn portable_path(path: &Path) -> io::Result<String> {
    path.components()
        .map(|component| match component {
            Component::Normal(value) => value
                .to_str()
                .map(str::to_owned)
                .ok_or_else(|| invalid("artifact path is not UTF-8")),
            _ => Err(invalid("artifact path is not relative")),
        })
        .collect::<Result<Vec<_>, _>>()
        .map(|parts| parts.join("/"))
}

fn content_hash(content: &[u8]) -> String {
    format!("{:x}", Sha256::digest(content))
}

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}
