use std::collections::hash_map::DefaultHasher;
use std::fs::{self, File, OpenOptions};
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::Mutex;

static CARGO_CHECK_LOCK: Mutex<()> = Mutex::new(());

#[allow(dead_code)]
pub fn cargo_test(manifest: &Path, test: &str) -> Output {
    let _guard = CARGO_CHECK_LOCK
        .lock()
        .expect("generated crate check lock is not poisoned");
    let _target_guard = GeneratedTargetLock::acquire();
    let cache_key = refresh_generated_cache();
    let _ = prepare_generated_package(manifest);
    let packages = local_packages(manifest);
    let output = Command::new("cargo")
        .args(["test", "--quiet", "--manifest-path"])
        .arg(manifest)
        .args(["--test", test])
        .env("CARGO_TARGET_DIR", generated_target())
        .env("CARGO_INCREMENTAL", "0")
        .env("RUSTFLAGS", "-Dwarnings")
        .output()
        .unwrap();
    record_generated_cache(&cache_key);
    clean_local_packages(manifest, &packages);
    output
}

pub fn cargo_check(manifest: &Path, library_only: bool) -> Output {
    cargo_check_with_features(manifest, library_only, &[])
}

pub fn cargo_check_with_features(manifest: &Path, library_only: bool, features: &[&str]) -> Output {
    let _guard = CARGO_CHECK_LOCK
        .lock()
        .expect("generated crate check lock is not poisoned");
    let _target_guard = GeneratedTargetLock::acquire();
    let cache_key = refresh_generated_cache();
    let _ = prepare_generated_package(manifest);
    let packages = local_packages(manifest);
    let mut command = Command::new("cargo");
    command
        .args(["check", "--quiet", "--manifest-path"])
        .arg(manifest)
        .env("CARGO_TARGET_DIR", generated_target())
        .env("CARGO_INCREMENTAL", "0")
        .env("RUSTFLAGS", "-Dwarnings");
    if library_only {
        command.arg("--lib");
    }
    if !features.is_empty() {
        command.args(["--features", &features.join(",")]);
    }
    let output = command.output().unwrap();
    record_generated_cache(&cache_key);
    clean_local_packages(manifest, &packages);
    output
}

pub fn prepare_generated_package(manifest: &Path) -> Option<String> {
    let source = fs::read_to_string(manifest).unwrap();
    if let Some(name) = package_name(&source, "appstruct-generated-test-") {
        return Some(name);
    }
    if !source.contains("name = \"appstruct-generated-backend\"") {
        return None;
    }
    let mut hasher = DefaultHasher::new();
    source.hash(&mut hasher);
    hash_sources(&manifest.parent().unwrap().join("src"), &mut hasher);
    let name = format!("appstruct-generated-test-{:016x}", hasher.finish());
    let source = source
        .replacen(
            "name = \"appstruct-generated-backend\"",
            &format!("name = {name:?}"),
            1,
        )
        .replacen(
            "[dependencies]",
            "[lib]\nname = \"appstruct_generated_backend\"\npath = \"src/lib.rs\"\n\n[dependencies]",
            1,
        );
    fs::write(manifest, source).unwrap();
    Some(name)
}

pub fn assert_rustfmt(manifest: &Path) {
    let output = Command::new("cargo")
        .args(["fmt", "--check", "--manifest-path"])
        .arg(manifest)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "generated Rust is not rustfmt-clean:\n{}",
        String::from_utf8_lossy(&output.stdout)
    );
}

fn generated_target() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/appstruct-generated-tests")
}

struct GeneratedTargetLock {
    file: File,
}

impl GeneratedTargetLock {
    fn acquire() -> Self {
        let target = generated_target();
        let parent = target.parent().unwrap();
        fs::create_dir_all(parent).unwrap();
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(parent.join(".appstruct-generated-tests.lock"))
            .unwrap();
        file.lock().unwrap();
        Self { file }
    }
}

impl Drop for GeneratedTargetLock {
    fn drop(&mut self) {
        let _ = self.file.unlock();
    }
}

fn refresh_generated_cache() -> String {
    let target = generated_target();
    let marker = target.join(".appstruct-cache-key");
    let key = generated_cache_key();
    if fs::read_to_string(&marker).is_ok_and(|stored| stored == key)
        && target.join("CACHEDIR.TAG").is_file()
    {
        return key;
    }
    if target.exists() {
        fs::remove_dir_all(&target).unwrap();
    }
    key
}

