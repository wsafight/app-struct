use super::{error_response, response, schema_ref};
use appstruct_ir::AppIr;
use serde_json::{Map, Value, json};

pub(super) fn add(ir: &AppIr, paths: &mut Map<String, Value>, schemas: &mut Map<String, Value>) {
    schemas.insert("ReportTemplate".to_owned(), template_schema());
    schemas.insert("ReportRun".to_owned(), run_schema());
    schemas.insert("CreateReportRunInput".to_owned(), create_schema(ir));
    schemas.insert("ReportRunList".to_owned(), list_schema());
    paths.insert("/api/reports/templates".to_owned(), templates_path());
    paths.insert(
        "/api/reports/templates/{name}/runs".to_owned(),
        create_path(),
    );
    paths.insert("/api/reports/runs".to_owned(), runs_path());
    paths.insert("/api/reports/runs/{id}".to_owned(), run_path());
    paths.insert("/api/reports/runs/{id}/cancel".to_owned(), cancel_path());
    paths.insert(
        "/api/reports/runs/{id}/download".to_owned(),
        download_path(),
    );
}

fn security() -> Value {
    json!([{ "cookieSession": [] }, { "bearerToken": [] }])
}
fn mutation_security() -> Value {
    json!([{ "cookieSession": [] }])
}
fn id_parameter() -> Value {
    json!({ "name": "id", "in": "path", "required": true, "schema": { "type": "string", "format": "uuid" } })
}

fn templates_path() -> Value {
    json!({ "get": {
        "operationId": "listReportTemplates", "tags": ["Reports"], "security": security(),
        "responses": {
            "200": response("Registered report templates", &json!({ "type": "array", "items": schema_ref("ReportTemplate") })),
            "401": error_response()
        }
    }})
}

fn create_path() -> Value {
    json!({ "post": {
        "operationId": "createReportRun", "tags": ["Reports"], "security": mutation_security(),
        "parameters": [
            { "name": "name", "in": "path", "required": true, "schema": { "type": "string", "pattern": "^[a-z0-9_-]{1,80}$" } },
            { "name": "Idempotency-Key", "in": "header", "required": true, "schema": { "type": "string", "minLength": 1, "maxLength": 200 } }
        ],
        "requestBody": { "required": true, "content": { "application/json": { "schema": schema_ref("CreateReportRunInput") } } },
        "responses": {
            "200": response("Existing idempotent report run", &schema_ref("ReportRun")),
            "202": response("Report run accepted", &schema_ref("ReportRun")),
            "400": error_response(), "401": error_response(), "404": error_response(),
            "409": error_response(), "422": error_response()
        }
    }})
}

fn runs_path() -> Value {
    json!({ "get": {
        "operationId": "listReportRuns", "tags": ["Reports"], "security": security(),
        "parameters": [
            { "name": "page", "in": "query", "schema": { "type": "integer", "minimum": 1, "maximum": 10000 } },
            { "name": "page_size", "in": "query", "schema": { "type": "integer", "minimum": 1, "maximum": 100 } }
        ],
        "responses": {
            "200": response("Visible report runs", &schema_ref("ReportRunList")),
            "400": error_response(), "401": error_response()
        }
    }})
}

fn run_path() -> Value {
    json!({ "get": {
        "operationId": "getReportRun", "tags": ["Reports"], "security": security(),
        "parameters": [id_parameter()],
        "responses": {
            "200": response("Report run", &schema_ref("ReportRun")),
            "400": error_response(), "401": error_response(), "403": error_response(), "404": error_response()
        }
    }})
}

fn cancel_path() -> Value {
    json!({ "post": {
        "operationId": "cancelReportRun", "tags": ["Reports"], "security": mutation_security(),
        "parameters": [id_parameter()],
        "responses": {
            "200": response("Cancelled report run", &schema_ref("ReportRun")),
            "400": error_response(), "401": error_response(), "403": error_response(),
            "404": error_response(), "409": error_response()
        }
    }})
}

