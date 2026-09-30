use appstruct_migrate::DriftStatus;
use clap::Subcommand;
use serde::Serialize;
use std::fs;
use std::path::Path;
use std::process::ExitCode;

mod runtime;
use runtime::{RuntimeStatus, runtime_status};

#[derive(Debug, Subcommand)]
pub(crate) enum ProjectCommand {
    /// Show Spec, generation, database, capability, and next-step status.
    Status,
    /// Show recent Git history for project configuration and migrations.
    Events {
        #[arg(long, default_value_t = 20, value_parser = clap::value_parser!(u16).range(1..=100))]
        limit: u16,
    },
    /// Write a redacted JSON diagnostics bundle.
    Diagnose {
        #[arg(long, default_value = "appstruct-diagnostics.json")]
        output: std::path::PathBuf,
    },
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct ProjectStatus {
    pub healthy: bool,
    pub spec: FileStatus,
    pub lock: FileStatus,
    pub generated: GeneratedStatus,
    pub versions: VersionStatus,
    pub compile: CompileStatus,
    pub build: BuildStatus,
    pub database: DatabaseStatus,
    pub runtime: RuntimeStatus,
    pub capabilities: CapabilityStatus,
    pub next_steps: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct FileStatus {
    pub present: bool,
    pub bytes: u64,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct GeneratedStatus {
    pub present: bool,
    pub manifest: bool,
    pub artifact_count: usize,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct CompileStatus {
    pub valid: bool,
    pub entity_count: usize,
    pub diagnostics: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct VersionStatus {
    pub cli: &'static str,
    pub ir: Option<u32>,
    pub preset: Option<String>,
    pub modules: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct BuildStatus {
    pub backend_debug: bool,
    pub backend_release: bool,
    pub web_dist: bool,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct DatabaseStatus {
    pub configured: bool,
    pub reachable: bool,
    pub applied: Option<usize>,
    pub pending: Option<usize>,
    pub drift: Option<String>,
    pub detail: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct CapabilityStatus {
    pub auth: bool,
    pub oauth_providers: Vec<String>,
    pub tenant: bool,
    pub billing: bool,
    pub billing_provider: Option<String>,
}

pub(crate) fn run(project: &Path, json: bool) -> ExitCode {
    let status = collect(project);
    if json {
        crate::report::success(&status);
    } else {
        render_text(&status);
    }
    if status.healthy {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(3)
    }
}

pub(crate) fn collect(project: &Path) -> ProjectStatus {
    let spec = file_status(&project.join("appstruct.yaml"));
    let lock = file_status(&project.join("appstruct.lock"));
    let generated = generated_status(project);
    let (versions, compile, capabilities) = compile_status(project);
    let build = build_status(project);
    let database = database_status(project);
    let runtime = runtime_status(project);
    let next_steps = next_steps(&compile, &generated, &database, &runtime);
    let healthy = spec.present
        && compile.valid
        && generated.present
        && generated.manifest
        && database.configured
        && database.reachable
        && database.drift.as_deref() != Some("detected");
    ProjectStatus {
        healthy,
        spec,
        lock,
        generated,
        versions,
        compile,
        build,
        database,
        runtime,
        capabilities,
        next_steps,
    }
}

fn generated_status(project: &Path) -> GeneratedStatus {
    let root = project.join("generated");
    GeneratedStatus {
        present: root.is_dir(),
        manifest: root.join(".appstruct-manifest.json").is_file(),
        artifact_count: fs::read_dir(&root)
            .ok()
            .map_or(0, |entries| entries.filter_map(Result::ok).count()),
    }
}

fn compile_status(project: &Path) -> (VersionStatus, CompileStatus, CapabilityStatus) {
    let mut versions = VersionStatus {
        cli: env!("CARGO_PKG_VERSION"),
        ir: None,
        preset: None,
        modules: Vec::new(),
    };
    let (compile, capabilities) = match appstruct_compiler::compile_project(project) {
        Ok(ir) => {
            versions.ir = Some(ir.ir_version);
            versions.preset = ir
                .preset
                .as_ref()
                .map(|preset| format!("{}@{}", preset.name, preset.version));
            versions.modules = ir
                .modules
                .iter()
                .map(|module| format!("{}@{}", module.name, module.version))
                .collect();
            (
                CompileStatus {
                    valid: true,
                    entity_count: ir.entities.len(),
                    diagnostics: Vec::new(),
                },
                CapabilityStatus {
                    auth: ir.auth.enabled,
                    oauth_providers: ir.auth.oauth_providers,
                    tenant: ir.tenant.enabled,
                    billing: ir.billing.enabled,
                    billing_provider: ir.billing.provider,
                },
            )
        }
        Err(diagnostics) => (
            CompileStatus {
                valid: false,
                entity_count: 0,
                diagnostics: diagnostics
                    .into_iter()
                    .map(|diagnostic| format!("{}: {}", diagnostic.code, diagnostic.message))
                    .collect(),
            },
            CapabilityStatus {
                auth: false,
                oauth_providers: Vec::new(),
                tenant: false,
                billing: false,
                billing_provider: None,
            },
        ),
    };

    (versions, compile, capabilities)
}

fn next_steps(
    compile: &CompileStatus,
    generated: &GeneratedStatus,
    database: &DatabaseStatus,
    runtime: &RuntimeStatus,
) -> Vec<String> {
    let mut next_steps = Vec::new();
    if !compile.valid {
        next_steps.push("run appstruct check and fix the reported diagnostics".to_owned());
    } else if !generated.manifest {
        next_steps.push("run appstruct generate".to_owned());
    }
    if !database.configured {
        next_steps.push("set DATABASE_URL or choose managed PostgreSQL".to_owned());
    } else if database.pending.unwrap_or(0) > 0 || database.drift.as_deref() == Some("detected") {
        next_steps.push("run appstruct migrate status, then review/apply migrations".to_owned());
    }
    if !runtime.api_ready || !runtime.web_ready {
        next_steps.push("run appstruct dev to start the API and Web application".to_owned());
    }
    if next_steps.is_empty() {
        next_steps.push("project is ready for local development".to_owned());
    }
    next_steps
}

fn build_status(project: &Path) -> BuildStatus {
    let binary = crate::build::backend_binary_name(project).ok();
    let target = project.join(".appstruct/cache/backend-target");
    BuildStatus {
        backend_debug: binary.is_some_and(|name| target.join("debug").join(name).is_file()),
        backend_release: binary.is_some_and(|name| target.join("release").join(name).is_file()),
        web_dist: project.join("generated/web/dist/index.html").is_file(),
    }
}

fn file_status(path: &Path) -> FileStatus {
    let bytes = fs::metadata(path).map_or(0, |metadata| metadata.len());
    FileStatus {
        present: path.is_file(),
        bytes,
    }
}

fn database_status(project: &Path) -> DatabaseStatus {
    let environment = crate::environment::ProjectEnvironment::load(project).ok();
    let url = environment
        .as_ref()
        .and_then(|environment| environment.get("DATABASE_URL"))
        .or_else(|| {
            appstruct_compiler::compile_project(project)
                .ok()
                .filter(|ir| ir.database.dev_mode == appstruct_ir::DatabaseDevMode::Managed)
                .map(|_| crate::development::MANAGED_DATABASE_URL.to_owned())
        });
    let Some(url) = url else {
        return DatabaseStatus {
            configured: false,
            reachable: false,
            applied: None,
            pending: None,
            drift: None,
            detail: Some("DATABASE_URL is not configured".to_owned()),
        };
    };
    match appstruct_migrate::status_project(project, &url) {
        Ok(status) => DatabaseStatus {
            configured: true,
            reachable: true,
            applied: Some(status.applied),
            pending: Some(status.pending),
            drift: Some(
                match status.drift {
                    DriftStatus::Clean => "clean",
                    DriftStatus::Deferred => "deferred",
                    DriftStatus::Detected(_) => "detected",
                }
                .to_owned(),
            ),
            detail: None,
        },
        Err(error) => DatabaseStatus {
            configured: true,
            reachable: false,
            applied: None,
            pending: None,
            drift: None,
            detail: Some(error.to_string()),
        },
    }
}

fn render_text(status: &ProjectStatus) {
    println!(
        "Project status: {}",
        if status.healthy {
            "ready"
        } else {
            "needs attention"
        }
    );
    println!(
        "- Spec: {} ({} bytes)",
        present(status.spec.present),
        status.spec.bytes
    );
    println!("- Lock: {}", present(status.lock.present));
    println!(
        "- Versions: CLI {}, IR {}, preset {} ({} modules)",
        status.versions.cli,
        status
            .versions
            .ir
            .map_or_else(|| "unknown".to_owned(), |version| version.to_string()),
        status.versions.preset.as_deref().unwrap_or("none"),
        status.versions.modules.len()
    );
    println!(
        "- Generated: {} ({} artifacts, manifest {})",
        present(status.generated.present),
        status.generated.artifact_count,
        present(status.generated.manifest)
    );
    println!(
        "- Compile: {} ({} entities)",
        if status.compile.valid {
            "valid"
        } else {
            "invalid"
        },
        status.compile.entity_count
    );
    println!(
        "- Build: debug {}, release {}, Web {}",
        present(status.build.backend_debug),
        present(status.build.backend_release),
        present(status.build.web_dist)
    );
    println!(
        "- Database: {}",
        if status.database.reachable {
            "reachable"
        } else if status.database.configured {
            "unreachable"
        } else {
            "not configured"
        }
    );
    println!(
        "- Runtime: API {}, Web {}",
        if status.runtime.api_ready {
            "ready"
        } else {
            "stopped"
        },
        if status.runtime.web_ready {
            "ready"
        } else {
            "stopped"
        }
    );
    if let Some(metrics) = &status.runtime.metrics {
        println!(
            "- Metrics: {} requests, {} server errors, average latency {}",
            metrics.requests,
            metrics.server_errors,
            metrics
                .average_latency_ms
                .map_or_else(|| "n/a".to_owned(), |latency| format!("{latency:.1} ms"))
        );
    }
    println!(
        "- Capabilities: auth={}, tenant={}, billing={}",
        status.capabilities.auth, status.capabilities.tenant, status.capabilities.billing
    );
    println!("Next:");
    for step in &status.next_steps {
        println!("  - {step}");
    }
}

fn present(value: bool) -> &'static str {
    if value { "ok" } else { "missing" }
}
