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
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum Decision {
    Allowed,
    Denied,
    RequiresRecord,
    RequiresTenant,
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

pub(crate) fn run(project: &Path, command: &AccessCommand) -> ExitCode {
    let AccessCommand::Preview {
        entity,
        authenticated,
        role,
        actor_id,
        tenant_id,
        record,
    } = command;
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
    let record = match read_record(record.as_deref()) {
        Ok(record) => record,
        Err(message) => return invalid(&message),
    };
    let entities = ir
        .entities
        .iter()
        .filter(|item| {
            entity
                .as_ref()
                .is_none_or(|name| item.id.0 == *name || item.rust_name == *name)
        })
        .collect::<Vec<_>>();
    if entities.is_empty() || (entity.is_some() && entities.len() != 1) {
        return invalid("select one existing entity by its name or full IR ID");
    }
    let sample = Sample {
        authenticated: *authenticated || !role.is_empty() || actor_id.is_some(),
        roles: role,
        actor_id: *actor_id,
        tenant_id: *tenant_id,
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
