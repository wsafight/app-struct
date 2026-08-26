use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::Mutex;

static CARGO_CHECK_LOCK: Mutex<()> = Mutex::new(());

pub fn cargo_check(manifest: &Path, library_only: bool) -> Output {
    let _guard = CARGO_CHECK_LOCK
        .lock()
        .expect("generated crate check lock is not poisoned");
    let _ = prepare_generated_package(manifest);
    let mut command = Command::new("cargo");
    command
        .args(["check", "--quiet", "--manifest-path"])
        .arg(manifest)
        .env("CARGO_TARGET_DIR", generated_target())
        .env("CARGO_INCREMENTAL", "0");
    if library_only {
        command.arg("--lib");
    }
    command.output().unwrap()
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
