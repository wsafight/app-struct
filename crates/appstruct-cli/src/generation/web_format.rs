use appstruct_codegen::{Artifact, ArtifactKind};
use sha2::{Digest, Sha256};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

pub(super) fn format(project: &Path, artifacts: &mut [Artifact]) -> io::Result<()> {
    let cache = project.join(".appstruct/cache");
    fs::create_dir_all(&cache)?;
    let formatter = prepare_formatter(&cache, artifacts)?;
    let temporary = tempfile::Builder::new()
        .prefix("web-format-")
        .tempdir_in(cache)?;
    for artifact in artifacts.iter().filter(|artifact| formatted(artifact)) {
        let path = temporary.path().join(&artifact.relative_path);
        fs::create_dir_all(
            path.parent()
                .ok_or_else(|| io::Error::other("web artifact has no parent"))?,
        )?;
        fs::write(path, &artifact.content)?;
    }
    let web = temporary.path().join("web");
    let files = artifacts
        .iter()
        .filter(|artifact| formatted(artifact))
        .map(|artifact| temporary.path().join(&artifact.relative_path))
        .collect::<Vec<_>>();
    command(
        prettier_command(&formatter)
            .current_dir(&web)
            .arg("--write")
            .args(files),
        "format generated web artifacts",
    )?;
    for artifact in artifacts.iter_mut().filter(|artifact| formatted(artifact)) {
        artifact.content = fs::read(temporary.path().join(&artifact.relative_path))?;
    }
    Ok(())
}

fn prepare_formatter(cache: &Path, artifacts: &[Artifact]) -> io::Result<PathBuf> {
    let package = dependency_artifact(artifacts, "web/package.json")?;
    let lock = dependency_artifact(artifacts, "web/pnpm-lock.yaml")?;
    let mut hash = Sha256::new();
    hash.update(&package.content);
    hash.update([0]);
    hash.update(&lock.content);
    let directory = cache
        .join("web-formatter")
        .join(format!("{:x}", hash.finalize()));
    let ready = directory.join(".ready");
    if ready.is_file() && prettier_path(&directory).is_file() {
        return Ok(directory);
    }
    fs::create_dir_all(&directory)?;
    fs::write(directory.join("package.json"), &package.content)?;
    fs::write(directory.join("pnpm-lock.yaml"), &lock.content)?;
    command(
        Command::new("pnpm").current_dir(&directory).args([
            "install",
            "--frozen-lockfile",
            "--ignore-scripts",
        ]),
        "install pinned web formatter",
    )?;
    fs::write(ready, b"ready\n")?;
    Ok(directory)
}

fn dependency_artifact<'artifacts>(
    artifacts: &'artifacts [Artifact],
    path: &str,
) -> io::Result<&'artifacts Artifact> {
    artifacts
        .iter()
        .find(|artifact| artifact.relative_path == Path::new(path))
        .ok_or_else(|| io::Error::other(format!("missing formatter dependency `{path}`")))
}

#[cfg(not(windows))]
fn prettier_command(formatter: &Path) -> Command {
    Command::new(prettier_path(formatter))
}

#[cfg(windows)]
fn prettier_command(formatter: &Path) -> Command {
    Command::new(prettier_path(formatter))
}

#[cfg(not(windows))]
fn prettier_path(formatter: &Path) -> PathBuf {
    formatter.join("node_modules/.bin/prettier")
}

#[cfg(windows)]
fn prettier_path(formatter: &Path) -> PathBuf {
    formatter.join("node_modules/.bin/prettier.cmd")
}

fn formatted(artifact: &Artifact) -> bool {
    artifact.kind == ArtifactKind::TypeScript
        || matches!(
            artifact.relative_path.to_str(),
            Some(
                "web/src/app/App.tsx"
                    | "web/src/app/Layout.tsx"
                    | "web/src/pages/ResourceDetail.tsx"
            )
        )
}

fn command(command: &mut Command, context: &str) -> io::Result<()> {
    let output = command.output()?;
    if output.status.success() {
        Ok(())
    } else {
        let detail = [output.stdout, output.stderr]
            .iter()
            .map(|bytes| String::from_utf8_lossy(bytes))
            .map(|text| text.trim().to_owned())
            .filter(|text| !text.is_empty())
            .collect::<Vec<_>>()
            .join("\n");
        Err(io::Error::other(format!(
            "{context}: process exited with {}: {detail}",
            output.status
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn artifact(path: &str, kind: ArtifactKind) -> Artifact {
        Artifact {
            relative_path: path.into(),
            content: Vec::new(),
            executable: false,
            kind,
        }
    }

    #[test]
    fn formats_only_ir_driven_web_sources() {
        assert!(formatted(&artifact(
            "web/src/generated/client.ts",
            ArtifactKind::TypeScript,
        )));
        assert!(formatted(&artifact(
            "web/src/app/App.tsx",
            ArtifactKind::Web,
        )));
        assert!(formatted(&artifact(
            "web/src/app/Layout.tsx",
            ArtifactKind::Web,
        )));
        assert!(formatted(&artifact(
            "web/src/pages/ResourceDetail.tsx",
            ArtifactKind::Web,
        )));
        assert!(!formatted(&artifact(
            "web/src/pages/ResourceList.tsx",
            ArtifactKind::Web,
        )));
    }
}
