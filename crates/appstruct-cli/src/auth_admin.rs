use crate::environment::ProjectEnvironment;
use appstruct_ir::{AppIr, EntityIr};
use clap::Subcommand;
use serde_json::{Value, json};
use std::path::Path;
use std::process::ExitCode;

#[derive(Debug, Subcommand)]
pub(crate) enum AuthCommand {
    /// Promote the first registered account to the `admin` role.
    BootstrapAdmin {
        /// Email address of an existing registered user.
        #[arg(long)]
        email: String,
    },
}

pub(crate) fn run(project: &Path, command: &AuthCommand) -> ExitCode {
    match command {
        AuthCommand::BootstrapAdmin { email } => bootstrap_admin(project, email),
    }
}

fn bootstrap_admin(project: &Path, email: &str) -> ExitCode {
    let ir = match appstruct_compiler::compile_project(project) {
        Ok(ir) => ir,
        Err(diagnostics) => {
            return crate::report::fail_diagnostics(
                crate::report::ErrorCategory::Validation,
                diagnostics,
            );
        }
    };
    let Some(user) = bootstrap_user(&ir) else {
        return crate::report::fail(
            "AS6201",
            crate::report::ErrorCategory::Authentication,
            "auth bootstrap requires enabled Auth and a declared `admin` role",
            crate::report::ExitClass::Validation,
        );
    };
    let Some(email) = normalize_email(email) else {
        return crate::report::fail(
            "AS6202",
            crate::report::ErrorCategory::Authentication,
            "invalid administrator email address",
            crate::report::ExitClass::Usage,
        );
    };
    let environment = match ProjectEnvironment::load(project) {
        Ok(environment) => environment,
        Err(error) => {
            return crate::report::fail(
                "AS6203",
                crate::report::ErrorCategory::Configuration,
                format!("cannot load project environment: {error}"),
                crate::report::ExitClass::Environment,
            );
        }
    };
    let Some(database_url) = environment.get("DATABASE_URL") else {
        return crate::report::fail(
            "AS6203",
            crate::report::ErrorCategory::Configuration,
            "DATABASE_URL is required; set it or add it to the project .env file",
            crate::report::ExitClass::Environment,
        );
    };
    match promote(&database_url, &ir, user, &email) {
        Ok(BootstrapResult::Promoted) => {
            render_success(&email, "promoted");
            ExitCode::SUCCESS
        }
        Ok(BootstrapResult::AlreadyAdmin) => {
            render_success(&email, "already_admin");
            ExitCode::SUCCESS
        }
        Err(error) => crate::report::fail(
            "AS6204",
            crate::report::ErrorCategory::Authentication,
            error,
            crate::report::ExitClass::Validation,
        ),
    }
}

fn render_success(email: &str, status: &str) {
    if crate::report::is_json() {
        crate::report::success(&serde_json::json!({
            "command": "auth",
            "action": "bootstrap_admin",
            "email": email,
            "status": status,
        }));
    } else if status == "promoted" {
        println!("Bootstrapped administrator `{email}`");
    } else {
        println!("Administrator `{email}` is already bootstrapped");
    }
}

fn bootstrap_user(ir: &AppIr) -> Option<&EntityIr> {
    if !ir.auth.enabled || !ir.auth.roles.iter().any(|role| role == "admin") {
        return None;
    }
    ir.entities
        .iter()
        .find(|entity| Some(&entity.id) == ir.auth.user_entity.as_ref())
}

