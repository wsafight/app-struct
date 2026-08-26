use crate::{Artifact, ArtifactKind, generated_header};
use appstruct_ir::{AppIr, EntityIr, FieldIr, FieldTypeIr, OperationTypeIr, ValueObjectIr};

pub(crate) fn plan(ir: &AppIr) -> Vec<Artifact> {
    vec![Artifact::text(
        "web/src/generated/client.ts",
        client_source(ir),
        ArtifactKind::TypeScript,
    )]
}

fn client_source(ir: &AppIr) -> String {
    let mut sections = vec![generated_header("//"), runtime_source().to_owned()];
    if ir.auth.enabled {
        sections.push(auth_source(ir));
    }
    sections.extend(ir.value_objects.iter().map(value_object_type));
    for entity in &ir.entities {
        sections.push(entity_types(entity));
        sections.push(entity_client(entity));
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

fn runtime_source() -> &'static str {
    r#"const API_BASE = (import.meta.env.VITE_API_URL as string | undefined) ?? "http://127.0.0.1:3000";

export interface FieldViolation {
  field: string;
  message: string;
}

export interface ListQuery {
  page?: number;
  page_size?: number;
  sort?: string;
  q?: string;
  filters?: Record<string, string>;
  range_filters?: Record<string, { gte?: string; lte?: string }>;
}

export interface ListResponse<T> {
  data: T[];
  meta: { page: number; page_size: number; total: number };
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

function listPath(path: string, query: ListQuery): string {
  const params = new URLSearchParams();
  if (query.page) params.set("page", String(query.page));
  if (query.page_size) params.set("page_size", String(query.page_size));
  if (query.sort) params.set("sort", query.sort);
  if (query.q) params.set("q", query.q);
  for (const [key, value] of Object.entries(query.filters ?? {})) {
    if (value !== "") params.set(`filter[${key}]`, value);
  }
  for (const [key, range] of Object.entries(query.range_filters ?? {})) {
    if (range.gte) params.set(`filter[${key}][gte]`, range.gte);
    if (range.lte) params.set(`filter[${key}][lte]`, range.lte);
  }
  const search = params.toString();
  return search ? `${path}?${search}` : path;
}
"#
}

fn auth_source(ir: &AppIr) -> String {
    let registration = ir.auth.registration_enabled;
    let password_reset = ir.auth.password_reset_enabled;
    format!(
        r#"export interface AuthUser {{
  id: string;
  email: string;
  roles: string[];
}}

interface AuthResponse {{ user: AuthUser; }}

export const authFeatures = {{ registration: {registration}, passwordReset: {password_reset} }} as const;

export const authApi = {{
  me: async () => (await request<AuthResponse>("/api/auth/me")).user,
  login: async (email: string, password: string) =>
    (await request<AuthResponse>("/api/auth/login", {{ method: "POST", body: JSON.stringify({{ email, password }}) }})).user,
  register: async (email: string, password: string) =>
    (await request<AuthResponse>("/api/auth/register", {{ method: "POST", body: JSON.stringify({{ email, password }}) }})).user,
  logout: async () => {{
    await request<void>("/api/auth/logout", {{ method: "POST" }});
    resourceEtags.clear();
  }},
  requestPasswordReset: (email: string) =>
    request<void>("/api/auth/password/request", {{ method: "POST", body: JSON.stringify({{ email }}) }}),
  resetPassword: (token: string, password: string) =>
    request<void>("/api/auth/password/reset", {{ method: "POST", body: JSON.stringify({{ token, password }}) }}),
}};
"#
    )
}

fn entity_types(entity: &EntityIr) -> String {
    let model_fields = entity
        .fields
        .iter()
        .map(|field| format!("  {}: {};", field.rust_name, model_type(field)))
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

fn entity_client(entity: &EntityIr) -> String {
    let variable = lower_camel(&entity.rust_name);
    let model = &entity.rust_name;
    let path = format!("/api/{}/", entity.table_name);
    format!(
        r#"export const {variable}Api = {{
  list: (query: ListQuery = {{}}) => request<ListResponse<{model}>>(listPath("{path}", query)),
  get: (id: string) => {{
    const member = `{path}${{encodeURIComponent(id)}}`;
    return request<{model}>(member, undefined, member);
  }},
  create: (input: Create{model}Input) =>
    request<{model}>("{path}", {{ method: "POST", body: JSON.stringify(input) }}),
  update: (id: string, input: Update{model}Input) => {{
    const member = `{path}${{encodeURIComponent(id)}}`;
    return request<{model}>(member, {{
      method: "PATCH",
      body: JSON.stringify(input),
    }}, member);
  }},
  remove: (id: string) => {{
    const member = `{path}${{encodeURIComponent(id)}}`;
    return request<void>(member, {{ method: "DELETE" }});
  }},
}};
"#
    )
}

fn input_property(field: &FieldIr, update: bool) -> String {
    let optional = update || field.nullable || field.default.is_some();
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
