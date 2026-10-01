use clap::Subcommand;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    fs, io,
    path::{Path, PathBuf},
    process::ExitCode,
};

const MANIFEST_PATH: &str = ".appstruct/release-manifest.json";
const MANIFEST_VERSION: u32 = 1;
const REQUIRED_FILES: &[&str] = &["appstruct.lock", "generated/.appstruct-manifest.json"];
const OPTIONAL_FILES: &[&str] = &[".appstruct/schema.snapshot.json"];
const ARTIFACT_TREES: &[&str] = &["generated/web/dist", "migrations"];

#[derive(Debug, Subcommand)]
pub(crate) enum DeployCommand {
    /// Verify that release artifacts still match the immutable build manifest.
    Verify,
}

#[derive(Debug, Deserialize, Serialize)]
struct ReleaseManifest {
    schema_version: u32,
    appstruct_version: String,
    backend: String,
    files: Vec<ReleaseFile>,
}

#[derive(Debug, Deserialize, Serialize)]
struct ReleaseFile {
    path: String,
    sha256: String,
    size: u64,
}

pub(crate) fn run(project: &Path, command: &DeployCommand) -> ExitCode {
    match command {
        DeployCommand::Verify => match verify_manifest(project) {
            Ok(manifest) => {
                if crate::report::is_json() {
                    crate::report::success(&serde_json::json!({
                        "command": "deploy verify",
                        "manifest": MANIFEST_PATH,
                        "appstruct_version": manifest.appstruct_version,
                        "file_count": manifest.files.len(),
                    }));
                } else {
                    println!(
                        "Release artifacts verified: {} files ({})",
                        manifest.files.len(),
                        MANIFEST_PATH
                    );
                }
                ExitCode::SUCCESS
            }
            Err(error) => crate::report::fail(
                "AS6501",
                crate::report::ErrorCategory::Build,
                format!("release verification failed: {error}"),
                crate::report::ExitClass::Validation,
            ),
        },
    }
}

pub(crate) fn write_manifest(project: &Path, backend: &Path) -> io::Result<PathBuf> {
    let backend_path = portable_path(backend)?;
    let mut paths = vec![
        PathBuf::from("appstruct.lock"),
        PathBuf::from("generated/.appstruct-manifest.json"),
        backend.to_path_buf(),
    ];
    for optional in [".appstruct/schema.snapshot.json", "migrations"] {
        let path = project.join(optional);
        if path.exists() {
            collect_files(project, &path, &mut paths)?;
        }
    }
    collect_files(project, &project.join("generated/web/dist"), &mut paths)?;
    paths.sort();
    paths.dedup();

    let files = paths
        .into_iter()
        .map(|relative| release_file(project, &relative))
        .collect::<io::Result<Vec<_>>>()?;
    let manifest = ReleaseManifest {
        schema_version: MANIFEST_VERSION,
        appstruct_version: env!("CARGO_PKG_VERSION").to_owned(),
        backend: backend_path,
        files,
    };
    let mut content = serde_json::to_vec_pretty(&manifest).map_err(io::Error::other)?;
    content.push(b'\n');
    let path = project.join(MANIFEST_PATH);
    fs::create_dir_all(path.parent().expect("manifest path has a parent"))?;
    fs::write(&path, content)?;
    Ok(path)
}

fn verify_manifest(project: &Path) -> io::Result<ReleaseManifest> {
    let path = project.join(MANIFEST_PATH);
    let manifest: ReleaseManifest = serde_json::from_slice(&fs::read(&path)?).map_err(|error| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("cannot parse {}: {error}", path.display()),
        )
    })?;
    if manifest.schema_version != MANIFEST_VERSION {
        return Err(invalid(format!(
            "unsupported release manifest version {}",
            manifest.schema_version
        )));
    }
    if manifest.appstruct_version != env!("CARGO_PKG_VERSION") {
        return Err(invalid(format!(
            "release was built by AppStruct {}, but verification is running with AppStruct {}",
            manifest.appstruct_version,
            env!("CARGO_PKG_VERSION")
        )));
    }
    if manifest.files.is_empty() {
        return Err(invalid("release manifest contains no files"));
    }
    let mut previous = None;
    for expected in &manifest.files {
        let relative = safe_relative(&expected.path)?;
        if previous.as_ref().is_some_and(|value| value >= &relative) {
            return Err(invalid("release manifest paths are not unique and sorted"));
        }
        previous = Some(relative.clone());
        let actual = release_file(project, &relative)?;
        if actual.sha256 != expected.sha256 || actual.size != expected.size {
            return Err(invalid(format!(
                "artifact `{}` does not match its release manifest",
                expected.path
            )));
        }
    }
    verify_inventory(project, &manifest.backend, &manifest.files)?;
    Ok(manifest)
}

