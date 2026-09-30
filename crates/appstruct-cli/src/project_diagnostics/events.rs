use serde::Serialize;
use std::path::Path;
use std::process::Command;

#[derive(Clone, Debug, Serialize)]
pub(crate) struct ProjectEvent {
    pub revision: String,
    pub author: String,
    pub timestamp: String,
    pub summary: String,
}

pub(crate) fn project_events(project: &Path, limit: usize) -> Vec<ProjectEvent> {
    let mut events = Vec::new();
    let status = Command::new("git")
        .current_dir(project)
        .args([
            "status",
            "--short",
            "--",
            "appstruct.yaml",
            "appstruct.lock",
            "appstruct.modules.lock",
            "spec",
            "migrations",
            "modules",
        ])
        .output();
    if status
        .as_ref()
        .is_ok_and(|output| output.status.success() && !output.stdout.is_empty())
    {
        events.push(ProjectEvent {
            revision: "working-tree".to_owned(),
            author: std::env::var("USER").unwrap_or_else(|_| "local-user".to_owned()),
            timestamp: "uncommitted".to_owned(),
            summary: "Uncommitted project configuration changes".to_owned(),
        });
    }
    let output = Command::new("git")
        .current_dir(project)
        .args([
            "log",
            &format!("-n{limit}"),
            "--date=iso-strict",
            "--pretty=format:%h%x1f%an%x1f%aI%x1f%s",
            "--",
            "appstruct.yaml",
            "appstruct.lock",
            "appstruct.modules.lock",
            "spec",
            "migrations",
            "modules",
        ])
        .output();
    if let Ok(output) = output
        && output.status.success()
    {
        events.extend(
            String::from_utf8_lossy(&output.stdout)
                .lines()
                .filter_map(|line| {
                    let mut parts = line.split('\u{1f}');
                    Some(ProjectEvent {
                        revision: parts.next()?.to_owned(),
                        author: parts.next()?.to_owned(),
                        timestamp: parts.next()?.to_owned(),
                        summary: parts.next()?.to_owned(),
                    })
                }),
        );
    }
    events.truncate(limit);
    events
}
