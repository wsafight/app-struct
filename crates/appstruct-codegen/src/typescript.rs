mod bulk;
mod entity;
mod modules;
use crate::{Artifact, ArtifactKind, generated_header};
use appstruct_ir::{AppIr, EntityIr, FieldIr, FieldTypeIr, OperationTypeIr, ValueObjectIr};
use modules::{audit_source, tenant_source, tenant_storage_source};
pub(crate) fn plan(ir: &AppIr) -> Vec<Artifact> {
    vec![Artifact::text(
        "web/src/generated/client.ts",
        client_source(ir),
        ArtifactKind::TypeScript,
    )]
}

fn client_source(ir: &AppIr) -> String {
    let mut sections = vec![
        generated_header("//"),
        runtime_source(),
        tenant_storage_source().to_owned(),
    ];
    if ir.auth.enabled {
        sections.push(auth_source(ir));
    }
    if ir.tenant.enabled {
        sections.push(tenant_source());
    }
    if ir.audit.enabled {
        sections.push(audit_source());
    }
    sections.extend(ir.value_objects.iter().map(value_object_type));
    for entity in &ir.entities {
        sections.push(entity_types(entity));
        sections.push(entity::client(entity));
    }
    sections.extend(operation_clients(ir));
    format!("{}\n", sections.join("\n"))
}
fn value_object_type(value: &ValueObjectIr) -> String {
    let fields = value
        .fields
        .iter()
        .map(|field| {
            let optional = if field.required { "" } else { "?" };
            let nullable = if field.required { "" } else { " | null" };
            format!(
                "  {}{optional}: {}{nullable};",
                field.rust_name,
                base_type(&field.ty)
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    format!("export interface {} {{\n{fields}\n}}\n", value.rust_name)
}
fn operation_clients(ir: &AppIr) -> Vec<String> {
    let mut clients = Vec::new();
    for command in &ir.commands {
        let variable = format!("{}Command", lower_camel(&command.rust_name));
        let input = operation_type_name(ir, &command.input);
        let output = operation_type_name(ir, &command.output);
        let path = format!("/api/commands/{}", kebab_name(&command.rust_name));
        clients.push(format!(
            "export const {variable} = (input: {input}) => request<{output}>(\"{path}\", {{ method: \"POST\", body: JSON.stringify(input) }});\n"
        ));
    }
    for query in &ir.queries {
        let variable = format!("{}Query", lower_camel(&query.rust_name));
        let output = operation_type_name(ir, &query.output);
        let path = format!("/api/queries/{}", kebab_name(&query.rust_name));
        clients.push(query.input.as_ref().map_or_else(
            || format!("export const {variable} = () => request<{output}>(\"{path}\");\n"),
            |input| {
                let input = operation_type_name(ir, input);
                format!(
                    "export const {variable} = (input: {input}) => request<{output}>(\"{path}\", {{ method: \"POST\", body: JSON.stringify(input) }});\n"
                )
            },
        ));
    }
    clients
}
fn operation_type_name<'ir>(ir: &'ir AppIr, operation_type: &OperationTypeIr) -> &'ir str {
    match operation_type {
        OperationTypeIr::Entity { entity } => ir
            .entities
            .iter()
            .find(|candidate| candidate.id == *entity)
            .map(|entity| entity.rust_name.as_str())
            .expect("compiler resolved operation entity"),
        OperationTypeIr::ValueObject { value_object } => ir
            .value_objects
            .iter()
            .find(|candidate| candidate.id == *value_object)
            .map(|value| value.rust_name.as_str())
            .expect("compiler resolved operation value object"),
    }
}
fn runtime_source() -> String {
    format!(
        "{}\n{}\n{}",
        request_runtime_source(),
        bulk::runtime_source(),
        list_runtime_source()
    )
}
fn request_runtime_source() -> &'static str {
    r#"const API_BASE = (import.meta.env.VITE_API_URL as string | undefined) ?? "http://127.0.0.1:3000";

export interface FieldViolation {
  field: string;
  message: string;
}

interface ErrorEnvelope {
  error: {
    code: string;
    message: string;
    fields: FieldViolation[];
  };
}

export class ApiError extends Error {
  constructor(
    public readonly status: number,
    public readonly code: string,
    message: string,
    public readonly fields: FieldViolation[] = [],
  ) {
    super(message);
  }
}

const resourceEtags = new Map<string, string>();

async function request<T>(path: string, init?: RequestInit, revisionKey?: string): Promise<T> {
  const headers = new Headers(init?.headers);
  headers.set("Content-Type", "application/json");
  const method = init?.method ?? "GET";
  const tenant = currentTenant();
  if (tenant) headers.set("X-AppStruct-Tenant", tenant);
  if (method !== "GET" && method !== "HEAD") {
    const csrf = cookieValue("appstruct_csrf");
    if (csrf) headers.set("X-CSRF-Token", csrf);
  }
  if (init?.method === "PATCH" || init?.method === "DELETE") {
    const etag = resourceEtags.get(path);
    if (etag) headers.set("If-Match", etag);
  }
  const response = await fetch(`${API_BASE}${path}`, {
    ...init,
    headers,
    credentials: "include",
  });
  if (!response.ok) {
    const body = (await response.json().catch(() => null)) as ErrorEnvelope | null;
    throw new ApiError(
      response.status,
      body?.error.code ?? "HTTP_ERROR",
      body?.error.message ?? `Request failed with status ${response.status}`,
      body?.error.fields,
    );
  }
  const etag = response.headers.get("ETag");
  if (etag && revisionKey) resourceEtags.set(revisionKey, etag);
  if (init?.method === "DELETE") resourceEtags.delete(path);
  if (response.status === 204) return undefined as T;
  return (await response.json()) as T;
}

function cookieValue(name: string): string | undefined {
  return document.cookie
    .split(";")
    .map((part) => part.trim().split("="))
    .find(([candidate]) => candidate === name)?.[1];
}
"#
}

