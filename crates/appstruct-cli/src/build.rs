use crate::environment::ProjectEnvironment;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};

pub(crate) fn run(project: &Path) -> ExitCode {
    let backend = match backend_manifest(project) {
        Ok(backend) => backend,
        Err(error) => {
            return crate::report::fail(
                "AS6004",
                crate::report::ErrorCategory::Build,
                format!("cannot resolve project layout: {error}"),
                crate::report::ExitClass::Validation,
            );
        }
    };
    let binary = match backend_binary_name(project) {
        Ok(binary) => binary,
        Err(error) => {
            return crate::report::fail(
                "AS6004",
                crate::report::ErrorCategory::Build,
                format!("cannot resolve project layout: {error}"),
                crate::report::ExitClass::Validation,
            );
        }
    };
    let generation = if crate::report::is_json() {
        super::generation::run_quiet(project, false)
    } else {
        super::generation::run(project, false)
    };
    if generation != ExitCode::SUCCESS {
        return ExitCode::from(1);
    }
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
    let web = project.join("generated/web");
    let target = project.join(".appstruct/cache/backend-target");
    if let Err(error) = build_steps(project, &backend, &web, &target, &environment) {
        let exit = if error.kind() == io::ErrorKind::NotFound {
            crate::report::ExitClass::Environment
        } else {
            crate::report::ExitClass::Validation
        };
        return crate::report::fail(
            "AS6004",
            crate::report::ErrorCategory::Build,
            format!("production build failed: {error}"),
            exit,
        );
    }
    let backend = format!(".appstruct/cache/backend-target/release/{binary}");
    if crate::report::is_json() {
        crate::report::success(&serde_json::json!({
            "command": "build",
            "backend": backend,
            "web": "generated/web/dist",
        }));
    } else {
        println!("Production build completed:");
        println!("- backend: {backend}");
        println!("- web: generated/web/dist");
    }
    ExitCode::SUCCESS
}

pub(crate) fn verify_update(project: &Path) -> io::Result<()> {
    let environment = ProjectEnvironment::load(project)?;
    let backend = backend_manifest(project)?;
    let web = project.join("generated/web");
    let target = project.join(".appstruct/cache/backend-target");
    build_steps(project, &backend, &web, &target, &environment)?;
    run_cargo(
        &environment,
        project,
        &target,
        &["test", "--release", "--locked", "--manifest-path"],
        Some(&backend),
        &["--all-targets"],
    )
}

pub(crate) fn backend_manifest(project: &Path) -> io::Result<PathBuf> {
    match project_layout(project)? {
        appstruct_compiler::ProjectLayout::LegacyGeneratedBackend => {
            Ok(project.join("generated/backend/Cargo.toml"))
        }
        appstruct_compiler::ProjectLayout::CompositionRoot => {
            Ok(project.join("generated/server/Cargo.toml"))
        }
    }
}

pub(crate) fn backend_binary_name(project: &Path) -> io::Result<&'static str> {
    match project_layout(project)? {
        appstruct_compiler::ProjectLayout::LegacyGeneratedBackend => {
            Ok("appstruct-generated-backend")
        }
        appstruct_compiler::ProjectLayout::CompositionRoot => Ok("appstruct-generated-server"),
    }
}

fn project_layout(project: &Path) -> io::Result<appstruct_compiler::ProjectLayout> {
    appstruct_compiler::project_layout(project).map_err(|diagnostic| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("{}: {}", diagnostic.code, diagnostic.message),
        )
    })
}

fn build_steps(
    project: &Path,
    backend: &Path,
    web: &Path,
    target: &Path,
    environment: &ProjectEnvironment,
) -> io::Result<()> {
    if !backend
        .parent()
        .expect("backend manifest has a parent")
        .join("Cargo.lock")
        .is_file()
    {
        run_cargo(
            environment,
            project,
            target,
            &["generate-lockfile", "--manifest-path"],
            Some(backend),
            &[],
        )?;
    }
    run_parallel(
        || backend_steps(project, backend, target, environment),
        || web_steps(web, environment),
    )
}

fn backend_steps(
    project: &Path,
    backend: &Path,
    target: &Path,
    environment: &ProjectEnvironment,
) -> io::Result<()> {
    run_cargo(
        environment,
        project,
        target,
        &["fmt", "--manifest-path"],
        Some(backend),
        &["--", "--check"],
    )?;
    run_cargo(
        environment,
        project,
        target,
        &["clippy", "--release", "--locked", "--manifest-path"],
        Some(backend),
        &["--all-targets", "--", "-D", "warnings"],
    )?;
    run_cargo(
        environment,
        project,
        target,
        &["build", "--release", "--locked", "--manifest-path"],
        Some(backend),
        &[],
    )
}

fn web_steps(web: &Path, environment: &ProjectEnvironment) -> io::Result<()> {
    run_command(
        environment,
        web,
        "pnpm",
        &["install", "--frozen-lockfile"],
        None,
        &[],
    )?;
    run_command(
        environment,
        web,
        "pnpm",
        &["run", "format:check"],
        None,
        &[],
    )?;
    run_command(environment, web, "pnpm", &["run", "build"], None, &[])
}