fn download_path() -> Value {
    json!({ "get": {
        "operationId": "downloadReportRun", "tags": ["Reports"], "security": security(),
        "parameters": [id_parameter()],
        "responses": {
            "200": { "description": "Generated PDF", "content": { "application/pdf": { "schema": { "type": "string", "contentEncoding": "binary" } } } },
            "400": error_response(), "401": error_response(), "403": error_response(),
            "404": error_response(), "409": error_response()
        }
    }})
}

fn template_schema() -> Value {
    json!({
        "type": "object",
        "required": ["name", "version", "document_type", "artifact_digest", "input_schema", "data_schema_version", "renderer_version"],
        "properties": {
            "name": { "type": "string" }, "version": { "type": "integer", "minimum": 1 },
            "document_type": { "const": "pdf" }, "artifact_digest": { "type": "string", "pattern": "^sha256:[0-9a-f]{64}$" },
            "input_schema": { "type": "object" }, "data_schema_version": { "type": "integer", "minimum": 1 },
            "renderer_version": { "const": "capture-v1" }
        }
    })
}

fn create_schema(ir: &AppIr) -> Value {
    let variants = ir
        .report
        .templates
        .iter()
        .map(|template| {
            let data = serde_json::from_str::<Value>(&template.input_schema)
                .expect("compiler validated report schema");
            json!({
                "title": format!("{} v{}", template.name, template.version),
                "x-appstruct-template": template.name,
                "type": "object", "required": ["data"],
                "properties": {
                    "data": data,
                    "locale": { "enum": ["en-US", "zh-CN"], "default": "en-US" },
                    "timezone": { "enum": ["UTC", "Asia/Shanghai"], "default": "UTC" },
                    "paper": { "enum": ["a4", "letter"], "default": "a4" },
                    "orientation": { "enum": ["portrait", "landscape"], "default": "portrait" }
                },
                "additionalProperties": false
            })
        })
        .collect::<Vec<_>>();
    json!({ "oneOf": variants })
}

fn run_schema() -> Value {
    json!({
        "type": "object",
        "required": ["id", "execution_job_id", "template", "template_version", "tenant_id", "actor_id", "stage", "progress", "locale", "timezone", "paper", "orientation", "result_file_id", "error_code", "created_at", "completed_at", "expires_at"],
        "properties": {
            "id": { "type": "string", "format": "uuid" },
            "execution_job_id": { "type": ["string", "null"], "format": "uuid" },
            "template": { "type": "string" }, "template_version": { "type": "integer" },
            "tenant_id": { "type": ["string", "null"], "format": "uuid" },
            "actor_id": { "type": ["string", "null"], "format": "uuid" },
            "stage": { "enum": ["queued", "rendering", "publishing", "succeeded", "failed", "cancelled"] },
            "progress": { "type": "integer", "minimum": 0, "maximum": 100 },
            "locale": { "enum": ["en-US", "zh-CN"] }, "timezone": { "enum": ["UTC", "Asia/Shanghai"] },
            "paper": { "enum": ["a4", "letter"] }, "orientation": { "enum": ["portrait", "landscape"] },
            "result_file_id": { "type": ["string", "null"], "format": "uuid" },
            "error_code": { "type": ["string", "null"] },
            "created_at": { "type": "string", "format": "date-time" },
            "completed_at": { "type": ["string", "null"], "format": "date-time" },
            "expires_at": { "type": "string", "format": "date-time" }
        }
    })
}

fn list_schema() -> Value {
    json!({
        "type": "object", "required": ["data", "meta"],
        "properties": {
            "data": { "type": "array", "items": schema_ref("ReportRun") },
            "meta": { "type": "object", "required": ["page", "page_size", "total"], "properties": {
                "page": { "type": "integer" }, "page_size": { "type": "integer" }, "total": { "type": "integer" }
            }}
        }
    })
}
