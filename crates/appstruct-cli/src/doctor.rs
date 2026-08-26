use crate::environment::ProjectEnvironment;
use appstruct_ir::DatabaseDevMode;
use serde::Serialize;
use std::path::Path;
use std::process::{Command, ExitCode};

const REQUIRED_PNPM: &str = "9.12.3";

#[derive(Debug, Serialize)]
struct DoctorReport {
    healthy: bool,
    checks: Vec<DoctorCheck>,
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
            for diagnostic in &diagnostics {
                super::render_text_diagnostic(diagnostic);
            }
            return ExitCode::from(1);
        }
    };
    let environment = match ProjectEnvironment::load(project) {
        Ok(environment) => environment,
        Err(error) => {
            eprintln!("error[AS6003]: {error}");
            return ExitCode::from(3);
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
    let report = DoctorReport { healthy, checks };
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

fn render_text(report: &DoctorReport) {
    println!("AppStruct doctor:");
    for check in &report.checks {
        let marker = if check.ok { "ok" } else { "failed" };
        println!("- {}: {marker} ({})", check.name, check.detail);
        if let Some(help) = &check.help {
            println!("  help: {help}");
        }
    }
}
