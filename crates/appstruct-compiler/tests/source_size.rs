use std::fs;
use std::path::Path;

// File size is an emergency backstop. Function-level complexity and length remain enforced by the
// workspace's deny-by-default Clippy configuration.
const MAX_RUST_LINES: usize = 800;
const FORBIDDEN_BLANKET_ALLOWS: &[&str] = &["allow(clippy::all)", "allow(clippy::pedantic)"];

#[test]
fn rust_source_files_avoid_extreme_size_and_blanket_lint_bypasses() {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut violations = Vec::new();
    collect_violations(&workspace.join("crates"), &mut violations);
    assert!(
        violations.is_empty(),
        "Rust source maintainability backstops failed:\n{}",
        violations.join("\n")
    );
}

fn collect_violations(directory: &Path, violations: &mut Vec<String>) {
    for entry in fs::read_dir(directory).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            // Templates and test fixtures are inputs or verification code, not shipped crate modules.
            if matches!(
                path.file_name().and_then(|name| name.to_str()),
                Some("tests" | "templates")
            ) {
                continue;
            }
            collect_violations(&path, violations);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            inspect_source(&path, violations);
        }
    }
}

fn inspect_source(path: &Path, violations: &mut Vec<String>) {
    let source = fs::read_to_string(path).unwrap();
    let line_count = source.lines().count();
    if line_count > MAX_RUST_LINES {
        violations.push(format!(
            "{}: {line_count} lines exceeds emergency limit {MAX_RUST_LINES}",
            path.display()
        ));
    }
    for lint in FORBIDDEN_BLANKET_ALLOWS {
        if source.contains(lint) {
            violations.push(format!(
                "{}: blanket #[{lint}] bypasses workspace quality lints",
                path.display()
            ));
        }
    }
}
