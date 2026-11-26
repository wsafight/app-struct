use appstruct_codegen::{Artifact, plan};
use appstruct_compiler::compile_project;
use std::fs;
use tempfile::TempDir;

fn artifact<'a>(artifacts: &'a [Artifact], path: &str) -> &'a str {
    let bytes = &artifacts
        .iter()
        .find(|item| item.relative_path.as_path() == std::path::Path::new(path))
        .unwrap_or_else(|| panic!("missing artifact: {path}"))
        .content;
    std::str::from_utf8(bytes).expect("utf8 artifact")
}

fn write_project(app_yaml: &str, domain_yaml: &str) -> TempDir {
    let dir = TempDir::new().expect("temp dir");
    fs::create_dir_all(dir.path().join("spec")).expect("spec dir");
    fs::write(dir.path().join("appstruct.yaml"), app_yaml).expect("write appstruct.yaml");
    fs::write(dir.path().join("spec/work.yaml"), domain_yaml).expect("write domain yaml");
    dir
}

#[test]
fn tenant_rls_injects_transaction_tenant_context_for_reads_and_writes() {
    let dir = write_project(
        r"
version: 1
app:
  name: rls-demo
database:
  provider: postgres
includes:
  - spec/work.yaml
modules:
  auth:
    enabled: true
    user_entity: User
  rbac:
    roles:
      - admin
      - member
    default_role: member
  tenant:
    enabled: true
    rls: true
",
        r"
domain: app
entities:
  User:
    fields:
      id:
        type: uuid
        primary_key: true
      email:
        type: string
        required: true
        unique: true
    access:
      list: { role: admin }
      read: { authenticated: true }
      create: { role: admin }
      update: { role: admin }
      delete: { role: admin }
  Task:
    fields:
      id:
        type: uuid
        primary_key: true
      title:
        type: string
        required: true
    access:
      list: { authenticated: true }
      read: { authenticated: true }
      create: { authenticated: true }
      update: { authenticated: true }
      delete: { authenticated: true }
",
    );
    let ir = compile_project(dir.path()).expect("compile rls project");
    let artifacts = plan(&ir).expect("plan rls project");

    let db = artifact(&artifacts, "backend/src/database.rs");
    assert!(db.contains("pub async fn begin_for_tenant"));
    assert!(db.contains("set_config('app.current_tenant_id'"));

    let lib = artifact(&artifacts, "backend/src/lib.rs");
    assert!(lib.contains("self.database.begin_for_tenant(tenant).await"));
    assert!(lib.contains("transaction.set_tenant_context(tenant).await"));

    let ext = artifact(&artifacts, "backend/src/extensions.rs");
    assert!(ext.contains("database.begin_for_tenant(self.tenant).await?"));

    let api = artifact(&artifacts, "backend/src/api/task.rs");
    assert!(api.contains("state.begin_transaction("));
}

#[test]
fn dashboard_generates_authenticated_manifest_and_web_page() {
    let dir = write_project(
        r"
version: 1
app:
  name: dash-demo
database:
  provider: postgres
includes:
  - spec/work.yaml
modules:
  auth:
    enabled: true
    user_entity: User
  rbac:
    roles:
      - admin
    default_role: admin
dashboard:
  title: Executive Overview
  widgets:
    - id: tasks_total
      title: Total Tasks
      entity: Task
      chart: tasks_by_status
      span: 6
",
        r"
domain: app
entities:
  User:
    fields:
      id:
        type: uuid
        primary_key: true
      email:
        type: string
        required: true
        unique: true
    access:
      list: { role: admin }
      read: { authenticated: true }
      create: { role: admin }
      update: { role: admin }
      delete: { role: admin }
  Task:
    fields:
      id:
        type: uuid
        primary_key: true
      title:
        type: string
        required: true
      status:
        type: string
        required: true
        filterable: true
    charts:
      tasks_by_status:
        label: Tasks by status
        type: bar
        dimension: status
        measure: count
    access:
      list: { authenticated: true }
      read: { authenticated: true }
      create: { authenticated: true }
      update: { authenticated: true }
      delete: { authenticated: true }
",
    );
    let ir = compile_project(dir.path()).expect("compile dashboard project");
    let artifacts = plan(&ir).expect("plan dashboard project");

    let lib = artifact(&artifacts, "backend/src/lib.rs");
    assert!(lib.contains(".route(\"/api/dashboard\", get(dashboard_manifest))"));
    assert!(lib.contains("if context.actor.is_none()"));
    assert!(lib.contains("/api/tasks/aggregate"));

    let page = artifact(&artifacts, "web/src/dashboard/DashboardPage.tsx");
    assert!(page.contains("Executive Overview"));
    assert!(page.contains("tasks_by_status"));

    let app = artifact(&artifacts, "web/src/app/App.tsx");
    assert!(app.contains("path: \"/dashboard\""));
    assert!(app.contains("Navigate to=\"/dashboard\""));

    let layout = artifact(&artifacts, "web/src/app/Layout.tsx");
    assert!(layout.contains("to=\"/dashboard\""));
}
