use crate::project_status::{ProjectCommand, ProjectStatus};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

mod events;
use events::{ProjectEvent, project_events};

#[derive(Serialize)]
struct DiagnosticBundle {
    generated_at_unix_seconds: u64,
    appstruct_version: &'static str,
    status: ProjectStatus,
    configuration: Vec<ConfigFile>,
    environment: Vec<EnvironmentEntry>,
    events: Vec<ProjectEvent>,
    request_ids: Vec<String>,
    recent_logs: Vec<String>,
}

#[derive(Serialize)]
struct ConfigFile {
    path: String,
    bytes: u64,
    sha256: String,
}

#[derive(Serialize)]
struct EnvironmentEntry {
    name: String,
    configured: bool,
}

pub(crate) fn run(project: &Path, command: &ProjectCommand, json: bool) -> ExitCode {
    match command {
        ProjectCommand::Status => crate::project_status::run(project, json),
        ProjectCommand::Events { limit } => render_events(project, usize::from(*limit), json),
        ProjectCommand::Diagnose { output } => diagnose(project, output, json),
    }
}

fn render_events(project: &Path, limit: usize, json: bool) -> ExitCode {
    let events = project_events(project, limit);
    if json {
        crate::report::success(&serde_json::json!({
            "command": "project events",
            "events": events,
        }));
    } else if events.is_empty() {
        println!("No project configuration events found");
    } else {
        println!("Project events:");
        for event in events {
            println!(
                "- {} {} {}: {}",
                event.timestamp, event.author, event.revision, event.summary
            );
        }
    }
    ExitCode::SUCCESS
}

fn diagnose(project: &Path, output: &Path, json: bool) -> ExitCode {
    let output = match diagnostics_path(project, output) {
        Ok(path) => path,
        Err(error) => {
            return crate::report::fail(
                "AS6401",
                crate::report::ErrorCategory::Project,
                error,
                crate::report::ExitClass::Usage,
            );
        }
    };
    let status = crate::project_status::collect(project);
    let recent_logs = recent_logs(project);
    let bundle = DiagnosticBundle {
        generated_at_unix_seconds: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
        appstruct_version: env!("CARGO_PKG_VERSION"),
        configuration: configuration_files(project),
        environment: environment_summary(project, &status),
        events: project_events(project, 50),
        request_ids: request_ids(&recent_logs),
        recent_logs,
        status,
    };
    let content = match serde_json::to_vec_pretty(&bundle) {
        Ok(mut content) => {
            content.push(b'\n');
            content
        }
        Err(error) => {
            return crate::report::fail(
                "AS6402",
                crate::report::ErrorCategory::Project,
                format!("cannot serialize diagnostics: {error}"),
                crate::report::ExitClass::Environment,
            );
        }
    };
    if let Some(parent) = output.parent()
        && let Err(error) = fs::create_dir_all(parent)
    {
        return diagnostics_write_error(&output, &error);
    }
    if let Err(error) = crate::transaction::write_new(&output, &content) {
        return diagnostics_write_error(&output, &error);
    }
    let relative = output.strip_prefix(project).unwrap_or(&output);
    if json {
        crate::report::success(&serde_json::json!({
            "command": "project diagnose",
            "output": relative,
            "bytes": content.len(),
        }));
    } else {
        println!("Created redacted diagnostics bundle {}", relative.display());
    }
    ExitCode::SUCCESS
}

fn diagnostics_path(project: &Path, output: &Path) -> Result<PathBuf, String> {
    if output.is_absolute()
        || output
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err("diagnostics output must be a project-relative path without traversal".into());
    }
    if output.extension().and_then(|value| value.to_str()) != Some("json") {
        return Err("diagnostics output must use a .json extension".into());
    }
    let mut parent = project.to_path_buf();
    for component in output.parent().into_iter().flat_map(Path::components) {
        let Component::Normal(component) = component else {
            unreachable!("diagnostics components were validated")
        };
        parent.push(component);
        match fs::symlink_metadata(&parent) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(format!(
                    "diagnostics output parent `{}` is a symlink",
                    parent.display()
                ));
            }
            Ok(metadata) if !metadata.is_dir() => {
                return Err(format!(
                    "diagnostics output parent `{}` is not a directory",
                    parent.display()
                ));
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.to_string()),
        }
    }
    let path = project.join(output);
    if path.exists() {
        return Err(format!(
            "diagnostics output `{}` already exists",
            output.display()
        ));
    }
    Ok(path)
}

fn configuration_files(project: &Path) -> Vec<ConfigFile> {
    let mut paths = Vec::new();
    for relative in ["appstruct.yaml", "appstruct.lock", "appstruct.modules.lock"] {
        let path = project.join(relative);
        if path.is_file() {
            paths.push(path);
        }
    }
    for relative in ["spec", "migrations", "modules"] {
        collect_files(&project.join(relative), &mut paths, 200);
    }
    paths.sort();
    paths.dedup();
    paths
        .into_iter()
        .filter_map(|path| {
            let content = fs::read(&path).ok()?;
            Some(ConfigFile {
                path: path
                    .strip_prefix(project)
                    .ok()?
                    .to_string_lossy()
                    .into_owned(),
                bytes: u64::try_from(content.len()).unwrap_or(u64::MAX),
                sha256: format!("sha256:{}", hex::encode(Sha256::digest(&content))),
            })
        })
        .collect()
}

