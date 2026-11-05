use super::{error_response, response, schema_ref, tenant};
use appstruct_ir::AppIr;
use serde_json::{Map, Value, json};

pub(super) fn add(ir: &AppIr, paths: &mut Map<String, Value>, schemas: &mut Map<String, Value>) {
    staged_uploads(ir, paths, schemas);
    schemas.insert("UserFile".to_owned(), json!({
        "type": "object", "required": ["id", "object_key", "original_name", "content_type", "size", "checksum", "tenant_id", "created_at"],
        "properties": { "id": { "type": "string", "format": "uuid" }, "object_key": { "type": "string" }, "original_name": { "type": "string" }, "content_type": { "type": "string" }, "size": { "type": "integer", "minimum": 0 }, "checksum": { "type": "string" }, "tenant_id": { "type": ["string", "null"], "format": "uuid" }, "created_at": { "type": "string", "format": "date-time" } }
    }));
    schemas.insert("UserFileList".to_owned(), json!({
        "type": "object", "required": ["data", "meta"], "properties": { "data": { "type": "array", "items": schema_ref("UserFile") }, "meta": { "type": "object", "required": ["page", "page_size", "total"], "properties": { "page": { "type": "integer" }, "page_size": { "type": "integer" }, "total": { "type": "integer" } } } }
    }));
    let mut parameters = Vec::new();
    if ir.tenant.enabled {
        parameters.push(tenant::parameter());
    }
    let mut upload_parameters = parameters.clone();
    let upload_content = ir
        .file
        .allowed_content_types
        .iter()
        .map(|content_type| {
            (
                content_type.clone(),
                json!({ "schema": { "type": "string", "format": "binary" } }),
            )
        })
        .collect::<Map<_, _>>();
    upload_parameters.push(
        json!({ "name": "name", "in": "query", "required": true, "schema": { "type": "string" } }),
    );
    let csrf = json!({ "name": "X-CSRF-Token", "in": "header", "required": true, "schema": { "type": "string" } });
    upload_parameters.push(csrf.clone());
    let mut list_parameters = parameters.clone();
    list_parameters.extend([
        json!({ "name": "page", "in": "query", "schema": { "type": "integer", "minimum": 1, "maximum": 10000, "default": 1 } }),
        json!({ "name": "page_size", "in": "query", "schema": { "type": "integer", "minimum": 1, "maximum": 100, "default": 25 } }),
    ]);
    paths.insert("/api/files".to_owned(), json!({
        "get": { "operationId": "listUserFiles", "tags": ["Files"], "security": [{ "cookieSession": [] }], "parameters": list_parameters, "responses": { "200": response("Uploaded files", &schema_ref("UserFileList")), "400": error_response(), "401": error_response(), "403": error_response() } },
        "post": { "operationId": "uploadUserFile", "tags": ["Files"], "security": [{ "cookieSession": [] }], "parameters": upload_parameters, "requestBody": { "required": true, "content": upload_content }, "responses": { "201": response("Uploaded file", &schema_ref("UserFile")), "400": error_response(), "401": error_response(), "403": error_response(), "413": error_response() } }
    }));
    parameters.push(json!({ "name": "id", "in": "path", "required": true, "schema": { "type": "string", "format": "uuid" } }));
    paths.insert("/api/files/{id}/download".to_owned(), json!({ "get": { "operationId": "downloadUserFile", "tags": ["Files"], "security": [{ "cookieSession": [] }], "parameters": parameters, "responses": { "200": { "description": "File content", "content": { "application/octet-stream": { "schema": { "type": "string", "format": "binary" } } } }, "401": error_response(), "404": error_response() } } }));
    parameters.push(csrf);
    paths.insert("/api/files/{id}".to_owned(), json!({ "delete": { "operationId": "deleteUserFile", "tags": ["Files"], "security": [{ "cookieSession": [] }], "parameters": parameters, "responses": { "204": { "description": "File deleted" }, "401": error_response(), "404": error_response(), "409": error_response() } } }));
}

