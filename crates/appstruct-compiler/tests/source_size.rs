use std::fs;
use std::path::Path;

const MAX_RUST_LINES: usize = 400;

#[test]
fn rust_source_files_stay_reviewable() {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut oversized = Vec::new();
    collect_oversized(&workspace.join("crates"), &mut oversized);
    assert!(
        oversized.is_empty(),
        "Rust source files exceed {MAX_RUST_LINES} lines:\n{}",
        oversized.join("\n")
    );
}

fn collect_oversized(directory: &Path, oversized: &mut Vec<String>) {
    for entry in fs::read_dir(directory).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            collect_oversized(&path, oversized);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            record_if_oversized(&path, oversized);
        }
    }
}

fn record_if_oversized(path: &Path, oversized: &mut Vec<String>) {
    let line_count = fs::read_to_string(path).unwrap().lines().count();
    if line_count > MAX_RUST_LINES {
        oversized.push(format!("{}: {line_count}", path.display()));
    }
}
