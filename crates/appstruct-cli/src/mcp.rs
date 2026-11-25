use appstruct_migrate::{
    DatabaseSchema, LintSeverity, SCHEMA_VERSION, diff, extract, from_json, lint_plan,
    migration_sql,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::fs;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

const PROTOCOL_VERSION: &str = "2024-11-05";
const SERVER_NAME: &str = "appstruct";
const AGENT_GUIDE: &str = include_str!("../templates/common/AGENTS.md");

#[derive(Debug, Deserialize)]
struct JsonRpcRequest {
    jsonrpc: Option<String>,
    id: Option<Value>,
    method: String,
    #[serde(default)]
    params: Value,
}

#[derive(Debug, Serialize)]
struct JsonRpcResponse {
    jsonrpc: &'static str,
    id: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<JsonRpcError>,
}

#[derive(Debug, Serialize)]
struct JsonRpcError {
    code: i64,
    message: String,
}

pub(crate) fn run(default_project: Option<&Path>) -> ExitCode {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut reader = stdin.lock();
    let mut writer = stdout.lock();
    match serve_stdio(&mut reader, &mut writer, default_project) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            let _ = writeln!(io::stderr(), "appstruct mcp error: {error}");
            ExitCode::from(1)
        }
    }
}

fn serve_stdio<R: BufRead, W: Write>(
    reader: &mut R,
    writer: &mut W,
    default_project: Option<&Path>,
) -> io::Result<()> {
    let mut line = String::new();
    loop {
        line.clear();
        let bytes = reader.read_line(&mut line)?;
        if bytes == 0 {
            break;
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let response = match serde_json::from_str::<JsonRpcRequest>(trimmed) {
            Ok(request) => handle_request(&request, default_project),
            Err(error) => Some(error_response(
                Value::Null,
                -32700,
                format!("parse error: {error}"),
            )),
        };
        if let Some(response) = response {
            serde_json::to_writer(&mut *writer, &response)?;
            writer.write_all(b"\n")?;
            writer.flush()?;
        }
    }
    Ok(())
}

fn handle_request(
    request: &JsonRpcRequest,
    default_project: Option<&Path>,
) -> Option<JsonRpcResponse> {
    // JSON-RPC notifications omit `id` and must not receive a response.
    let id = request.id.clone()?;
    if let Some(version) = request.jsonrpc.as_deref()
        && version != "2.0"
    {
        return Some(error_response(
            id,
            -32600,
            format!("unsupported jsonrpc version: {version}"),
        ));
    }
    let response = match request.method.as_str() {
        "initialize" => ok_response(
            id,
            json!({
                "protocolVersion": PROTOCOL_VERSION,
                "capabilities": {
                    "tools": { "listChanged": false },
                    "resources": { "subscribe": false, "listChanged": false }
                },
                "serverInfo": {
                    "name": SERVER_NAME,
                    "version": env!("CARGO_PKG_VERSION")
                }
            }),
        ),
        "ping" => ok_response(id, json!({})),
        "tools/list" => ok_response(id, json!({ "tools": tool_definitions() })),
        "tools/call" => handle_tool_call(id, &request.params, default_project),
        "resources/list" => ok_response(id, json!({ "resources": resource_definitions() })),
        "resources/read" => handle_resource_read(id, &request.params),
        method => error_response(id, -32601, format!("method not found: {method}")),
    };
    Some(response)
}

fn ok_response(id: Value, result: Value) -> JsonRpcResponse {
    JsonRpcResponse {
        jsonrpc: "2.0",
        id,
        result: Some(result),
        error: None,
    }
}

fn error_response(id: Value, code: i64, message: String) -> JsonRpcResponse {
    JsonRpcResponse {
        jsonrpc: "2.0",
        id,
        result: None,
        error: Some(JsonRpcError { code, message }),
    }
}

fn resource_definitions() -> Value {
    json!([
        {
            "uri": "appstruct://schema/app-spec",
            "name": "AppStruct App Spec JSON Schema",
            "description": "Draft 2020-12 JSON Schema for appstruct.yaml and spec/*.yaml.",
            "mimeType": "application/json"
        },
        {
            "uri": "appstruct://guide/agent",
            "name": "AppStruct AI Agent Guide",
            "description": "Ownership boundaries, verification loop, and security invariants for AI coding agents.",
            "mimeType": "text/markdown"
        }
    ])
}

fn handle_resource_read(id: Value, params: &Value) -> JsonRpcResponse {
    let Some(uri) = params.get("uri").and_then(Value::as_str) else {
        return error_response(id, -32602, "missing required parameter: uri".to_owned());
    };
    match uri {
        "appstruct://schema/app-spec" => ok_response(
            id,
            json!({
                "contents": [{
                    "uri": uri,
                    "mimeType": "application/json",
                    "text": appstruct_compiler::APP_SPEC_SCHEMA
                }]
            }),
        ),
        "appstruct://guide/agent" => ok_response(
            id,
            json!({
                "contents": [{
                    "uri": uri,
                    "mimeType": "text/markdown",
                    "text": AGENT_GUIDE
                }]
            }),
        ),
        other => error_response(id, -32602, format!("unknown resource uri: {other}")),
    }
}

fn tool_definitions() -> Value {
    json!([
        {
            "name": "appstruct_schema",
            "description": "Return the Draft 2020-12 JSON Schema for AppStruct YAML specs. Pass an optional `section` (a top-level property or $defs key) to retrieve a focused slice.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "section": {
                        "type": "string",
                        "description": "Optional top-level property or $defs entry name to extract instead of the full schema."
                    }
                }
            }
        },
        {
            "name": "appstruct_check",
            "description": "Validate appstruct.yaml and spec/*.yaml, build normalized IR in memory, and verify custom web symbol references. Returns structured diagnostics.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "project": {
                        "type": "string",
                        "description": "Project directory or path inside an AppStruct project. Defaults to server --project or current working directory."
                    },
                    "deny_warnings": {
                        "type": "boolean",
                        "description": "Treat non-fatal warnings as validation failures."
                    }
                }
            }
        },
        {
            "name": "appstruct_access_preview",
            "description": "Evaluate declared CRUD and field-level access rules for a sample actor without running the server or database.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "project": { "type": "string" },
                    "entity": {
                        "type": "string",
                        "description": "Optional entity name or full IR ID (e.g. 'Task' or 'Work::Task')."
                    },
                    "authenticated": { "type": "boolean" },
                    "roles": {
                        "type": "array",
                        "items": { "type": "string" }
                    },
                    "actor_id": { "type": "string" },
                    "tenant_id": { "type": "string" }
                }
            }
        },
        {
            "name": "appstruct_migrate_plan",
            "description": "Compute the forward-only PostgreSQL migration diff, SQL preview, and migration lint warnings against .appstruct/schema.snapshot.json without writing files or connecting to a database.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "project": { "type": "string" },
                    "deny_warnings": { "type": "boolean" }
                }
            }
        },
        {
            "name": "appstruct_db_pull",
            "description": "Inspect a live PostgreSQL schema via DATABASE_URL and generate or diff an AppStruct YAML domain draft.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "project": { "type": "string" },
                    "schema": {
                        "type": "string",
                        "description": "PostgreSQL schema to inspect (default: 'public')."
                    },
                    "output": {
                        "type": "string",
                        "description": "Project-relative output path (default: 'spec/imported.yaml')."
                    },
                    "mode": {
                        "type": "string",
                        "enum": ["create", "check", "diff"],
                        "description": "Whether to write the draft ('create'), check drift ('check'), or return a unified diff ('diff'). Defaults to 'diff'."
                    },
                    "access": {
                        "type": "string",
                        "enum": ["none", "public", "authenticated", "role", "owner"],
                        "description": "Optional access policy preset for imported entities."
                    },
                    "role": { "type": "string" },
                    "owner": { "type": "string" }
                }
            }
        },
        {
            "name": "appstruct_generate",
            "description": "Run deterministic code generation (Rust backend, React web app, OpenAPI, IR) or check whether generated files are up to date.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "project": { "type": "string" },
                    "check": {
                        "type": "boolean",
                        "description": "Verify generated files are current without writing them."
                    }
                }
            }
        }
    ])
}

