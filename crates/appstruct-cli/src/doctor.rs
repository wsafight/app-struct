use crate::environment::ProjectEnvironment;
use appstruct_ir::DatabaseDevMode;
use serde::Serialize;
use std::path::Path;
use std::process::{Command, ExitCode};

const REQUIRED_PNPM: &str = "11.25.0";

#[derive(Debug, Serialize)]
struct DoctorReport {
    healthy: bool,
    checks: Vec<DoctorCheck>,
    next_step: &'static str,
}

#[derive(Debug, Serialize)]
struct DoctorCheck {
    name: String,
    ok: bool,
    detail: String,
    help: Option<String>,
}

pub(crate) fn run(project: &Path, json: bool) -> ExitCode {
    let ir = match appstruct_compiler::compile_project(project) {
        Ok(ir) => ir,
        Err(diagnostics) => {
            return crate::report::fail_diagnostics(
                crate::report::ErrorCategory::Validation,
                diagnostics,
            );
        }
    };
    let environment = match ProjectEnvironment::load(project) {
        Ok(environment) => environment,
        Err(error) => {
            return crate::report::fail(
                "AS6003",
                crate::report::ErrorCategory::Configuration,
                error.to_string(),
                crate::report::ExitClass::Environment,
            );
        }
    };
    let mut checks = vec![
        tool("rustc", &["--version"], Some("1.98.")),
        tool("cargo", &["--version"], Some("1.98.")),
        tool("rustfmt", &["--version"], None),
        tool("cargo", &["clippy", "--version"], None),
        tool("pnpm", &["--version"], Some(REQUIRED_PNPM)),
    ];
    checks.extend(match ir.database.dev_mode {
        DatabaseDevMode::Managed => managed_checks(project),
        DatabaseDevMode::External => external_checks(project, &environment),
    });
    let healthy = checks.iter().all(|check| check.ok);
    let next_step = next_step(&checks);
    let report = DoctorReport {
        healthy,
        checks,
        next_step,
    };
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&report).expect("doctor report is serializable")
        );
    } else {
        render_text(&report);
    }
    if healthy {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(3)
    }
}

fn managed_checks(project: &Path) -> Vec<DoctorCheck> {
    vec![
        file_check(project.join("compose.yaml"), "compose.yaml"),
        tool(
            "docker",
            &["version", "--format", "{{.Server.Version}}"],
            None,
        ),
        tool("docker", &["compose", "version", "--short"], None),
    ]
}

fn external_checks(project: &Path, environment: &ProjectEnvironment) -> Vec<DoctorCheck> {
    let Some(database_url) = environment.get("DATABASE_URL") else {
        return vec![DoctorCheck {
            name: "PostgreSQL".to_owned(),
            ok: false,
            detail: "DATABASE_URL is not configured".to_owned(),
            help: Some("set DATABASE_URL or add it to the project .env file".to_owned()),
        }];
    };
    let (ok, detail) = match appstruct_migrate::status_project(project, &database_url) {
        Ok(status) => (
            true,
            format!(
                "reachable; {} applied, {} pending",
                status.applied, status.pending
            ),
        ),
        Err(error) => (
            false,
            format!("connection or migration state failed: {error}"),
        ),
    };
    vec![DoctorCheck {
        name: "PostgreSQL".to_owned(),
        ok,
        detail,
        help: (!ok).then(|| "verify DATABASE_URL and run appstruct migrate status".to_owned()),
    }]
}

fn tool(program: &str, arguments: &[&str], required: Option<&str>) -> DoctorCheck {
    let name = if program == "cargo" && arguments.first() == Some(&"clippy") {
        "clippy"
    } else {
        program
    };
    match Command::new(program).args(arguments).output() {
        Ok(output) if output.status.success() => {
            let version = String::from_utf8_lossy(&output.stdout).trim().to_owned();
            let ok = required.is_none_or(|value| version.contains(value));
            DoctorCheck {
                name: name.to_owned(),
                ok,
                detail: if ok {
                    version
                } else {
                    format!("found `{version}`, required `{}`", required.unwrap())
                },
                help: (!ok).then(|| format!("install the required {name} version")),
            }
        }
        Ok(output) => DoctorCheck {
            name: name.to_owned(),
            ok: false,
            detail: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
            help: Some(format!("install or start {name}")),
        },
        Err(error) => DoctorCheck {
            name: name.to_owned(),
            ok: false,
            detail: error.to_string(),
            help: Some(format!("install {name} and ensure it is on PATH")),
        },
    }
}