fn promote(
    database_url: &str,
    ir: &AppIr,
    user: &EntityIr,
    email: &str,
) -> Result<BootstrapResult, String> {
    let mut client =
        appstruct_migrate::connect_database(database_url).map_err(|error| error.to_string())?;
    let mut transaction = client
        .transaction()
        .map_err(|error| format!("cannot start bootstrap transaction: {error}"))?;
    transaction
        .query_one(
            "SELECT pg_advisory_xact_lock(hashtext('appstruct:bootstrap-admin'))",
            &[],
        )
        .map_err(|error| format!("cannot lock administrator bootstrap: {error}"))?;

    let (id_column, email_column) = user_columns(user)?;
    let account = find_account(
        &mut transaction,
        &user.table_name,
        id_column,
        email_column,
        email,
    )?;
    let administrators = transaction
        .query(
            "SELECT user_id::text FROM \"_appstruct_auth_accounts\" WHERE roles @> '[\"admin\"]'::jsonb ORDER BY user_id",
            &[],
        )
        .map_err(|error| format!("cannot inspect existing administrators: {error}"))?
        .into_iter()
        .map(|row| row.get::<_, String>(0))
        .collect::<Vec<_>>();

    if administrators.iter().any(|id| id != &account.id) {
        return Err("another administrator is already bootstrapped".to_owned());
    }
    if administrators.first() == Some(&account.id) {
        transaction
            .commit()
            .map_err(|error| format!("cannot finish bootstrap transaction: {error}"))?;
        return Ok(BootstrapResult::AlreadyAdmin);
    }

    let mut roles = account.roles;
    roles.push("admin".to_owned());
    roles.sort();
    roles.dedup();
    let roles_json = serde_json::to_string(&roles)
        .map_err(|error| format!("cannot serialize administrator roles: {error}"))?;
    transaction
        .execute(
            "UPDATE \"_appstruct_auth_accounts\" SET roles = $2::text::jsonb WHERE user_id::text = $1",
            &[&account.id, &roles_json],
        )
        .map_err(|error| format!("cannot promote registered user: {error}"))?;
    if ir.audit.enabled {
        record_audit(&mut transaction, &account.id, &account.before, &roles)?;
    }
    transaction
        .commit()
        .map_err(|error| format!("cannot commit administrator bootstrap: {error}"))?;
    Ok(BootstrapResult::Promoted)
}

struct Account {
    id: String,
    roles: Vec<String>,
    before: Value,
}

fn find_account(
    transaction: &mut postgres::Transaction<'_>,
    table: &str,
    id_column: &str,
    email_column: &str,
    email: &str,
) -> Result<Account, String> {
    let sql = format!(
        "SELECT a.user_id::text, a.roles::text FROM \"_appstruct_auth_accounts\" a JOIN {table} u ON u.{id} = a.user_id WHERE LOWER(u.{email}) = $1",
        table = quote_ident(table),
        id = quote_ident(id_column),
        email = quote_ident(email_column),
    );
    let row = transaction
        .query_opt(&sql, &[&email])
        .map_err(|error| format!("cannot find registered user: {error}"))?
        .ok_or_else(|| format!("no registered user has email `{email}`"))?;
    let roles = serde_json::from_str::<Vec<String>>(&row.get::<_, String>(1))
        .map_err(|error| format!("registered user has invalid roles: {error}"))?;
    Ok(Account {
        id: row.get(0),
        before: json!({ "roles": roles }),
        roles,
    })
}

fn record_audit(
    transaction: &mut postgres::Transaction<'_>,
    user_id: &str,
    before: &Value,
    roles: &[String],
) -> Result<(), String> {
    let before = before.to_string();
    let after = json!({ "roles": roles }).to_string();
    transaction
        .execute(
            "INSERT INTO \"_appstruct_audit_events\" (id, entity, record_id, operation, actor_id, tenant_id, before, after, occurred_at) VALUES (gen_random_uuid(), '_appstruct_auth_accounts', $1, 'update', NULL, NULL, $2::text::jsonb, $3::text::jsonb, CURRENT_TIMESTAMP)",
            &[&user_id, &before, &after],
        )
        .map_err(|error| format!("cannot record administrator bootstrap audit event: {error}"))?;
    Ok(())
}

fn user_columns(user: &EntityIr) -> Result<(&str, &str), String> {
    let id = user
        .fields
        .iter()
        .find(|field| field.primary_key)
        .map(|field| field.column_name.as_str())
        .ok_or_else(|| "auth user entity has no primary key".to_owned())?;
    let email = user
        .fields
        .iter()
        .find(|field| field.rust_name == "email")
        .map(|field| field.column_name.as_str())
        .ok_or_else(|| "auth user entity has no email field".to_owned())?;
    Ok((id, email))
}