fn handle_tool_call(id: Value, params: &Value, default_project: Option<&Path>) -> JsonRpcResponse {
    let Some(name) = params.get("name").and_then(Value::as_str) else {
        return error_response(id, -32602, "missing required parameter: name".to_owned());
    };
    let arguments = params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| json!({}));

    let (payload, is_error) = match name {
        "appstruct_schema" => tool_schema(&arguments),
        "appstruct_check" => tool_check(&arguments, default_project),
        "appstruct_access_preview" => tool_access_preview(&arguments, default_project),
        "appstruct_migrate_plan" => tool_migrate_plan(&arguments, default_project),
        "appstruct_db_pull" => tool_db_pull(&arguments, default_project),
        "appstruct_generate" => tool_generate(&arguments, default_project),
        unknown => {
            return error_response(id, -32602, format!("unknown tool: {unknown}"));
        }
    };

    let text = serde_json::to_string_pretty(&payload).unwrap_or_else(|_| payload.to_string());
    ok_response(
        id,
        json!({
            "content": [{ "type": "text", "text": text }],
            "structuredContent": payload,
            "isError": is_error
        }),
    )
}

fn resolve_project(arguments: &Value, default_project: Option<&Path>) -> Result<PathBuf, Value> {
    let start = if let Some(path) = arguments.get("project").and_then(Value::as_str) {
        PathBuf::from(path)
    } else if let Some(path) = default_project {
        path.to_path_buf()
    } else {
        std::env::current_dir().map_err(|error| {
            json!({
                "error": "environment_error",
                "message": format!("cannot read current directory: {error}")
            })
        })?
    };
    appstruct_compiler::discover_project(&start)
        .map_err(|diagnostic| json!({ "error": "project_not_found", "diagnostic": diagnostic }))
}