fn run_parallel<Backend, Web>(backend: Backend, web: Web) -> io::Result<()>
where
    Backend: FnOnce() -> io::Result<()> + Send,
    Web: FnOnce() -> io::Result<()> + Send,
{
    std::thread::scope(|scope| {
        let backend = scope.spawn(backend);
        let web = scope.spawn(web);
        let backend = backend
            .join()
            .map_err(|_| io::Error::other("backend build worker panicked"))?;
        let web = web
            .join()
            .map_err(|_| io::Error::other("web build worker panicked"))?;
        backend?;
        web
    })
}

fn run_cargo(
    environment: &ProjectEnvironment,
    directory: &Path,
    target: &Path,
    leading: &[&str],
    path_argument: Option<&Path>,
    trailing: &[&str],
) -> io::Result<()> {
    let mut command = Command::new("cargo");
    command
        .current_dir(directory)
        .env("CARGO_TARGET_DIR", target)
        .args(leading);
    if let Some(path) = path_argument {
        command.arg(path);
    }
    command.args(trailing);
    environment.apply(&mut command);
    isolate_stdout_for_json(&mut command);
    check_status(command.status()?, "cargo")
}

fn run_command(
    environment: &ProjectEnvironment,
    directory: &Path,
    program: &str,
    leading: &[&str],
    path_argument: Option<&Path>,
    trailing: &[&str],
) -> io::Result<()> {
    let mut command = Command::new(program);
    command.current_dir(directory).args(leading);
    if let Some(path) = path_argument {
        command.arg(path);
    }
    command.args(trailing);
    environment.apply(&mut command);
    isolate_stdout_for_json(&mut command);
    check_status(command.status()?, program)
}

fn isolate_stdout_for_json(command: &mut Command) {
    if crate::report::is_json() {
        command.stdout(Stdio::null());
    }
}

fn check_status(status: std::process::ExitStatus, program: &str) -> io::Result<()> {
    if status.success() {
        Ok(())
    } else {
        Err(io::Error::other(format!(
            "`{program}` exited with {status}"
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::{
        backend_binary_name, backend_manifest, check_status, isolate_stdout_for_json, run,
        run_parallel,
    };
    use std::fs;
    use std::io;
    use std::path::Path;
    use std::process::ExitCode;
    use std::sync::mpsc;
    use std::time::Duration;

    #[test]
    fn backend_and_web_build_steps_run_in_parallel() {
        let (backend_started, from_backend) = mpsc::channel();
        let (web_started, from_web) = mpsc::channel();
        run_parallel(
            move || {
                backend_started.send(()).map_err(io::Error::other)?;
                from_web
                    .recv_timeout(Duration::from_secs(1))
                    .map_err(io::Error::other)
            },
            move || {
                web_started.send(()).map_err(io::Error::other)?;
                from_backend
                    .recv_timeout(Duration::from_secs(1))
                    .map_err(io::Error::other)
            },
        )
        .unwrap();
    }

    #[test]
    fn backend_target_is_selected_by_lock_protocol_not_directory_presence() {
        let project = tempfile::tempdir().unwrap();
        fs::create_dir_all(project.path().join("app/backend")).unwrap();
        fs::write(
            project.path().join("appstruct.lock"),
            "lock_version = 1\nproject_layout_version = 1\nappstruct = \"0.1.0\"\n",
        )
        .unwrap();
        assert!(
            backend_manifest(project.path())
                .unwrap()
                .ends_with(Path::new("generated/backend/Cargo.toml"))
        );
        assert_eq!(
            backend_binary_name(project.path()).unwrap(),
            "appstruct-generated-backend"
        );

        fs::write(
            project.path().join("appstruct.lock"),
            "lock_version = 1\nproject_layout_version = 2\nappstruct = \"0.1.0\"\n",
        )
        .unwrap();
        assert!(
            backend_manifest(project.path())
                .unwrap()
                .ends_with(Path::new("generated/server/Cargo.toml"))
        );
        assert_eq!(
            backend_binary_name(project.path()).unwrap(),
            "appstruct-generated-server"
        );
    }

    #[test]
    fn unversioned_lock_requires_explicit_update() {
        let project = tempfile::tempdir().unwrap();
        fs::write(
            project.path().join("appstruct.lock"),
            "lock_version = 1\nappstruct = \"0.1.0\"\n",
        )
        .unwrap();
        assert!(backend_manifest(project.path()).is_err());
    }

    #[test]
    fn run_fails_when_the_project_layout_cannot_be_resolved() {
        let project = tempfile::tempdir().unwrap();
        assert_ne!(run(project.path()), ExitCode::SUCCESS);
    }

    #[test]
    fn check_status_and_parallel_workers_report_failures() {
        assert!(check_status(std::process::Command::new("true").status().unwrap(), "true").is_ok());
        let error = check_status(
            std::process::Command::new("false").status().unwrap(),
            "cargo",
        )
        .unwrap_err();
        assert!(error.to_string().contains("cargo"));

        let error = run_parallel(|| panic!("backend boom"), || Ok(())).unwrap_err();
        assert!(error.to_string().contains("backend build worker panicked"));
        let error = run_parallel(|| Ok(()), || panic!("web boom")).unwrap_err();
        assert!(error.to_string().contains("web build worker panicked"));
        run_parallel(|| Ok(()), || Ok(())).unwrap();
    }

    #[test]
    fn isolate_stdout_for_json_nulls_the_pipe() {
        crate::report::set_output_format(crate::report::OutputFormat::Json);
        let mut command = std::process::Command::new("true");
        isolate_stdout_for_json(&mut command);
        crate::report::set_output_format(crate::report::OutputFormat::Text);
        isolate_stdout_for_json(&mut command);
    }
}
