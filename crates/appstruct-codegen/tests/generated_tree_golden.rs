use appstruct_codegen::{Artifact, plan};
use appstruct_compiler::compile_project;
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

/// Fixtures whose generated tree is pinned by a manifest. Each one covers a distinct feature
/// surface so a generator regression fails in the smallest possible place.
const FIXTURES: &[&str] = &[
    "m0-project",
    "m2-project",
    "m3-project",
    "m6-preset-project",
    "m8-server-project",
];

/// Artifacts that are byte-identical across every fixture and are verbatim copies of a template.
/// Hashing them would add noise on every dependency bump, so the test instead asserts they still
/// match the template.
const TEMPLATE_BACKED: &[(&str, &str)] = &[
    (
        "web/pnpm-lock.yaml",
        include_str!("../templates/web/pnpm-lock.yaml"),
    ),
    ("web/.gitignore", include_str!("../templates/web/gitignore")),
];

const MANIFEST_HEADER: &str = concat!(
    "# Generated tree manifest. Do not edit by hand.\n",
    "# Regenerate with: UPDATE_GOLDEN=1 cargo test -p appstruct-codegen --test generated_tree_golden\n",
    "# Format: <sha256>  <path>, sorted by path.\n",
);

#[test]
fn generated_trees_match_the_checked_in_manifests() {
    for fixture in FIXTURES {
        assert_fixture(fixture);
    }
}

#[test]
fn template_backed_artifacts_are_unchanged_copies() {
    for fixture in FIXTURES {
        let artifacts = planned_artifacts(fixture);
        for (path, expected) in TEMPLATE_BACKED {
            let artifact = artifacts
                .iter()
                .find(|artifact| artifact.relative_path == Path::new(path))
                .unwrap_or_else(|| panic!("{fixture} does not emit {path}"));
            assert_eq!(
                std::str::from_utf8(&artifact.content).unwrap(),
                *expected,
                "{fixture}: {path} drifted from its template"
            );
        }
    }
}

#[test]
fn generated_trees_are_reproducible() {
    for fixture in FIXTURES {
        let ir = compile_project(&fixture_path(fixture)).unwrap();
        let first = plan(&ir).unwrap();
        let second = plan(&ir).unwrap();
        assert_eq!(first, second, "{fixture} is not byte deterministic");
    }
}

fn assert_fixture(fixture: &str) {
    let artifacts = planned_artifacts(fixture);
    let actual = render_manifest(&artifacts);
    let manifest = manifest_path(fixture);
    if std::env::var_os("UPDATE_GOLDEN").is_some() {
        fs::write(&manifest, &actual).unwrap();
        return;
    }
    let expected = fs::read_to_string(&manifest).unwrap_or_else(|error| {
        panic!(
            "cannot read {}\nrun `UPDATE_GOLDEN=1 cargo test -p appstruct-codegen --test generated_tree_golden` ({error})",
            manifest.display()
        )
    });
    if expected == actual {
        return;
    }
    dump_for_inspection(fixture, &artifacts);
    panic!("{}", describe_difference(fixture, &expected, &actual));
}

/// Hashes every artifact except the template-backed copies, sorted by path for a stable diff.
fn render_manifest(artifacts: &[Artifact]) -> String {
    let mut entries = artifacts
        .iter()
        .filter(|artifact| !is_template_backed(&artifact.relative_path))
        .map(|artifact| {
            (
                portable_path(&artifact.relative_path),
                appstruct_core::sha256_hex(&artifact.content),
            )
        })
        .collect::<Vec<_>>();
    entries.sort_by(|left, right| left.0.cmp(&right.0));
    let mut manifest = String::from(MANIFEST_HEADER);
    for (path, digest) in entries {
        let _ = writeln!(manifest, "{digest}  {path}");
    }
    manifest
}

fn describe_difference(fixture: &str, expected: &str, actual: &str) -> String {
    let expected = parse_manifest(expected);
    let actual = parse_manifest(actual);
    let mut added = Vec::new();
    let mut removed = Vec::new();
    let mut changed = Vec::new();
    for (path, digest) in &actual {
        match expected.get(path) {
            None => added.push(path.clone()),
            Some(previous) if previous != digest => changed.push(path.clone()),
            Some(_) => {}
        }
    }
    for path in expected.keys() {
        if !actual.contains_key(path) {
            removed.push(path.clone());
        }
    }
    let mut message = format!("{fixture}: generated tree does not match its manifest\n");
    for (label, paths) in [
        ("added", &added),
        ("removed", &removed),
        ("changed", &changed),
    ] {
        if paths.is_empty() {
            continue;
        }
        let _ = writeln!(message, "\n{label} ({}):", paths.len());
        for path in paths.iter().take(20) {
            let _ = writeln!(message, "  {path}");
        }
        if paths.len() > 20 {
            let _ = writeln!(message, "  ... and {} more", paths.len() - 20);
        }
    }
    let _ = writeln!(
        message,
        "\nThe current tree was written to target/golden-actual/{fixture}/ for inspection.\n\
         Accept with: UPDATE_GOLDEN=1 cargo test -p appstruct-codegen --test generated_tree_golden"
    );
    message
}

fn parse_manifest(source: &str) -> BTreeMap<String, String> {
    source
        .lines()
        .filter(|line| !line.starts_with('#') && !line.is_empty())
        .filter_map(|line| line.split_once("  "))
        .map(|(digest, path)| (path.to_owned(), digest.to_owned()))
        .collect()
}

/// Writes the freshly generated tree next to the build output so a reviewer can inspect the actual
/// bytes when a manifest hash changes.
fn dump_for_inspection(fixture: &str, artifacts: &[Artifact]) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/golden-actual")
        .join(fixture);
    let _ = fs::remove_dir_all(&root);
    for artifact in artifacts {
        let destination = root.join(&artifact.relative_path);
        if let Some(parent) = destination.parent() {
            let _ = fs::create_dir_all(parent);
            let _ = fs::write(destination, &artifact.content);
        }
    }
}

fn planned_artifacts(fixture: &str) -> Vec<Artifact> {
    let ir = compile_project(&fixture_path(fixture)).unwrap();
    plan(&ir).unwrap()
}

fn is_template_backed(path: &Path) -> bool {
    TEMPLATE_BACKED
        .iter()
        .any(|(skipped, _)| path == Path::new(skipped))
}

/// Manifests must be identical across platforms, so separators are always `/`.
fn portable_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn fixture_path(fixture: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures")
        .join(fixture)
}

fn manifest_path(fixture: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/golden")
        .join(format!("{fixture}-generated.manifest"))
}
