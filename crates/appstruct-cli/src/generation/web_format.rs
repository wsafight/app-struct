use appstruct_codegen::{Artifact, ArtifactKind};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

const FORMAT_CACHE_LIMIT: usize = 4_096;

#[derive(Clone, Debug, Default)]
pub(super) struct FormatTimings {
    pub total: Duration,
    pub formatter_setup: Duration,
    pub prettier: Duration,
    pub persistent_hits: usize,
    pub misses: usize,
}

struct Formatter {
    directory: PathBuf,
    identity: String,
}

pub(super) fn format(project: &Path, artifacts: &mut [Artifact]) -> io::Result<FormatTimings> {
    let total_started = Instant::now();
    let cache = project.join(".appstruct/cache");
    fs::create_dir_all(&cache)?;
    let setup_started = Instant::now();
    let formatter = prepare_formatter(&cache, artifacts)?;
    let formatter_setup = setup_started.elapsed();
    let output_cache = cache.join("web-format-v1");
    let mut misses = Vec::new();
    let mut persistent_hits = 0;
    let mut cache_keys = Vec::new();
    for (index, artifact) in artifacts.iter_mut().enumerate() {
        if !formatted(artifact) {
            continue;
        }
        let key = format_cache_key(artifact, &formatter.identity);
        if let Ok(content) = fs::read(format_cache_path(&output_cache, &key)) {
            artifact.content = content;
            persistent_hits += 1;
        } else {
            misses.push(index);
            cache_keys.push((index, key));
        }
    }
    if misses.is_empty() {
        return Ok(FormatTimings {
            total: total_started.elapsed(),
            formatter_setup,
            persistent_hits,
            ..FormatTimings::default()
        });
    }
    let temporary = tempfile::Builder::new()
        .prefix("web-format-")
        .tempdir_in(cache)?;
    for index in &misses {
        let artifact = &artifacts[*index];
        let path = temporary.path().join(&artifact.relative_path);
        fs::create_dir_all(
            path.parent()
                .ok_or_else(|| io::Error::other("web artifact has no parent"))?,
        )?;
        fs::write(path, &artifact.content)?;
    }
    let web = temporary.path().join("web");
    let files = misses
        .iter()
        .map(|index| temporary.path().join(&artifacts[*index].relative_path))
        .collect::<Vec<_>>();
    let prettier_started = Instant::now();
    command(
        prettier_command(&formatter.directory)
            .current_dir(&web)
            .arg("--write")
            .args(files),
        "format generated web artifacts",
    )?;
    let prettier = prettier_started.elapsed();
    for (index, key) in cache_keys {
        let artifact = &mut artifacts[index];
        artifact.content = fs::read(temporary.path().join(&artifact.relative_path))?;
        let _ = write_format_cache(&output_cache, &key, &artifact.content);
    }
    prune_format_cache(&output_cache, FORMAT_CACHE_LIMIT);
    Ok(FormatTimings {
        total: total_started.elapsed(),
        formatter_setup,
        prettier,
        persistent_hits,
        misses: misses.len(),
    })
}

fn prepare_formatter(cache: &Path, artifacts: &[Artifact]) -> io::Result<Formatter> {
    let package = dependency_artifact(artifacts, "web/package.json")?;
    let lock = dependency_artifact(artifacts, "web/pnpm-lock.yaml")?;
    let mut hash = Sha256::new();
    hash.update(&package.content);
    hash.update([0]);
    hash.update(&lock.content);
    let dependency_identity = format!("sha256:{:x}", hash.finalize());
    let directory = cache.join("web-formatter").join(&dependency_identity[7..]);
    let ready = directory.join(".ready");
    if !ready.is_file() || !prettier_path(&directory).is_file() {
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
    }
    let version =
        crate::cache::command_identity(prettier_command(&directory).arg("--version"), "prettier")?;
    Ok(Formatter {
        directory,
        identity: format!("appstruct-web-format-v1\0{dependency_identity}\0{version}"),
    })
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

fn format_cache_key(artifact: &Artifact, formatter_identity: &str) -> [u8; 32] {
    let mut hash = Sha256::new();
    hash.update(formatter_identity.as_bytes());
    hash.update([0]);
    hash.update(artifact.relative_path.as_os_str().as_encoded_bytes());
    hash.update([0]);
    hash.update(&artifact.content);
    hash.finalize().into()
}

fn format_cache_path(directory: &Path, key: &[u8; 32]) -> PathBuf {
    let mut name = String::with_capacity(71);
    for byte in key {
        use std::fmt::Write as _;
        let _ = write!(name, "{byte:02x}");
    }
    name.push_str(".formatted");
    directory.join(name)
}

fn write_format_cache(directory: &Path, key: &[u8; 32], content: &[u8]) -> io::Result<()> {
    fs::create_dir_all(directory)?;
    let destination = format_cache_path(directory, key);
    if destination.is_file() {
        return Ok(());
    }
    let mut temporary = tempfile::NamedTempFile::new_in(directory)?;
    temporary.write_all(content)?;
    temporary.flush()?;
    match temporary.persist_noclobber(destination) {
        Ok(_) => Ok(()),
        Err(error) if error.error.kind() == io::ErrorKind::AlreadyExists => Ok(()),
        Err(error) => Err(error.error),
    }
}

fn prune_format_cache(directory: &Path, limit: usize) {
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    let mut files = entries
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .path()
                .extension()
                .is_some_and(|value| value == "formatted")
        })
        .filter_map(|entry| {
            let modified = entry.metadata().ok()?.modified().ok()?;
            Some((modified, entry.path()))
        })
        .collect::<Vec<_>>();
    if files.len() <= limit {
        return;
    }
    files.sort_by_key(|(modified, _)| *modified);
    let remove = files.len() - limit;
    for (_, path) in files.into_iter().take(remove) {
        let _ = fs::remove_file(path);
    }
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

    #[test]
    fn format_cache_keys_include_content_path_and_tool_identity() {
        let first = Artifact {
            relative_path: "web/src/generated/client.ts".into(),
            content: b"export const value=1".to_vec(),
            executable: false,
            kind: ArtifactKind::TypeScript,
        };
        let mut changed = first.clone();
        changed.content.push(b';');
        let mut moved = first.clone();
        moved.relative_path = "web/src/generated/other.ts".into();

        assert_ne!(
            format_cache_key(&first, "prettier 1"),
            format_cache_key(&changed, "prettier 1")
        );
        assert_ne!(
            format_cache_key(&first, "prettier 1"),
            format_cache_key(&moved, "prettier 1")
        );
        assert_ne!(
            format_cache_key(&first, "prettier 1"),
            format_cache_key(&first, "prettier 2")
        );
    }
}
