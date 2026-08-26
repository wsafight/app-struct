use appstruct_ir::{DatabaseProvider, Diagnostic};
use appstruct_migrate::{
    DatabaseSchema, MigrationPlan, SchemaChange, diff, extract, from_json, migration_sql, to_json,
};
use clap::Subcommand;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

#[derive(Clone, Copy, Debug, Subcommand)]
pub(crate) enum MigrateCommand {
    /// Show the schema diff without writing files or connecting to a database.
    Plan,
    /// Accept a safe development migration and update the schema snapshot.
    Dev {
        /// Confirm creation of the migration and snapshot in non-interactive environments.
        #[arg(long)]
        accept: bool,
    },
}

pub(crate) fn run(project: &Path, command: MigrateCommand) -> ExitCode {
    let target = match appstruct_compiler::compile_project(project) {
        Ok(ir) => extract(&ir),
        Err(diagnostics) => {
            render_diagnostics(&diagnostics);
            return ExitCode::from(1);
        }
    };
    let before = match read_snapshot(project) {
        Ok(Some(schema)) => schema,
        Ok(None) => empty_schema(),
        Err(error) => {
            eprintln!("error[AS4101]: cannot read schema snapshot: {error}");
            return ExitCode::from(1);
        }
    };
    let plan = diff(&before, &target);
    render_plan(&plan);
    match command {
        MigrateCommand::Plan => ExitCode::SUCCESS,
        MigrateCommand::Dev { accept } => accept_plan(project, &target, &plan, accept),
    }
}

fn accept_plan(
    project: &Path,
    target: &DatabaseSchema,
    plan: &MigrationPlan,
    accept: bool,
) -> ExitCode {
    if plan.is_empty() {
        println!("Schema snapshot is current");
        return ExitCode::SUCCESS;
    }
    if plan.is_blocked() {
        eprintln!("error[AS4102]: migration contains destructive or review-required changes");
        return ExitCode::from(1);
    }
    if !accept {
        eprintln!("error[AS4103]: pass `--accept` to create this development migration");
        return ExitCode::from(2);
    }
    let sql = match migration_sql(plan) {
        Ok(sql) => sql,
        Err(error) => {
            eprintln!("error[AS4104]: cannot render migration: {error}");
            return ExitCode::from(1);
        }
    };
    let snapshot = match to_json(target) {
        Ok(snapshot) => snapshot,
        Err(error) => {
            eprintln!("error[AS4105]: cannot serialize schema snapshot: {error}");
            return ExitCode::from(1);
        }
    };
    match write_plan(project, &sql, &snapshot) {
        Ok(path) => {
            println!("Created safe migration {}", path.display());
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("error[AS4106]: cannot commit migration plan: {error}");
            ExitCode::from(3)
        }
    }
}

fn read_snapshot(project: &Path) -> io::Result<Option<DatabaseSchema>> {
    let path = project.join(".appstruct/schema.snapshot.json");
    match fs::read_to_string(path) {
        Ok(source) => from_json(&source)
            .map(Some)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

fn empty_schema() -> DatabaseSchema {
    DatabaseSchema {
        schema_version: 1,
        provider: DatabaseProvider::Postgres,
        tables: Vec::new(),
        foreign_keys: Vec::new(),
    }
}

fn write_plan(project: &Path, sql: &str, snapshot: &str) -> io::Result<PathBuf> {
    let migrations = project.join("migrations");
    let state = project.join(".appstruct");
    fs::create_dir_all(&migrations)?;
    fs::create_dir_all(&state)?;
    let migration = migrations.join(next_migration_name(&migrations)?);
    let migration_staging = migration.with_extension("sql.tmp");
    let snapshot_path = state.join("schema.snapshot.json");
    let snapshot_staging = state.join("schema.snapshot.json.tmp");
    fs::write(&migration_staging, sql)?;
    if let Err(error) = fs::write(&snapshot_staging, snapshot) {
        let _ = fs::remove_file(&migration_staging);
        return Err(error);
    }
    fs::rename(&migration_staging, &migration)?;
    if let Err(error) = fs::rename(&snapshot_staging, &snapshot_path) {
        let _ = fs::remove_file(&migration);
        return Err(error);
    }
    Ok(migration)
}

fn next_migration_name(directory: &Path) -> io::Result<String> {
    let count = fs::read_dir(directory)?
        .filter_map(Result::ok)
        .filter(|entry| entry.path().extension().is_some_and(|value| value == "sql"))
        .count();
    Ok(format!("{:04}_appstruct.sql", count + 1))
}

fn render_plan(plan: &MigrationPlan) {
    if plan.is_empty() {
        println!("Migration plan: no changes");
        return;
    }
    println!("Migration plan ({} changes):", plan.changes.len());
    for planned in &plan.changes {
        println!(
            "- {} [{:?}/{:?}]",
            change_label(&planned.change),
            planned.risk.schema,
            planned.risk.execution
        );
    }
}

fn change_label(change: &SchemaChange) -> String {
    match change {
        SchemaChange::AddTable { table } => format!("add table `{}`", table.name),
        SchemaChange::RemoveTable { table } => format!("remove table `{}`", table.name),
        SchemaChange::RenameTable { before, after } => {
            format!("rename table `{}` to `{}`", before.name, after.name)
        }
        SchemaChange::AddColumn { table, column } => {
            format!("add column `{table}.{}`", column.name)
        }
        SchemaChange::RemoveColumn { table, column } => {
            format!("remove column `{table}.{}`", column.name)
        }
        SchemaChange::AlterColumn { table, after, .. } => {
            format!("alter column `{table}.{}`", after.name)
        }
        SchemaChange::AddForeignKey { foreign_key } => {
            format!("add foreign key `{}`", foreign_key.id)
        }
        SchemaChange::RemoveForeignKey { foreign_key } => {
            format!("remove foreign key `{}`", foreign_key.id)
        }
    }
}

fn render_diagnostics(diagnostics: &[Diagnostic]) {
    for diagnostic in diagnostics {
        eprintln!("error[{}]: {}", diagnostic.code, diagnostic.message);
    }
}