fn tool_schema(arguments: &Value) -> (Value, bool) {
    let parsed: Value = match serde_json::from_str(appstruct_compiler::APP_SPEC_SCHEMA) {
        Ok(value) => value,
        Err(error) => {
            return (
                json!({ "error": "schema_parse_failed", "message": error.to_string() }),
                true,
            );
        }
    };
    let Some(section) = arguments.get("section").and_then(Value::as_str) else {
        return (parsed, false);
    };
    if let Some(def) = parsed.get("$defs").and_then(|d| d.get(section)) {
        return (
            json!({ "section": section, "kind": "$defs", "schema": def }),
            false,
        );
    }
    if let Some(prop) = parsed
        .pointer("/$defs/root/properties")
        .and_then(|p| p.get(section))
    {
        return (
            json!({ "section": section, "kind": "root.property", "schema": prop }),
            false,
        );
    }
    if let Some(prop) = parsed
        .pointer("/$defs/domain/properties")
        .and_then(|p| p.get(section))
    {
        return (
            json!({ "section": section, "kind": "domain.property", "schema": prop }),
            false,
        );
    }
    if let Some(prop) = parsed.get("properties").and_then(|p| p.get(section)) {
        return (
            json!({ "section": section, "kind": "property", "schema": prop }),
            false,
        );
    }
    let available_root_properties = parsed
        .pointer("/$defs/root/properties")
        .and_then(Value::as_object)
        .map(|map| map.keys().cloned().collect::<Vec<_>>())
        .unwrap_or_default();
    let available_defs = parsed
        .get("$defs")
        .and_then(Value::as_object)
        .map(|map| map.keys().cloned().collect::<Vec<_>>())
        .unwrap_or_default();
    (
        json!({
            "error": "unknown_section",
            "message": format!("section '{section}' not found in App Spec JSON Schema"),
            "available_root_properties": available_root_properties,
            "available_defs": available_defs
        }),
        true,
    )
}

