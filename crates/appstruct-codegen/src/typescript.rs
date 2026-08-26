use crate::{Artifact, ArtifactKind, generated_header};
use appstruct_ir::{AppIr, EntityIr, FieldIr, FieldTypeIr};

pub(crate) fn plan(ir: &AppIr) -> Vec<Artifact> {
    vec![Artifact::text(
        "web/src/generated/client.ts",
        client_source(ir),
        ArtifactKind::TypeScript,
    )]
}

fn client_source(ir: &AppIr) -> String {
    let mut sections = vec![generated_header("//"), runtime_source().to_owned()];
    for entity in &ir.entities {
        sections.push(entity_types(entity));
        sections.push(entity_client(entity));
    }
    format!("{}\n", sections.join("\n"))
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

async function request<T>(path: string, init?: RequestInit): Promise<T> {
  const response = await fetch(`${API_BASE}${path}`, {
    ...init,
    headers: { "Content-Type": "application/json", ...init?.headers },
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
  if (response.status === 204) return undefined as T;
  return (await response.json()) as T;
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
  get: (id: string) => request<{model}>(`{path}${{encodeURIComponent(id)}}`),
  create: (input: Create{model}Input) =>
    request<{model}>("{path}", {{ method: "POST", body: JSON.stringify(input) }}),
  update: (id: string, input: Update{model}Input) =>
    request<{model}>(`{path}${{encodeURIComponent(id)}}`, {{
      method: "PATCH",
      body: JSON.stringify(input),
    }}),
  remove: (id: string) =>
    request<void>(`{path}${{encodeURIComponent(id)}}`, {{ method: "DELETE" }}),
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