fn collect_files(directory: &Path, output: &mut Vec<PathBuf>, limit: usize) {
    if output.len() >= limit {
        return;
    }
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    let mut entries = entries.filter_map(Result::ok).collect::<Vec<_>>();
    entries.sort_by_key(std::fs::DirEntry::file_name);
    for entry in entries {
        if output.len() >= limit {
            break;
        }
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if kind.is_symlink() {
            continue;
        }
        if kind.is_dir() {
            collect_files(&entry.path(), output, limit);
        } else if kind.is_file() {
            output.push(entry.path());
        }
    }
}

fn environment_summary(project: &Path, status: &ProjectStatus) -> Vec<EnvironmentEntry> {
    let environment = crate::environment::ProjectEnvironment::load(project).unwrap_or_default();
    let mut names = BTreeSet::from([
        "DATABASE_URL".to_owned(),
        "APPSTRUCT_ENV".to_owned(),
        "APPSTRUCT_API_PORT".to_owned(),
        "APPSTRUCT_WEB_PORT".to_owned(),
    ]);
    for provider in &status.capabilities.oauth_providers {
        let prefix = format!("APPSTRUCT_{}", provider.to_ascii_uppercase());
        for suffix in ["CLIENT_ID", "CLIENT_SECRET", "REDIRECT_URI"] {
            names.insert(format!("{prefix}_{suffix}"));
        }
        if provider == "oidc" {
            for suffix in ["AUTHORIZATION_URL", "TOKEN_URL", "USERINFO_URL"] {
                names.insert(format!("{prefix}_{suffix}"));
            }
        }
    }
    if status.capabilities.billing {
        names.insert("APPSTRUCT_STRIPE_SECRET_KEY".to_owned());
        names.insert("APPSTRUCT_STRIPE_WEBHOOK_SECRET".to_owned());
    }
    names
        .into_iter()
        .map(|name| EnvironmentEntry {
            configured: environment.get(&name).is_some(),
            name,
        })
        .collect()
}

fn recent_logs(project: &Path) -> Vec<String> {
    let Ok(source) = fs::read_to_string(project.join(".appstruct/logs/dev.log")) else {
        return Vec::new();
    };
    let lines = source.lines().collect::<Vec<_>>();
    lines
        .into_iter()
        .rev()
        .take(200)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .map(redact_log_line)
        .collect()
}

fn redact_log_line(line: &str) -> String {
    let lower = line.to_ascii_lowercase();
    if [
        "password",
        "secret",
        "authorization",
        "cookie",
        "database_url",
        "access_token",
        "refresh_token",
        "token=",
    ]
    .iter()
    .any(|needle| lower.contains(needle))
    {
        "[redacted sensitive log line]".to_owned()
    } else {
        line.chars().take(2_000).collect()
    }
}

fn request_ids(logs: &[String]) -> Vec<String> {
    let mut ids = BTreeSet::new();
    for line in logs {
        let lower = line.to_ascii_lowercase();
        let Some(offset) = lower.find("request_id") else {
            continue;
        };
        let value = line[offset + "request_id".len()..]
            .trim_start_matches(|character: char| {
                character.is_ascii_whitespace() || matches!(character, '=' | ':' | '"')
            })
            .chars()
            .take_while(|character| character.is_ascii_alphanumeric() || *character == '-')
            .take(128)
            .collect::<String>();
        if !value.is_empty() {
            ids.insert(value);
        }
    }
    ids.into_iter().take(20).collect()
}

fn diagnostics_write_error(path: &Path, error: &std::io::Error) -> ExitCode {
    crate::report::fail(
        "AS6403",
        crate::report::ErrorCategory::Transaction,
        format!("cannot write diagnostics `{}`: {error}", path.display()),
        crate::report::ExitClass::Environment,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn log_redaction_hides_credentials_and_extracts_request_ids() {
        assert_eq!(
            redact_log_line("DATABASE_URL=postgres://user:password@host/db"),
            "[redacted sensitive log line]"
        );
        let logs = vec!["request_id=018f-test request completed".to_owned()];
        assert_eq!(request_ids(&logs), vec!["018f-test"]);
    }

    #[test]
    fn diagnostics_paths_are_relative_new_json_files() {
        let project = tempfile::tempdir().unwrap();
        assert!(diagnostics_path(project.path(), Path::new("report.json")).is_ok());
        assert!(diagnostics_path(project.path(), Path::new("../report.json")).is_err());
        assert!(diagnostics_path(project.path(), Path::new("report.txt")).is_err());
        fs::write(project.path().join("report.json"), "{}").unwrap();
        assert!(diagnostics_path(project.path(), Path::new("report.json")).is_err());
    }
}
