use appstruct_codegen::Artifact;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};

const MANIFEST_NAME: &str = ".appstruct-manifest.json";
const MANIFEST_VERSION: u32 = 1;
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

pub(crate) fn expected_files(artifacts: &[Artifact]) -> io::Result<BTreeMap<PathBuf, Vec<u8>>> {
    let mut files = BTreeMap::new();
    let mut entries = Vec::with_capacity(artifacts.len());
    for artifact in artifacts {
        validate_relative_path(&artifact.relative_path)?;
        let path = portable_path(&artifact.relative_path)?;
        entries.push(ManifestEntry {
            path: path.clone(),
            kind: artifact.kind.as_str().to_owned(),
            sha256: content_hash(&artifact.content),
        });
        if files
            .insert(artifact.relative_path.clone(), artifact.content.clone())
            .is_some()
        {
            return Err(invalid(format!("duplicate artifact path `{path}`")));
        }
    }
    let manifest = OwnershipManifest {
        manifest_version: MANIFEST_VERSION,
        generator_version: env!("CARGO_PKG_VERSION").to_owned(),
        artifacts: entries,
    };
    let mut content = serde_json::to_vec_pretty(&manifest).map_err(io::Error::other)?;
    content.push(b'\n');
    files.insert(PathBuf::from(MANIFEST_NAME), content);
    Ok(files)
}

pub(super) fn validate_existing(
    root: &Path,
    expected: &BTreeMap<PathBuf, Vec<u8>>,
) -> io::Result<()> {
    if !root.exists() {
        return Ok(());
    }
    let owned = load_owned(root)?;
    validate_generated_files(root, owned.as_ref(), Some(expected))
}

pub(crate) fn validate_owned_tree(root: &Path) -> io::Result<()> {
    if !root.is_dir() {
        return Err(invalid(format!(
            "generated transaction tree `{}` is not a directory",
            root.display()
        )));
    }
    let owned = load_owned(root)?.ok_or_else(|| {
        invalid(format!(
            "generated transaction tree `{}` has no ownership manifest",
            root.display()
        ))
    })?;
    validate_generated_files(root, Some(&owned), None)
}

pub(crate) fn copy_owned_tree(source: &Path, destination: &Path) -> io::Result<()> {
    validate_owned_tree(source)?;
    let owned = load_owned(source)?.expect("validated tree has a manifest");
    fs::create_dir(destination)?;
    for relative in owned
        .iter()
        .map(PathBuf::as_path)
        .chain(std::iter::once(Path::new(MANIFEST_NAME)))
    {
        copy_file(source, destination, relative)?;
    }
    for cargo_lock in ["backend/Cargo.lock", "server/Cargo.lock"] {
        let cargo_lock = Path::new(cargo_lock);
        if source.join(cargo_lock).is_file() {
            copy_file(source, destination, cargo_lock)?;
        }
    }
    validate_owned_tree(destination)
}

fn copy_file(source: &Path, destination: &Path, relative: &Path) -> io::Result<()> {
    let target = destination.join(relative);
    fs::create_dir_all(
        target
            .parent()
            .ok_or_else(|| invalid("owned file has no parent"))?,
    )?;
    fs::copy(source.join(relative), target)?;
    Ok(())
}

fn load_owned(root: &Path) -> io::Result<Option<BTreeSet<PathBuf>>> {
    let manifest_path = root.join(MANIFEST_NAME);
    if !manifest_path.is_file() {
        return Ok(None);
    }
    let source = fs::read(&manifest_path)?;
    let manifest: OwnershipManifest = serde_json::from_slice(&source)
        .map_err(|error| invalid(format!("invalid ownership manifest: {error}")))?;
    if manifest.manifest_version != MANIFEST_VERSION {
        return Err(invalid(format!(
            "unsupported ownership manifest version {}",
            manifest.manifest_version
        )));
    }
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
    Ok(Some(owned))
}

fn validate_generated_files(
    root: &Path,
    owned: Option<&BTreeSet<PathBuf>>,
    expected: Option<&BTreeMap<PathBuf, Vec<u8>>>,
) -> io::Result<()> {
    for path in generated_files(root)? {
        let relative = path.strip_prefix(root).expect("walked below root");
        let recognized = owned.map_or_else(
            || expected.is_some_and(|files| files.contains_key(relative)),
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
