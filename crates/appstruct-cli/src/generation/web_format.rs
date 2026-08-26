use appstruct_codegen::Artifact;
use std::fs;
use std::io;
use std::path::Path;
use std::process::Command;

pub(super) fn format(project: &Path, artifacts: &mut [Artifact]) -> io::Result<()> {
    let cache = project.join(".appstruct/cache");
    fs::create_dir_all(&cache)?;
    let temporary = tempfile::Builder::new()
        .prefix("web-format-")
        .tempdir_in(cache)?;
    for artifact in artifacts.iter().filter(|artifact| is_web(artifact)) {
        let path = temporary.path().join(&artifact.relative_path);
        fs::create_dir_all(
            path.parent()
                .ok_or_else(|| io::Error::other("web artifact has no parent"))?,
        )?;
        fs::write(path, &artifact.content)?;
    }
    let web = temporary.path().join("web");
    command(
        Command::new("pnpm").current_dir(&web).args([
            "install",
            "--frozen-lockfile",
            "--ignore-scripts",
        ]),
        "install pinned web formatter",
    )?;
    command(
        Command::new("pnpm").current_dir(&web).args([
            "exec",
            "prettier",
            "--write",
            "src/**/*.{ts,tsx,css}",
            "*.ts",
        ]),
        "format generated web artifacts",
    )?;
    for artifact in artifacts.iter_mut().filter(|artifact| formatted(artifact)) {
        artifact.content = fs::read(temporary.path().join(&artifact.relative_path))?;
    }
    Ok(())
}

fn is_web(artifact: &Artifact) -> bool {
    artifact.relative_path.starts_with("web")
}

fn formatted(artifact: &Artifact) -> bool {
    is_web(artifact)
        && artifact
            .relative_path
            .extension()
            .is_some_and(|extension| matches!(extension.to_str(), Some("ts" | "tsx" | "css")))
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