fn list_runtime_source() -> &'static str {
    r#"interface FilterQuery {
  q?: string;
  filters?: Record<string, string>;
  range_filters?: Record<string, { gte?: string; lte?: string }>;
}

export interface ListQuery extends FilterQuery {
  page?: number;
  page_size?: number;
  sort?: string;
}

export interface CursorListQuery extends FilterQuery {
  cursor?: string;
  limit?: number;
}

export interface AggregateQuery extends FilterQuery {
  metrics?: string[];
  group_by?: string[];
  limit?: number;
}

export interface ListResponse<T> {
  data: T[];
  meta: { page: number; page_size: number; total: number };
}

export interface CursorListResponse<T> {
  data: T[];
  meta: { limit: number; next_cursor: string | null; has_more: boolean };
}

export interface AggregateRow {
  [key: string]: unknown;
}

export interface AggregateResponse {
  data: AggregateRow[];
  meta: { metrics: string[]; group_by: string[]; limit: number };
}

function listPath(path: string, query: ListQuery | CursorListQuery): string {
  const params = new URLSearchParams();
  if ("page" in query && query.page) params.set("page", String(query.page));
  if ("page_size" in query && query.page_size) params.set("page_size", String(query.page_size));
  if ("sort" in query && query.sort) params.set("sort", query.sort);
  if ("cursor" in query && query.cursor) params.set("cursor", query.cursor);
  if ("limit" in query && query.limit) params.set("limit", String(query.limit));
  appendFilterParams(params, query);
  const search = params.toString();
  return search ? `${path}?${search}` : path;
}

function aggregatePath(path: string, query: AggregateQuery): string {
  const params = new URLSearchParams();
  if (query.metrics?.length) params.set("metrics", query.metrics.join(","));
  if (query.group_by?.length) params.set("group_by", query.group_by.join(","));
  if (query.limit) params.set("limit", String(query.limit));
  appendFilterParams(params, query);
  const search = params.toString();
  return search ? `${path}?${search}` : path;
}

function appendFilterParams(params: URLSearchParams, query: FilterQuery): void {
  if (query.q) params.set("q", query.q);
  for (const [key, value] of Object.entries(query.filters ?? {})) {
    if (value !== "") params.set(`filter[${key}]`, value);
  }
  for (const [key, range] of Object.entries(query.range_filters ?? {})) {
    if (range.gte) params.set(`filter[${key}][gte]`, range.gte);
    if (range.lte) params.set(`filter[${key}][lte]`, range.lte);
  }
}
"#
}

fn auth_source(ir: &AppIr) -> String {
    let registration = ir.auth.registration_enabled;
    let password_reset = ir.auth.password_reset_enabled;
    let oauth = ir.auth.oauth_enabled;
    format!(
        r#"export interface AuthUser {{
  id: string;
  email: string;
  roles: string[];
}}