fn verify_inventory(project: &Path, backend: &str, files: &[ReleaseFile]) -> io::Result<()> {
    let expected = files
        .iter()
        .map(|file| file.path.as_str())
        .collect::<BTreeSet<_>>();
    for required in REQUIRED_FILES {
        if !expected.contains(required) {
            return Err(invalid(format!(
                "release manifest is missing required artifact `{required}`"
            )));
        }
    }
    safe_relative(backend)?;
    if !expected.contains(backend) {
        return Err(invalid(format!(
            "release manifest is missing required artifact `{backend}`"
        )));
    }
    if !expected
        .iter()
        .any(|path| path.starts_with("generated/web/dist/"))
    {
        return Err(invalid(
            "release manifest does not contain a production Web artifact",
        ));
    }

    for optional in OPTIONAL_FILES {
        match fs::symlink_metadata(project.join(optional)) {
            Ok(_) if !expected.contains(optional) => {
                return Err(unlisted_artifact(optional));
            }
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
    }
    for tree in ARTIFACT_TREES {
        let root = project.join(tree);
        let mut actual = Vec::new();
        match collect_files(project, &root, &mut actual) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound && *tree == "migrations" => {
                continue;
            }
            Err(error) => return Err(error),
        }
        for path in actual {
            let portable = portable_path(&path)?;
            if !expected.contains(portable.as_str()) {
                return Err(unlisted_artifact(&portable));
            }
        }
    }
    Ok(())
}

fn collect_files(root: &Path, path: &Path, output: &mut Vec<PathBuf>) -> io::Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() {
        return Err(invalid(format!(
            "release artifact `{}` must not be a symlink",
            path.display()
        )));
    }
    if metadata.is_file() {
        output.push(
            path.strip_prefix(root)
                .map_err(|_| invalid("release artifact escaped the project"))?
                .to_path_buf(),
        );
        return Ok(());
    }
    if !metadata.is_dir() {
        return Err(invalid(format!(
            "release artifact `{}` is not a file or directory",
            path.display()
        )));
    }
    let mut entries = fs::read_dir(path)?.collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(fs::DirEntry::file_name);
    for entry in entries {
        collect_files(root, &entry.path(), output)?;
    }
    Ok(())
}

fn release_file(project: &Path, relative: &Path) -> io::Result<ReleaseFile> {
    let portable = portable_path(relative)?;
    let path = project.join(relative);
    let metadata = fs::symlink_metadata(&path)?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err(invalid(format!(
            "release artifact `{portable}` must be a regular file"
        )));
    }
    let content = fs::read(path)?;
    Ok(ReleaseFile {
        path: portable,
        sha256: hex::encode(Sha256::digest(&content)),
        size: content.len() as u64,
    })
}

fn safe_relative(value: &str) -> io::Result<PathBuf> {
    let path = PathBuf::from(value);
    if path.is_absolute()
        || path
            .components()
            .any(|part| !matches!(part, std::path::Component::Normal(_)))
    {
        return Err(invalid(format!("unsafe release artifact path `{value}`")));
    }
    Ok(path)
}

fn portable_path(path: &Path) -> io::Result<String> {
    let value = path
        .to_str()
        .ok_or_else(|| invalid("release artifact path is not UTF-8"))?
        .replace('\\', "/");
    safe_relative(&value)?;
    Ok(value)
}

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

fn unlisted_artifact(path: &str) -> io::Error {
    invalid(format!(
        "artifact `{path}` is not listed in the release manifest"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn release_manifest_detects_changed_and_unsafe_artifacts() {
        let project = tempfile::tempdir().unwrap();
        for (path, content) in [
            ("appstruct.lock", "lock"),
            ("generated/.appstruct-manifest.json", "{}"),
            ("generated/web/dist/index.html", "web"),
            ("build/backend", "binary"),
            ("migrations/0001.sql", "select 1"),
        ] {
            let path = project.path().join(path);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, content).unwrap();
        }
        write_manifest(project.path(), Path::new("build/backend")).unwrap();
        let manifest = verify_manifest(project.path()).unwrap();
        assert_eq!(manifest.files.len(), 5);
        assert_eq!(manifest.files[0].path, "appstruct.lock");

        fs::write(
            project.path().join("generated/web/dist/index.html"),
            "changed",
        )
        .unwrap();
        assert!(
            verify_manifest(project.path())
                .unwrap_err()
                .to_string()
                .contains("does not match")
        );
        assert!(safe_relative("../outside").is_err());
        assert!(safe_relative("/absolute").is_err());
    }

    #[test]
    fn release_manifest_rejects_unlisted_and_missing_required_artifacts() {
        let project = tempfile::tempdir().unwrap();
        for (path, content) in [
            ("appstruct.lock", "project_layout_version = 1\n"),
            ("generated/.appstruct-manifest.json", "{}"),
            ("generated/web/dist/index.html", "web"),
            (
                ".appstruct/cache/backend-target/release/appstruct-generated-backend",
                "binary",
            ),
        ] {
            let path = project.path().join(path);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, content).unwrap();
        }
        write_manifest(
            project.path(),
            Path::new(".appstruct/cache/backend-target/release/appstruct-generated-backend"),
        )
        .unwrap();

        fs::write(
            project.path().join("generated/web/dist/injected.js"),
            "unexpected",
        )
        .unwrap();
        assert!(
            verify_manifest(project.path())
                .unwrap_err()
                .to_string()
                .contains("is not listed")
        );

        fs::remove_file(project.path().join("generated/web/dist/injected.js")).unwrap();
        let manifest_path = project.path().join(MANIFEST_PATH);
        let mut manifest: ReleaseManifest =
            serde_json::from_slice(&fs::read(&manifest_path).unwrap()).unwrap();
        manifest.files.retain(|file| file.path != "appstruct.lock");
        fs::write(manifest_path, serde_json::to_vec(&manifest).unwrap()).unwrap();
        assert!(
            verify_manifest(project.path())
                .unwrap_err()
                .to_string()
                .contains("missing required artifact `appstruct.lock`")
        );
    }
}
