use appstruct_ir::{AccessRuleIr, AppIr, EntityIr, GeneratedValueIr};
use clap::Subcommand;
use serde::Serialize;
use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
    process::ExitCode,
};

#[derive(Debug, Subcommand)]
pub(crate) enum AccessCommand {
    /// Read-only declaration matrix; application Rust Policy is not executed.
    Preview {
        #[arg(long)]
        entity: Option<String>,
        /// Treat the sample actor as authenticated.
        #[arg(long)]
        authenticated: bool,
        /// Role(s) for one sample actor; repeat for multiple roles.
        #[arg(long)]
        role: Vec<String>,
        #[arg(long)]
        actor_id: Option<uuid::Uuid>,
        #[arg(long)]
        tenant_id: Option<uuid::Uuid>,
        /// Local sample API record, required for evaluating owner relations.
        #[arg(long, requires = "entity")]
        record: Option<PathBuf>,
    },
    /// Generate a full role-by-entity/field permission matrix (HTML or JSON).
    Matrix {
        /// Optional entity filter (name or full IR ID).
        #[arg(long)]
        entity: Option<String>,
        /// Write the self-contained HTML audit report to this file path (defaults to stdout).
        #[arg(long)]
        output: Option<PathBuf>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum Decision {
    Allowed,
    Denied,
    RequiresRecord,
    RequiresTenant,
    RequiresEntitlement,
}

#[derive(Serialize)]
struct Evaluation {
    decision: Decision,
    reasons: Vec<String>,
}

struct Sample<'a> {
    authenticated: bool,
    roles: &'a [String],
    actor_id: Option<uuid::Uuid>,
    tenant_id: Option<uuid::Uuid>,
    record: Option<&'a Value>,
}

#[derive(Serialize)]
struct AccessRow {
    entity: String,
    operation: String,
    field: Option<String>,
    rule: AccessRuleIr,
    declarative: Evaluation,
    runtime_policy: &'static str,
}

pub(crate) fn preview_value(
    project: &Path,
    entity: Option<&str>,
    authenticated: bool,
    roles: &[String],
    actor_id: Option<uuid::Uuid>,
    tenant_id: Option<uuid::Uuid>,
    record: Option<&Value>,
) -> Result<Value, Value> {
    let ir = appstruct_compiler::compile_project(project)
        .map_err(|diagnostics| serde_json::json!({ "error": "validation_failed", "diagnostics": diagnostics }))?;
    if roles.iter().any(|role| !ir.auth.roles.contains(role)) {
        return Err(serde_json::json!({
            "error": "invalid_role",
            "message": "sample roles must be declared in modules.rbac.roles",
            "declared_roles": ir.auth.roles,
        }));
    }
    let entities = ir
        .entities
        .iter()
        .filter(|item| entity.is_none_or(|name| item.id.0 == name || item.rust_name == name))
        .collect::<Vec<_>>();
    if entities.is_empty() || (entity.is_some() && entities.len() != 1) {
        return Err(serde_json::json!({
            "error": "invalid_entity",
            "message": "select one existing entity by its name or full IR ID",
            "available_entities": ir.entities.iter().map(|e| &e.id.0).collect::<Vec<_>>(),
        }));
    }
    let sample = Sample {
        authenticated: authenticated || !roles.is_empty() || actor_id.is_some(),
        roles,
        actor_id,
        tenant_id,
        record,
    };
    let rows = entities
        .into_iter()
        .flat_map(|item| matrix(&ir, item, &sample))
        .collect::<Vec<_>>();
    Ok(serde_json::json!({
        "command": "access",
        "action": "preview",
        "scope": "declarations_only",
        "sample_values_included": false,
        "rows": rows,
    }))
}

pub(crate) fn run(project: &Path, command: &AccessCommand) -> ExitCode {
    match command {
        AccessCommand::Preview {
            entity,
            authenticated,
            role,
            actor_id,
            tenant_id,
            record,
        } => run_preview(
            project,
            entity.as_deref(),
            *authenticated,
            role,
            *actor_id,
            *tenant_id,
            record.as_deref(),
        ),
        AccessCommand::Matrix { entity, output } => {
            run_matrix(project, entity.as_deref(), output.as_deref())
        }
    }
}

fn run_matrix(project: &Path, entity: Option<&str>, output: Option<&Path>) -> ExitCode {
    let ir = match appstruct_compiler::compile_project(project) {
        Ok(ir) => ir,
        Err(diagnostics) => {
            return crate::report::fail_diagnostics(
                crate::report::ErrorCategory::Validation,
                diagnostics,
            );
        }
    };
    let entities = ir
        .entities
        .iter()
        .filter(|item| entity.is_none_or(|name| item.id.0 == name || item.rust_name == name))
        .collect::<Vec<_>>();
    if entities.is_empty() || (entity.is_some() && entities.len() != 1) {
        return invalid("select one existing entity by its name or full IR ID");
    }
    let tenant_id = ir.tenant.enabled.then(uuid::Uuid::nil);
    let mut profiles: Vec<(String, bool, Vec<String>)> = vec![
        ("anonymous".to_owned(), false, Vec::new()),
        ("authenticated".to_owned(), true, Vec::new()),
    ];
    for role in &ir.auth.roles {
        profiles.push((format!("role:{role}"), true, vec![role.clone()]));
    }
    let mut row_meta = Vec::new();
    let mut actors = Vec::new();
    for (profile_index, (label, authenticated, roles)) in profiles.iter().enumerate() {
        let sample = Sample {
            authenticated: *authenticated,
            roles,
            actor_id: None,
            tenant_id,
            record: None,
        };
        let evaluated = entities
            .iter()
            .flat_map(|item| matrix(&ir, item, &sample))
            .collect::<Vec<_>>();
        if profile_index == 0 {
            row_meta = evaluated
                .iter()
                .map(|row| crate::access_matrix_html::MatrixRowMeta {
                    entity: row.entity.clone(),
                    operation: row.operation.clone(),
                    field: row.field.clone(),
                })
                .collect();
        }
        let decisions = evaluated
            .iter()
            .map(|row| decision_badge(row.declarative.decision))
            .collect();
        actors.push(crate::access_matrix_html::MatrixActor {
            label: label.clone(),
            decisions,
        });
    }
    let html = crate::access_matrix_html::render_html(&ir.app.name, &row_meta, &actors);
    if let Some(path) = output {
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
            && let Err(error) = fs::create_dir_all(parent)
        {
            return invalid(&format!("cannot create output directory: {error}"));
        }
        if let Err(error) = fs::write(path, &html) {
            return invalid(&format!("cannot write HTML matrix report: {error}"));
        }
        if crate::report::is_json() {
            crate::report::success(&serde_json::json!({
                "command": "access",
                "action": "matrix",
                "output": path,
                "entity_count": entities.len(),
                "role_count": ir.auth.roles.len(),
                "row_count": row_meta.len(),
            }));
        } else {
            println!("Wrote access permission matrix to {}", path.display());
        }
    } else if crate::report::is_json() {
        crate::report::success(&serde_json::json!({
            "command": "access",
            "action": "matrix",
            "scope": "declarations_only",
            "actors": actors.iter().map(|a| &a.label).collect::<Vec<_>>(),
            "row_count": row_meta.len(),
            "html": html,
        }));
    } else {
        print!("{html}");
    }
    ExitCode::SUCCESS
}

pub(crate) fn matrix_value(
    project: &Path,
    entity: Option<&str>,
) -> Result<serde_json::Value, serde_json::Value> {
    let ir = appstruct_compiler::compile_project(project).map_err(|diagnostics| {
        serde_json::json!({
            "error": "validation_failed",
            "diagnostics": diagnostics
        })
    })?;
    let entities = ir
        .entities
        .iter()
        .filter(|item| entity.is_none_or(|name| item.id.0 == name || item.rust_name == name))
        .collect::<Vec<_>>();
    if entities.is_empty() || (entity.is_some() && entities.len() != 1) {
        return Err(serde_json::json!({
            "error": "invalid_entity",
            "message": "select one existing entity by its name or full IR ID"
        }));
    }
    let tenant_id = ir.tenant.enabled.then(uuid::Uuid::nil);
    let mut profiles: Vec<(String, bool, Vec<String>)> = vec![
        ("anonymous".to_owned(), false, Vec::new()),
        ("authenticated".to_owned(), true, Vec::new()),
    ];
    for role in &ir.auth.roles {
        profiles.push((format!("role:{role}"), true, vec![role.clone()]));
    }
    let mut row_meta = Vec::new();
    let mut actor_labels = Vec::new();
    let mut actor_decisions: Vec<Vec<&'static str>> = Vec::new();
    for (profile_index, (label, authenticated, roles)) in profiles.iter().enumerate() {
        let sample = Sample {
            authenticated: *authenticated,
            roles,
            actor_id: None,
            tenant_id,
            record: None,
        };
        let evaluated = entities
            .iter()
            .flat_map(|item| matrix(&ir, item, &sample))
            .collect::<Vec<_>>();
        if profile_index == 0 {
            row_meta = evaluated
                .iter()
                .map(|row| (row.entity.clone(), row.operation.clone(), row.field.clone()))
                .collect();
        }
        actor_labels.push(label.clone());
        actor_decisions.push(
            evaluated
                .iter()
                .map(|row| decision_badge(row.declarative.decision).code)
                .collect(),
        );
    }
    let rows = row_meta
        .iter()
        .enumerate()
        .map(|(row_index, (entity_name, operation, field))| {
            let decisions: serde_json::Map<String, serde_json::Value> = actor_labels
                .iter()
                .enumerate()
                .map(|(actor_index, label)| {
                    let code = actor_decisions
                        .get(actor_index)
                        .and_then(|items| items.get(row_index))
                        .copied()
                        .unwrap_or("denied");
                    (label.clone(), serde_json::Value::String(code.to_owned()))
                })
                .collect();
            serde_json::json!({
                "entity": entity_name,
                "operation": operation,
                "field": field,
                "decisions": decisions,
            })
        })
        .collect::<Vec<_>>();
    Ok(serde_json::json!({
        "app_name": ir.app.name,
        "scope": "declarations_only",
        "entity_count": entities.len(),
        "role_count": ir.auth.roles.len(),
        "actors": actor_labels,
        "rows": rows,
    }))
}

const fn decision_badge(decision: Decision) -> crate::access_matrix_html::MatrixDecision {
    match decision {
        Decision::Allowed => crate::access_matrix_html::MatrixDecision {
            code: "allowed",
            label: "Allowed",
            css_class: "allowed",
        },
        Decision::Denied => crate::access_matrix_html::MatrixDecision {
            code: "denied",
            label: "Denied",
            css_class: "denied",
        },
        Decision::RequiresRecord => crate::access_matrix_html::MatrixDecision {
            code: "requires_record",
            label: "Owner / Record",
            css_class: "conditional",
        },
        Decision::RequiresTenant => crate::access_matrix_html::MatrixDecision {
            code: "requires_tenant",
            label: "Tenant",
            css_class: "conditional",
        },
        Decision::RequiresEntitlement => crate::access_matrix_html::MatrixDecision {
            code: "requires_entitlement",
            label: "Entitlement",
            css_class: "conditional",
        },
    }
}

fn run_preview(
    project: &Path,
    entity: Option<&str>,
    authenticated: bool,
    role: &[String],
    actor_id: Option<uuid::Uuid>,
    tenant_id: Option<uuid::Uuid>,
    record: Option<&Path>,
) -> ExitCode {
    let ir = match appstruct_compiler::compile_project(project) {
        Ok(ir) => ir,
        Err(diagnostics) => {
            return crate::report::fail_diagnostics(
                crate::report::ErrorCategory::Validation,
                diagnostics,
            );
        }
    };
    if role.iter().any(|role| !ir.auth.roles.contains(role)) {
        return invalid("sample roles must be declared in modules.rbac.roles");
    }
    let record = match read_record(record) {
        Ok(record) => record,
        Err(message) => return invalid(&message),
    };
    let entities = ir
        .entities
        .iter()
        .filter(|item| entity.is_none_or(|name| item.id.0 == name || item.rust_name == name))
        .collect::<Vec<_>>();
    if entities.is_empty() || (entity.is_some() && entities.len() != 1) {
        return invalid("select one existing entity by its name or full IR ID");
    }
    let sample = Sample {
        authenticated: authenticated || !role.is_empty() || actor_id.is_some(),
        roles: role,
        actor_id,
        tenant_id,
        record: record.as_ref(),
    };
    let rows = entities
        .into_iter()
        .flat_map(|entity| matrix(&ir, entity, &sample))
        .collect::<Vec<_>>();
    if crate::report::is_json() {
        crate::report::success(
            &serde_json::json!({ "command": "access", "action": "preview", "scope": "declarations_only", "sample_values_included": false, "rows": rows }),
        );
    } else {
        println!("Declared access preview (sample actor; Rust Policy is not executed)");
        for row in rows {
            println!(
                "{} {}{}: {:?}",
                row.entity,
                row.operation,
                row.field
                    .as_ref()
                    .map_or_else(String::new, |field| format!(" field {field}")),
                row.declarative.decision
            );
            for reason in row.declarative.reasons {
                println!("  {reason}");
            }
        }
        println!(
            "Verify runtime Policy, current database state and tenant membership with application tests."
        );
    }
    ExitCode::SUCCESS
}

fn read_record(path: Option<&Path>) -> Result<Option<Value>, String> {
    let Some(path) = path else {
        return Ok(None);
    };
    if fs::metadata(path)
        .map_err(|_| "cannot inspect sample record")?
        .len()
        > 1_048_576
    {
        return Err("sample record must be at most 1 MiB".to_owned());
    }
    let source = fs::read(path).map_err(|_| "cannot read sample record")?;
    let value: Value =
        serde_json::from_slice(&source).map_err(|_| "sample record must be valid JSON")?;
    if !value.is_object() {
        return Err("sample record must be a JSON object".to_owned());
    }
    Ok(Some(value))
}

fn matrix(ir: &AppIr, entity: &EntityIr, sample: &Sample<'_>) -> Vec<AccessRow> {
    let mut rows = Vec::new();
    for (operation, rule) in [
        ("list", &entity.access.list),
        ("read", &entity.access.read),
        ("create", &entity.access.create),
        ("update", &entity.access.update),
        ("delete", &entity.access.delete),
    ] {
        let mut item = row(entity, operation, None, rule, sample);
        if !["list", "read"].contains(&operation)
            && ir.entities.iter().any(|parent| {
                parent
                    .views
                    .aggregates
                    .iter()
                    .any(|aggregate| aggregate.child == entity.id)
            })
        {
            item.declarative = evaluation(
                Decision::Denied,
                "aggregate child writes must use the parent collection API",
            );
        }
        rows.push(item);
    }
    for field in &entity.fields {
        rows.push(row(
            entity,
            "field_read",
            Some(&field.api_name),
            &AccessRuleIr::All {
                rules: vec![
                    entity.access.read.clone(),
                    field.read_access.clone().unwrap_or(AccessRuleIr::Public),
                ],
            },
            sample,
        ));
        let mut write = row(
            entity,
            "field_write",
            Some(&field.api_name),
            &AccessRuleIr::All {
                rules: vec![
                    entity.access.update.clone(),
                    field.write_access.clone().unwrap_or(AccessRuleIr::Public),
                ],
            },
            sample,
        );
        if field.generated.is_some() || field.primary_key || entity.is_workflow_field(field) {
            write.declarative = evaluation(
                Decision::Denied,
                "field is generated, a primary key or managed by Workflow; ordinary update cannot write it",
            );
        }
        rows.push(write);
    }
    rows
}

fn row(
    entity: &EntityIr,
    operation: &str,
    field: Option<&str>,
    rule: &AccessRuleIr,
    sample: &Sample<'_>,
) -> AccessRow {
    let mut declared = evaluate(rule, entity, sample);
    if entity.tenant_scoped && declared.decision != Decision::Denied {
        if let Some(tenant) = sample.tenant_id {
            if let Some(record) = sample.record {
                let tenant_field = entity
                    .fields
                    .iter()
                    .find(|field| field.generated == Some(GeneratedValueIr::Tenant));
                match tenant_field.and_then(|field| record.get(&field.api_name)).and_then(Value::as_str) {
                    Some(value) if value == tenant.to_string() => declared.reasons.push("sample record matches tenant; membership still requires runtime verification".to_owned()),
                    Some(_) => declared = evaluation(Decision::Denied, "sample record belongs to another tenant"),
                    None => declared = evaluation(Decision::RequiresRecord, "sample record must include its tenant field"),
                }
            } else {
                declared
                    .reasons
                    .push("tenant membership is checked at runtime".to_owned());
            }
        } else {
            declared = evaluation(
                Decision::RequiresTenant,
                "tenant-scoped resource requires an explicit tenant context",
            );
        }
    }
    AccessRow {
        entity: entity.id.0.clone(),
        operation: operation.to_owned(),
        field: field.map(str::to_owned),
        rule: rule.clone(),
        declarative: declared,
        runtime_policy: "not_evaluated",
    }
}

fn evaluate(rule: &AccessRuleIr, entity: &EntityIr, sample: &Sample<'_>) -> Evaluation {
    match rule {
        AccessRuleIr::Public => evaluation(Decision::Allowed, "public access is declared"),
        AccessRuleIr::Authenticated => evaluation(
            if sample.authenticated {
                Decision::Allowed
            } else {
                Decision::Denied
            },
            "authentication is required",
        ),
        AccessRuleIr::Role { role } => evaluation(
            if sample.authenticated && sample.roles.contains(role) {
                Decision::Allowed
            } else {
                Decision::Denied
            },
            &format!("requires declared role `{role}`"),
        ),
        AccessRuleIr::Owner { field } => {
            let owner = entity
                .fields
                .iter()
                .find(|item| item.id == *field)
                .and_then(|item| sample.record.and_then(|record| record.get(&item.api_name)))
                .and_then(Value::as_str);
            match (sample.actor_id, owner) {
                _ if !sample.authenticated => {
                    evaluation(Decision::Denied, "owner access requires authentication")
                }
                (Some(actor), Some(owner)) => evaluation(
                    if owner == actor.to_string() {
                        Decision::Allowed
                    } else {
                        Decision::Denied
                    },
                    "compared sample actor ID with declared ownership relation",
                ),
                _ => evaluation(
                    Decision::RequiresRecord,
                    "owner access requires --actor-id and a sample record containing the ownership relation",
                ),
            }
        }
        AccessRuleIr::Entitlement { entitlement } => evaluation(
            Decision::RequiresEntitlement,
            &format!("active subscription must include entitlement `{entitlement}`"),
        ),
        AccessRuleIr::Any { rules } | AccessRuleIr::All { rules } => {
            let any = matches!(rule, AccessRuleIr::Any { .. });
            let evaluations = rules
                .iter()
                .map(|rule| evaluate(rule, entity, sample))
                .collect::<Vec<_>>();
            let decisive = if any {
                Decision::Allowed
            } else {
                Decision::Denied
            };
            let decision = if evaluations.iter().any(|item| item.decision == decisive) {
                decisive
            } else if evaluations
                .iter()
                .any(|item| item.decision == Decision::RequiresRecord)
            {
                Decision::RequiresRecord
            } else if any {
                Decision::Denied
            } else {
                Decision::Allowed
            };
            let mut reasons = vec![
                if any {
                    "at least one branch must allow access"
                } else {
                    "every branch must allow access"
                }
                .to_owned(),
            ];
            reasons.extend(evaluations.into_iter().flat_map(|item| item.reasons));
            Evaluation { decision, reasons }
        }
    }
}

fn evaluation(decision: Decision, reason: &str) -> Evaluation {
    Evaluation {
        decision,
        reasons: vec![reason.to_owned()],
    }
}
fn invalid(message: &str) -> ExitCode {
    crate::report::fail(
        "AS6401",
        crate::report::ErrorCategory::Configuration,
        message,
        crate::report::ExitClass::Usage,
    )
}
