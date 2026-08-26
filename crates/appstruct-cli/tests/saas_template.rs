use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

#[test]
fn saas_template_matches_the_checked_in_demo() {
    let temporary = tempfile::tempdir().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_appstruct"))
        .current_dir(temporary.path())
        .args(["new", "saas-demo", "--template", "saas"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let expected = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/saas-demo");
    assert_directories_equal(&temporary.path().join("saas-demo"), &expected);
}

fn assert_directories_equal(actual: &Path, expected: &Path) {
    let actual_entries = entries(actual);
    let expected_entries = entries(expected);
    assert_eq!(actual_entries, expected_entries, "directory entries differ");
    for name in actual_entries {
        let actual_path = actual.join(&name);
        let expected_path = expected.join(name);
        if actual_path.is_dir() {
            assert!(expected_path.is_dir());
            assert_directories_equal(&actual_path, &expected_path);
        } else {
            assert_eq!(
                fs::read(&actual_path).unwrap(),
                fs::read(&expected_path).unwrap(),
                "file bytes differ for {}",
                actual_path.display()
            );
        }
    }
}

fn entries(directory: &Path) -> Vec<PathBuf> {
    let mut entries = fs::read_dir(directory)
        .unwrap()
        .map(|entry| PathBuf::from(entry.unwrap().file_name()))
        .collect::<Vec<_>>();
    entries.sort();
    entries
}