fn tool_check(arguments: &Value, default_project: Option<&Path>) -> (Value, bool) {
    let project = match resolve_project(arguments, default_project) {
        Ok(path) => path,
        Err(error) => return (error, true),
    };
    let deny_warnings = arguments
        .get("deny_warnings")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    match appstruct_compiler::compile_project_report(&project) {
        Ok(report) => {
            let mut diagnostics = report.diagnostics;
            if let Err(error) = appstruct_codegen::check_user_symbols(&project, &report.ir)
                .map(|missing| diagnostics.extend(missing))
            {
                return (
                    json!({
                        "valid": false,
                        "error": "symbol_check_failed",
                        "message": error.to_string()
                    }),
                    true,
                );
            }
            let has_error = diagnostics
                .iter()
                .any(|item| item.severity == appstruct_ir::Severity::Error);
            let valid = !(has_error || (deny_warnings && !diagnostics.is_empty()));
            (
                json!({
                    "valid": valid,
                    "app_name": report.ir.app.name,
                    "entity_count": report.ir.entities.len(),
                    "diagnostics": diagnostics
                }),
                !valid,
            )
        }
        Err(diagnostics) => (
            json!({
                "valid": false,
                "entity_count": 0,
                "diagnostics": diagnostics
            }),
            true,
        ),
    }
}

fn tool_access_preview(arguments: &Value, default_project: Option<&Path>) -> (Value, bool) {
    let project = match resolve_project(arguments, default_project) {
        Ok(path) => path,
        Err(error) => return (error, true),
    };
    let entity = arguments.get("entity").and_then(Value::as_str);
    let authenticated = arguments
        .get("authenticated")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let roles = arguments
        .get("roles")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let actor_id = match parse_optional_uuid(arguments, "actor_id") {
        Ok(id) => id,
        Err(error) => return (error, true),
    };
    let tenant_id = match parse_optional_uuid(arguments, "tenant_id") {
        Ok(id) => id,
        Err(error) => return (error, true),
    };

    match crate::access_preview::preview_value(
        &project,
        entity,
        authenticated,
        &roles,
        actor_id,
        tenant_id,
        None,
    ) {
        Ok(value) => (value, false),
        Err(error) => (error, true),
    }
}

fn parse_optional_uuid(arguments: &Value, field: &str) -> Result<Option<uuid::Uuid>, Value> {
    let Some(raw) = arguments.get(field).and_then(Value::as_str) else {
        return Ok(None);
    };
    uuid::Uuid::parse_str(raw).map(Some).map_err(|error| {
        json!({
            "error": "invalid_uuid",
            "field": field,
            "message": error.to_string()
        })
    })
}

fn tool_migrate_plan(arguments: &Value, default_project: Option<&Path>) -> (Value, bool) {
    let project = match resolve_project(arguments, default_project) {
        Ok(path) => path,
        Err(error) => return (error, true),
    };
    let deny_warnings = arguments
        .get("deny_warnings")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    let ir = match appstruct_compiler::compile_project(&project) {
        Ok(ir) => ir,
        Err(diagnostics) => {
            return (
                json!({ "error": "validation_failed", "diagnostics": diagnostics }),
                true,
            );
        }
    };
    let target = match extract(&ir) {
        Ok(schema) => schema,
        Err(error) => {
            return (
                json!({ "error": "schema_extract_failed", "message": error.to_string() }),
                true,
            );
        }
    };
    let snapshot_path = project.join(".appstruct/schema.snapshot.json");
    let previous = if snapshot_path.is_file() {
        let raw = match fs::read_to_string(&snapshot_path) {
            Ok(raw) => raw,
            Err(error) => {
                return (
                    json!({ "error": "snapshot_read_failed", "message": error.to_string() }),
                    true,
                );
            }
        };
        match from_json(&raw) {
            Ok(schema) => schema,
            Err(error) => {
                return (
                    json!({ "error": "snapshot_parse_failed", "message": error.to_string() }),
                    true,
                );
            }
        }
    } else {
        DatabaseSchema {
            schema_version: SCHEMA_VERSION,
            provider: ir.database.provider,
            allow_unmanaged_tables: false,
            tables: Vec::new(),
            unique_constraints: Vec::new(),
            indexes: Vec::new(),
            seeds: Vec::new(),
            foreign_keys: Vec::new(),
            rls_tables: Vec::new(),
        }
    };

    let plan = diff(&previous, &target);
    let lint = lint_plan(&plan);
    let sql = if plan.is_empty() {
        None
    } else {
        match migration_sql(&plan) {
            Ok(sql) => Some(sql),
            Err(error) => {
                return (
                    json!({ "error": "sql_generation_failed", "message": error }),
                    true,
                );
            }
        }
    };
    let has_lint_error = lint
        .iter()
        .any(|item| item.severity == LintSeverity::Error || (deny_warnings && !lint.is_empty()));
    (
        json!({
            "has_changes": !plan.is_empty(),
            "blocked": plan.is_blocked(),
            "change_count": plan.changes.len(),
            "changes": plan.changes,
            "lint": lint,
            "sql": sql
        }),
        has_lint_error,
    )
}