fn normalize_email(value: &str) -> Option<String> {
    let email = value.trim().to_ascii_lowercase();
    (email.len() <= 320
        && email
            .split_once('@')
            .is_some_and(|(local, domain)| !local.is_empty() && domain.contains('.')))
    .then_some(email)
}

fn quote_ident(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\"\""))
}

enum BootstrapResult {
    Promoted,
    AlreadyAdmin,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn administrator_email_matches_runtime_normalization() {
        assert_eq!(
            normalize_email(" Admin@Example.COM ").as_deref(),
            Some("admin@example.com")
        );
        assert!(normalize_email("invalid@example").is_none());
        assert!(normalize_email("no-at-sign").is_none());
        assert!(normalize_email(&format!("{}@example.com", "a".repeat(320))).is_none());
    }

    #[test]
    fn quote_ident_escapes_embedded_quotes() {
        assert_eq!(quote_ident(r#"user"name"#), r#""user""name""#);
    }

    #[test]
    fn bootstrap_user_requires_enabled_auth_and_admin_role() {
        let ir: AppIr =
            serde_json::from_str(include_str!("../../../tests/golden/m0-app-ir.json")).unwrap();
        assert!(bootstrap_user(&ir).is_some());
        let mut disabled = ir.clone();
        disabled.auth.enabled = false;
        assert!(bootstrap_user(&disabled).is_none());
        let mut no_admin = ir;
        no_admin.auth.roles.retain(|role| role != "admin");
        assert!(bootstrap_user(&no_admin).is_none());
    }

    #[test]
    fn user_columns_require_primary_key_and_email() {
        let ir: AppIr =
            serde_json::from_str(include_str!("../../../tests/golden/m0-app-ir.json")).unwrap();
        let user = bootstrap_user(&ir).unwrap();
        let (id, email) = user_columns(user).unwrap();
        assert!(!id.is_empty());
        assert_eq!(email, "email");

        let mut missing_email = user.clone();
        missing_email
            .fields
            .retain(|field| field.rust_name != "email");
        assert!(user_columns(&missing_email).unwrap_err().contains("email"));

        let mut missing_key = user.clone();
        for field in &mut missing_key.fields {
            field.primary_key = false;
        }
        assert!(
            user_columns(&missing_key)
                .unwrap_err()
                .contains("primary key")
        );
    }

    #[test]
    fn bootstrap_admin_rejects_invalid_projects_and_emails() {
        assert_ne!(
            run(
                Path::new("/missing-appstruct-project"),
                &AuthCommand::BootstrapAdmin {
                    email: "admin@example.com".to_owned(),
                },
            ),
            ExitCode::SUCCESS
        );
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/m0-project");
        assert_ne!(
            run(
                &fixture,
                &AuthCommand::BootstrapAdmin {
                    email: "invalid".to_owned(),
                },
            ),
            ExitCode::SUCCESS
        );
    }

    #[test]
    fn render_success_covers_text_and_json_status_paths() {
        crate::report::set_output_format(crate::report::OutputFormat::Text);
        render_success("admin@example.com", "promoted");
        render_success("admin@example.com", "already_admin");
        crate::report::set_output_format(crate::report::OutputFormat::Json);
        render_success("admin@example.com", "promoted");
        crate::report::set_output_format(crate::report::OutputFormat::Text);
    }

    #[test]
    fn promote_reports_database_connection_failures() {
        let ir: AppIr =
            serde_json::from_str(include_str!("../../../tests/golden/m0-app-ir.json")).unwrap();
        let user = bootstrap_user(&ir).unwrap();
        let Err(error) = promote(
            "postgresql://appstruct:secret@127.0.0.1:1/appstruct?sslmode=disable",
            &ir,
            user,
            "admin@example.com",
        ) else {
            panic!("expected a database connection failure");
        };
        assert!(error.contains("cannot connect") || error.contains("PostgreSQL"));
    }
}