interface AuthResponse {{ user: AuthUser; email_verified: boolean; }}

export const authFeatures = {{ registration: {registration}, passwordReset: {password_reset}, emailVerification: true, oauth: {oauth} }} as const;

export const authApi = {{
  me: async () => (await request<AuthResponse>("/api/auth/me")).user,
  login: async (email: string, password: string) =>
    (await request<AuthResponse>("/api/auth/login", {{ method: "POST", body: JSON.stringify({{ email, password }}) }})).user,
  register: async (email: string, password: string) =>
    (await request<AuthResponse>("/api/auth/register", {{ method: "POST", body: JSON.stringify({{ email, password }}) }})).user,
  logout: async () => {{
    await request<void>("/api/auth/logout", {{ method: "POST" }});
    resourceEtags.clear();
    selectTenant();
  }},
  requestPasswordReset: (email: string) =>
    request<void>("/api/auth/password/request", {{ method: "POST", body: JSON.stringify({{ email }}) }}),
  resetPassword: (token: string, password: string) =>
    request<void>("/api/auth/password/reset", {{ method: "POST", body: JSON.stringify({{ token, password }}) }}),
  requestEmailVerification: () => request<void>("/api/auth/email/request", {{ method: "POST" }}),
  verifyEmail: (token: string) => request<void>("/api/auth/email/verify", {{ method: "POST", body: JSON.stringify({{ token }}) }}),
  startOidc: () => {{ window.location.assign(`${{API_BASE}}/api/auth/oauth/oidc/start`); }},
}};
"#
    )
}

fn entity_types(entity: &EntityIr) -> String {
    let model_fields = entity
        .fields
        .iter()
        .map(|field| {
            let marker = if field.read_access.is_some() { "?" } else { "" };
            format!("  {}{marker}: {};", field.rust_name, model_type(field))
        })
        .collect::<Vec<_>>()
        .join("\n");
    let create_fields = entity
        .fields
        .iter()
        .filter(|field| field.generated.is_none())
        .map(|field| input_property(field, false))
        .collect::<Vec<_>>()
        .join("\n");
    let update_fields = entity
        .fields
        .iter()
        .filter(|field| !field.primary_key && field.generated.is_none())
        .map(|field| input_property(field, true))
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "export interface {} {{\n{model_fields}\n}}\n\nexport interface Create{}Input {{\n{create_fields}\n}}\n\nexport interface Update{}Input {{\n{update_fields}\n}}\n",
        entity.rust_name, entity.rust_name, entity.rust_name
    )
}

fn input_property(field: &FieldIr, update: bool) -> String {
    let optional =
        update || field.nullable || field.default.is_some() || field.write_access.is_some();
    let marker = if optional { "?" } else { "" };
    let nullable = if field.nullable { " | null" } else { "" };
    format!(
        "  {}{marker}: {}{nullable};",
        field.rust_name,
        base_type(&field.ty)
    )
}

fn model_type(field: &FieldIr) -> String {
    let nullable = if field.nullable { " | null" } else { "" };
    format!("{}{nullable}", base_type(&field.ty))
}

fn base_type(field_type: &FieldTypeIr) -> &'static str {
    match field_type {
        FieldTypeIr::Integer | FieldTypeIr::Bigint => "number",
        FieldTypeIr::Boolean => "boolean",
        FieldTypeIr::Json => "unknown",
        FieldTypeIr::Uuid
        | FieldTypeIr::String
        | FieldTypeIr::Text
        | FieldTypeIr::Decimal
        | FieldTypeIr::Date
        | FieldTypeIr::Datetime
        | FieldTypeIr::Enum { .. }
        | FieldTypeIr::Relation { .. } => "string",
    }
}

fn lower_camel(value: &str) -> String {
    let mut characters = value.chars();
    characters.next().map_or_else(String::new, |first| {
        first.to_lowercase().chain(characters).collect()
    })
}

fn kebab_name(value: &str) -> String {
    let mut output = String::new();
    for (index, character) in value.chars().enumerate() {
        if character.is_ascii_uppercase() {
            if index > 0 {
                output.push('-');
            }
            output.push(character.to_ascii_lowercase());
        } else {
            output.push(character);
        }
    }
    output
}