fn record_generated_cache(key: &str) {
    let target = generated_target();
    if target.join("CACHEDIR.TAG").is_file() {
        fs::write(target.join(".appstruct-cache-key"), key).unwrap();
    }
}

fn generated_cache_key() -> String {
    let mut hasher = DefaultHasher::new();
    fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../Cargo.lock"))
        .unwrap()
        .hash(&mut hasher);
    Command::new("rustc")
        .args(["--version", "--verbose"])
        .output()
        .unwrap()
        .stdout
        .hash(&mut hasher);
    Command::new("cargo")
        .args(["--version", "--verbose"])
        .output()
        .unwrap()
        .stdout
        .hash(&mut hasher);
    format!("{:016x}\n", hasher.finish())
}

fn local_packages(manifest: &Path) -> Vec<String> {
    let output = Command::new("cargo")
        .args(["metadata", "--format-version", "1", "--manifest-path"])
        .arg(manifest)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "cannot inspect generated crate packages:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let metadata: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let mut packages = metadata["packages"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|package| package["source"].is_null())
        .map(|package| package["name"].as_str().unwrap().to_owned())
        .collect::<Vec<_>>();
    packages.sort();
    packages.dedup();
    packages
}

fn clean_local_packages(manifest: &Path, packages: &[String]) {
    assert!(
        !packages.is_empty(),
        "generated crate metadata did not contain local packages"
    );
    let mut command = Command::new("cargo");
    command
        .args(["clean", "--manifest-path"])
        .arg(manifest)
        .arg("--target-dir")
        .arg(generated_target());
    for package in packages {
        command.arg("--package").arg(package);
    }
    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "cannot reclaim generated crate packages ({}):\npackages: {}\n{}{}",
        output.status,
        packages.join("\n"),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn package_name(source: &str, prefix: &str) -> Option<String> {
    source.lines().find_map(|line| {
        line.strip_prefix("name = \"")
            .and_then(|name| name.strip_suffix('"'))
            .filter(|name| name.starts_with(prefix))
            .map(str::to_owned)
    })
}

fn hash_sources(directory: &Path, hasher: &mut DefaultHasher) {
    let mut entries = fs::read_dir(directory)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect::<Vec<_>>();
    entries.sort();
    for path in entries {
        path.file_name().hash(hasher);
        if path.is_dir() {
            hash_sources(&path, hasher);
        } else {
            fs::read(path).unwrap().hash(hasher);
        }
    }
}

#[allow(dead_code)]
pub fn server_manifest(generated_package: &str) -> String {
    format!(
        r#"[package]
name = "appstruct-extension-server"
version = "0.0.0"
edition = "2024"

[dependencies]
appstruct-generated-backend = {{ package = {generated_package:?}, path = "../generated/backend" }}
async-trait = "0.1.92"
"#
    )
}

#[allow(dead_code)]
pub fn missing_handler_source() -> &'static str {
    r"use appstruct_generated_backend::{ApiError, AppExtensions, RequestContext};
use appstruct_generated_backend::entities::project;
use appstruct_generated_backend::extensions::{ArchiveProjectHandler, ArchiveProjectInput};
use async_trait::async_trait;

struct Handlers;

#[async_trait]
impl ArchiveProjectHandler for Handlers {
    async fn execute(&self, _ctx: &RequestContext, _input: ArchiveProjectInput) -> Result<project::Model, ApiError> {
        Err(ApiError::NotFound)
    }
}

fn main() { let _extensions = AppExtensions::builder().handlers(Handlers).build(); }
"
}

#[allow(dead_code)]
pub fn complete_handler_source() -> &'static str {
    r"use appstruct_generated_backend::{ApiError, AppExtensions, RequestContext};
use appstruct_generated_backend::entities::project;
use appstruct_generated_backend::extensions::{ArchiveProjectHandler, ArchiveProjectInput, ProjectMetrics, ProjectMetricsHandler};
use async_trait::async_trait;

struct Handlers;

#[async_trait]
impl ArchiveProjectHandler for Handlers {
    async fn execute(&self, _ctx: &RequestContext, _input: ArchiveProjectInput) -> Result<project::Model, ApiError> {
        Err(ApiError::NotFound)
    }
}

#[async_trait]
impl ProjectMetricsHandler for Handlers {
    async fn execute(&self, _ctx: &RequestContext) -> Result<ProjectMetrics, ApiError> {
        Ok(ProjectMetrics { active: 0, total: 0 })
    }
}

fn main() { let _extensions = AppExtensions::builder().handlers(Handlers).build(); }
"
}
