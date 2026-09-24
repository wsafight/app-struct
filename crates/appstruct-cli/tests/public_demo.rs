use std::fs;
use std::path::Path;
use std::process::Command;

#[test]
fn public_demo_recipe_creates_a_valid_tenant_application() {
    let temporary = tempfile::tempdir().unwrap();
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/public-demo/create.sh");
    let output = Command::new("bash")
        .arg(&script)
        .arg(temporary.path())
        .env("APPSTRUCT_BIN", env!("CARGO_BIN_EXE_appstruct"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let project = temporary.path().join("public-demo");
    let app = fs::read_to_string(project.join("appstruct.yaml")).unwrap();
    let work = fs::read_to_string(project.join("spec/work.yaml")).unwrap();
    assert!(app.contains("reader_roles: [member, admin]"));
    assert!(work.contains("target: Project"));
    assert!(work.contains("tenant: true"));
    assert!(work.contains("audit: true"));
    let check = Command::new(env!("CARGO_BIN_EXE_appstruct"))
        .arg("--project")
        .arg(&project)
        .arg("check")
        .output()
        .unwrap();
    assert!(check.status.success());
    let repeated = Command::new("bash")
        .arg(script)
        .arg(temporary.path())
        .env("APPSTRUCT_BIN", env!("CARGO_BIN_EXE_appstruct"))
        .output()
        .unwrap();
    assert!(!repeated.status.success());
}