fn file_check(path: impl AsRef<Path>, name: &str) -> DoctorCheck {
    let exists = path.as_ref().is_file();
    DoctorCheck {
        name: name.to_owned(),
        ok: exists,
        detail: if exists {
            "present".to_owned()
        } else {
            "missing".to_owned()
        },
        help: (!exists).then(|| "restore the file from the selected template".to_owned()),
    }
}

fn next_step(checks: &[DoctorCheck]) -> &'static str {
    match checks.iter().find(|check| !check.ok) {
        None => "Run appstruct dev to start the application.",
        Some(check) => match check.name.as_str() {
            "rustc" | "cargo" | "rustfmt" | "clippy" => {
                "Run rustup toolchain install 1.98.0 --component clippy,rustfmt, then appstruct doctor."
            }
            "pnpm" => "Install pnpm 11.25.0, then run appstruct doctor.",
            "compose.yaml" => {
                "Restore compose.yaml from the selected template, then run appstruct doctor."
            }
            "docker" => {
                "Start Docker and verify docker compose version, then run appstruct doctor."
            }
            "PostgreSQL" if check.detail.contains("DATABASE_URL is not configured") => {
                "Set DATABASE_URL in .env, then run appstruct migrate dev --accept."
            }
            "PostgreSQL" => "Check DATABASE_URL and PostgreSQL, then run appstruct migrate status.",
            _ => "Resolve the failed check, then run appstruct doctor.",
        },
    }
}

fn render_text(report: &DoctorReport) {
    println!("AppStruct doctor:");
    for check in &report.checks {
        let marker = if check.ok { "ok" } else { "failed" };
        println!("- {}: {marker} ({})", check.name, check.detail);
        if let Some(help) = &check.help {
            println!("  help: {help}");
        }
    }
    println!("Next: {}", report.next_step);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn file_check_reports_missing_and_present_files() {
        let temporary = tempfile::tempdir().unwrap();
        let missing = file_check(temporary.path().join("compose.yaml"), "compose.yaml");
        assert!(!missing.ok);
        assert_eq!(missing.detail, "missing");
        fs::write(temporary.path().join("compose.yaml"), "services: {}\n").unwrap();
        let present = file_check(temporary.path().join("compose.yaml"), "compose.yaml");
        assert!(present.ok);
        assert_eq!(present.detail, "present");
    }

    #[test]
    fn tool_check_covers_success_and_missing_binaries() {
        let rustc = tool("rustc", &["--version"], Some("1.98."));
        assert_eq!(rustc.name, "rustc");
        let clippy = tool("cargo", &["clippy", "--version"], None);
        assert_eq!(clippy.name, "clippy");
        let missing = tool("appstruct-missing-binary-for-tests", &["--version"], None);
        assert!(!missing.ok);
        assert!(missing.help.is_some());
        let failed = tool("false", &[], None);
        assert!(!failed.ok);
    }

    #[test]
    fn managed_checks_include_compose_and_docker() {
        let project = tempfile::tempdir().unwrap();
        let checks = managed_checks(project.path());
        assert_eq!(checks[0].name, "compose.yaml");
        assert!(!checks[0].ok);
    }

    #[test]
    fn external_checks_require_database_url() {
        let project = tempfile::tempdir().unwrap();
        let environment = ProjectEnvironment::default();
        let checks = external_checks(project.path(), &environment);
        assert_eq!(checks[0].name, "PostgreSQL");
        assert!(!checks[0].ok);
        assert!(checks[0].detail.contains("DATABASE_URL"));
    }

    #[test]
    fn run_reports_invalid_projects_and_managed_fixture_status() {
        assert_ne!(
            run(Path::new("/missing-appstruct-project"), true),
            ExitCode::SUCCESS
        );
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/m0-project");
        let _ = run(&fixture, true);
        let _ = run(&fixture, false);
    }

    #[test]
    fn render_text_prints_help_for_failed_checks() {
        render_text(&DoctorReport {
            healthy: false,
            checks: vec![DoctorCheck {
                name: "pnpm".to_owned(),
                ok: false,
                detail: "missing".to_owned(),
                help: Some("install pnpm".to_owned()),
            }],
            next_step: "Install pnpm 11.25.0, then run appstruct doctor.",
        });
    }

    #[test]
    fn next_step_uses_first_failed_check_or_starts_dev() {
        assert_eq!(
            next_step(&[]),
            "Run appstruct dev to start the application."
        );
        let checks = vec![DoctorCheck {
            name: "PostgreSQL".to_owned(),
            ok: false,
            detail: "DATABASE_URL is not configured".to_owned(),
            help: None,
        }];
        assert!(next_step(&checks).contains("appstruct migrate dev --accept"));
    }
}
