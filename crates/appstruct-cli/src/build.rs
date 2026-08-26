use crate::environment::ProjectEnvironment;
use std::io;
use std::path::Path;
use std::process::{Command, ExitCode};

pub(crate) fn run(project: &Path) -> ExitCode {
    if super::generation::run(project, false) != ExitCode::SUCCESS {
        return ExitCode::from(1);
    }
    let environment = match ProjectEnvironment::load(project) {
        Ok(environment) => environment,
        Err(error) => {
            eprintln!("error[AS6003]: {error}");
            return ExitCode::from(3);
        }
    };
    let backend = project.join("generated/backend/Cargo.toml");
    let web = project.join("generated/web");
    let target = project.join(".appstruct/cache/backend-target");
    if let Err(error) = build_steps(project, &backend, &web, &target, &environment) {
        let exit = if error.kind() == io::ErrorKind::NotFound {
            3
        } else {
            1
        };
        eprintln!("error[AS6004]: production build failed: {error}");
        return ExitCode::from(exit);
    }
    println!("Production build completed:");
    println!("- backend: .appstruct/cache/backend-target/release/appstruct-generated-backend");
    println!("- web: generated/web/dist");
    ExitCode::SUCCESS
}

pub(crate) fn verify_update(project: &Path) -> io::Result<()> {
    let environment = ProjectEnvironment::load(project)?;
    let backend = project.join("generated/backend/Cargo.toml");
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
    )?;
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
    check_status(command.status()?, program)
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