fn staged_uploads(ir: &AppIr, paths: &mut Map<String, Value>, schemas: &mut Map<String, Value>) {
    schemas.insert("FileUploadRequest".to_owned(), json!({
        "type": "object", "additionalProperties": false, "required": ["name", "content_type", "size"],
        "properties": { "name": { "type": "string" }, "content_type": { "type": "string" }, "size": { "type": "integer", "minimum": 0, "maximum": ir.file.max_bytes } }
    }));
    schemas.insert("FileUploadSession".to_owned(), json!({
        "type": "object", "required": ["id", "upload_url", "direct", "expires_at"],
        "properties": { "id": { "type": "string", "format": "uuid" }, "upload_url": { "type": "string" }, "direct": { "type": "boolean" }, "expires_at": { "type": "string", "format": "date-time" } }
    }));
    schemas.insert("FileUploadStatus".to_owned(), json!({
        "type": "object", "required": ["id", "file_id", "expires_at", "cancelled"],
        "properties": { "id": { "type": "string", "format": "uuid" }, "file_id": { "type": ["string", "null"], "format": "uuid" }, "expires_at": { "type": "string", "format": "date-time" }, "cancelled": { "type": "boolean" } }
    }));
    schemas.insert("RecordAttachmentList".to_owned(), json!({
        "type": "object", "required": ["data"], "properties": { "data": { "type": "array", "maxItems": 100, "items": { "type": "object", "required": ["id", "original_name", "content_type", "size"], "properties": { "id": { "type": "string", "format": "uuid" }, "original_name": { "type": "string" }, "content_type": { "type": "string" }, "size": { "type": "integer" } } } } }
    }));
    let mut scope = Vec::new();
    if ir.tenant.enabled {
        scope.push(tenant::parameter());
    }
    let csrf = json!({ "name": "X-CSRF-Token", "in": "header", "required": true, "schema": { "type": "string" } });
    let mut mutation = scope.clone();
    mutation.push(csrf.clone());
    let operation = |id: &str, parameters: Vec<Value>, success: &str, schema: Value| {
        json!({
            "operationId": id, "tags": ["Files"], "security": [{ "cookieSession": [] }], "parameters": parameters,
            "responses": { success: if schema.is_null() { json!({ "description": "Success" }) } else { response("Success", &schema) }, "400": error_response(), "401": error_response(), "403": error_response(), "404": error_response(), "409": error_response(), "413": error_response() }
        })
    };
    let mut prepare = operation(
        "prepareFileUpload",
        mutation.clone(),
        "201",
        schema_ref("FileUploadSession"),
    );
    prepare["description"] = json!(
        "Creates an actor/tenant-scoped 15-minute staging session. PUT bytes to upload_url, then complete. A direct S3 PUT must not receive application cookies, tenant or CSRF headers. Completion validates size and MIME and publishes an immutable copy; sessions are limited to 20 pending per actor."
    );
    prepare["requestBody"] = json!({ "required": true, "content": { "application/json": { "schema": schema_ref("FileUploadRequest") } } });
    paths.insert("/api/files/uploads".to_owned(), json!({ "post": prepare }));
    scope.push(json!({ "name": "id", "in": "path", "required": true, "schema": { "type": "string", "format": "uuid" } }));
    mutation.clone_from(&scope);
    mutation.push(csrf.clone());
    paths.insert(
        "/api/files/uploads/{id}".to_owned(),
        json!({
            "get": operation("fileUploadStatus", scope, "200", schema_ref("FileUploadStatus")),
            "delete": operation("cancelFileUpload", mutation.clone(), "204", Value::Null)
        }),
    );
    let mut content = operation("putStagedFileContent", mutation.clone(), "204", Value::Null);
    content["requestBody"] = json!({ "required": true, "content": { "application/octet-stream": { "schema": { "type": "string", "format": "binary" } } } });
    paths.insert(
        "/api/files/uploads/{id}/content".to_owned(),
        json!({ "put": content }),
    );
    paths.insert(
        "/api/files/uploads/{id}/complete".to_owned(),
        json!({ "post": operation("completeFileUpload", mutation, "200", schema_ref("UserFile")) }),
    );
    let mut record = Vec::new();
    if ir.tenant.enabled {
        record.push(tenant::parameter());
    }
    record.extend([
        json!({ "name": "resource", "in": "path", "required": true, "schema": { "type": "string", "enum": ir.entities.iter().map(|entity| &entity.table_name).collect::<Vec<_>>() } }),
        json!({ "name": "record_id", "in": "path", "required": true, "schema": { "type": "string" } }),
    ]);
    let mut listing = operation(
        "listRecordAttachments",
        record.clone(),
        "200",
        schema_ref("RecordAttachmentList"),
    );
    listing["description"] = json!(
        "Requires record read access and policy. Attachments remain files in the existing organization/personal library scope; binding does not narrow library download visibility. Resource is the generated table name; typed record IDs are canonicalized."
    );
    paths.insert(
        "/api/files/records/{resource}/{record_id}".to_owned(),
        json!({ "get": listing }),
    );
    record.push(json!({ "name": "file_id", "in": "path", "required": true, "schema": { "type": "string", "format": "uuid" } }));
    record.push(csrf);
    paths.insert(
        "/api/files/records/{resource}/{record_id}/{file_id}".to_owned(),
        json!({
            "put": operation("attachRecordFile", record.clone(), "204", Value::Null),
            "delete": operation("detachRecordFile", record.clone(), "204", Value::Null)
        }),
    );
    let mut replacement = operation("replaceRecordAttachment", record, "204", Value::Null);
    replacement["requestBody"] = json!({ "required": true, "content": { "application/json": { "schema": { "type": "object", "additionalProperties": false, "required": ["replacement_file_id"], "properties": { "replacement_file_id": { "type": "string", "format": "uuid" } } } } } });
    paths.insert(
        "/api/files/records/{resource}/{record_id}/{file_id}/replace".to_owned(),
        json!({ "put": replacement }),
    );
}