fn tool_db_pull(arguments: &Value, default_project: Option<&Path>) -> (Value, bool) {
    let project = match resolve_project(arguments, default_project) {
        Ok(path) => path,
        Err(error) => return (error, true),
    };
    let mut args = vec![
        "--project".to_owned(),
        project.display().to_string(),
        "--format".to_owned(),
        "json".to_owned(),
        "db".to_owned(),
        "pull".to_owned(),
    ];
    if let Some(schema) = arguments.get("schema").and_then(Value::as_str) {
        args.push("--schema".to_owned());
        args.push(schema.to_owned());
    }
    if let Some(output) = arguments.get("output").and_then(Value::as_str) {
        args.push("--output".to_owned());
        args.push(output.to_owned());
    }
    match arguments
        .get("mode")
        .and_then(Value::as_str)
        .unwrap_or("diff")
    {
        "create" => {}
        "check" => args.push("--check".to_owned()),
        "diff" => args.push("--diff".to_owned()),
        other => {
            return (
                json!({ "error": "invalid_mode", "message": format!("unsupported mode: {other}") }),
                true,
            );
        }
    }
    if let Some(access) = arguments.get("access").and_then(Value::as_str) {
        args.push("--access".to_owned());
        args.push(access.to_owned());
    }
    if let Some(role) = arguments.get("role").and_then(Value::as_str) {
        args.push("--role".to_owned());
        args.push(role.to_owned());
    }
    if let Some(owner) = arguments.get("owner").and_then(Value::as_str) {
        args.push("--owner".to_owned());
        args.push(owner.to_owned());
    }
    run_self_cli(&args)
}

fn tool_generate(arguments: &Value, default_project: Option<&Path>) -> (Value, bool) {
    let project = match resolve_project(arguments, default_project) {
        Ok(path) => path,
        Err(error) => return (error, true),
    };
    let mut args = vec![
        "--project".to_owned(),
        project.display().to_string(),
        "--format".to_owned(),
        "json".to_owned(),
        "generate".to_owned(),
    ];
    if arguments
        .get("check")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        args.push("--check".to_owned());
    }
    run_self_cli(&args)
}

fn run_self_cli(args: &[String]) -> (Value, bool) {
    let exe = match std::env::current_exe() {
        Ok(path) => path,
        Err(error) => {
            return (
                json!({ "error": "cli_lookup_failed", "message": error.to_string() }),
                true,
            );
        }
    };
    match Command::new(exe).args(args).output() {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout).trim().to_owned();
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
            let parsed = serde_json::from_str::<Value>(&stdout)
                .or_else(|_| serde_json::from_str::<Value>(&stderr))
                .unwrap_or_else(|_| {
                    json!({
                        "stdout": stdout,
                        "stderr": stderr,
                        "exit_code": output.status.code()
                    })
                });
            (parsed, !output.status.success())
        }
        Err(error) => (
            json!({ "error": "cli_exec_failed", "message": error.to_string() }),
            true,
        ),
    }
}

#[cfg(test)]
mod tests;
