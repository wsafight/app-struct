use appstruct_ir::AppIr;
use serde_json::Value;

#[allow(clippy::too_many_lines)]
pub(super) fn source(ir: &AppIr) -> String {
    let names = ir
        .report
        .templates
        .iter()
        .map(|template| format!("{:?}", template.name))
        .collect::<Vec<_>>()
        .join(" | ");
    let input_map = ir
        .report
        .templates
        .iter()
        .map(|template| {
            let schema: Value = serde_json::from_str(&template.input_schema)
                .expect("compiler validated report schema");
            format!("  {:?}: {};", template.name, schema_type(&schema))
        })
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        r#"
export type ReportTemplateName = {names};
export type ReportRunStage = "queued" | "rendering" | "publishing" | "succeeded" | "failed" | "cancelled";
export type ReportLocale = "en-US" | "zh-CN";
export type ReportTimezone = "UTC" | "Asia/Shanghai";
export type ReportPaper = "a4" | "letter";
export type ReportOrientation = "portrait" | "landscape";

export interface ReportInputMap {{
{input_map}
}}

export interface ReportTemplate {{
  name: ReportTemplateName;
  version: number;
  document_type: "pdf";
  artifact_digest: string;
  input_schema: Record<string, unknown>;
  data_schema_version: number;
  renderer_version: "capture-v1";
}}

export interface ReportRun {{
  id: string;
  execution_job_id: string | null;
  template: ReportTemplateName;
  template_version: number;
  tenant_id: string | null;
  actor_id: string | null;
  stage: ReportRunStage;
  progress: number;
  locale: ReportLocale;
  timezone: ReportTimezone;
  paper: ReportPaper;
  orientation: ReportOrientation;
  result_file_id: string | null;
  error_code: string | null;
  created_at: string;
  completed_at: string | null;
  expires_at: string;
}}

export interface CreateReportOptions {{
  locale?: ReportLocale;
  timezone?: ReportTimezone;
  paper?: ReportPaper;
  orientation?: ReportOrientation;
}}

export interface ReportRunList {{
  data: ReportRun[];
  meta: {{ page: number; page_size: number; total: number }};
}}

export const reportApi = {{
  templates: () => request<ReportTemplate[]>("/api/reports/templates"),
  create: <T extends ReportTemplateName>(
    template: T,
    data: ReportInputMap[T],
    idempotencyKey: string,
    options: CreateReportOptions = {{}},
  ) => request<ReportRun>(`/api/reports/templates/${{encodeURIComponent(template)}}/runs`, {{
    method: "POST",
    headers: {{ "Idempotency-Key": idempotencyKey }},
    body: JSON.stringify({{ data, ...options }}),
  }}),
  list: (page = 1, pageSize = 25) =>
    request<ReportRunList>(`/api/reports/runs?page=${{page}}&page_size=${{pageSize}}`),
  get: (id: string) => request<ReportRun>(`/api/reports/runs/${{encodeURIComponent(id)}}`),
  cancel: (id: string) => request<ReportRun>(`/api/reports/runs/${{encodeURIComponent(id)}}/cancel`, {{ method: "POST" }}),
  download: (id: string) => downloadReportPdf(id),
}};

async function downloadReportPdf(id: string): Promise<Blob> {{
  const path = `/api/reports/runs/${{encodeURIComponent(id)}}/download`;
  const response = await fetch(`${{API_BASE}}${{path}}`, {{
    credentials: "include",
    headers: requestHeaders(undefined, path),
  }});
  if (!response.ok) {{
    const body = (await response.json().catch(() => null)) as ErrorEnvelope | null;
    throw new ApiError(
      response.status,
      body?.error.code ?? "HTTP_ERROR",
      body?.error.message ?? `Request failed with status ${{response.status}}`,
      body?.error.fields,
    );
  }}
  return response.blob();
}}
"#
    )
}

fn schema_type(schema: &Value) -> String {
    if let Some(values) = schema.get("enum").and_then(Value::as_array) {
        return values
            .iter()
            .map(literal_type)
            .collect::<Vec<_>>()
            .join(" | ");
    }
    match schema.get("type").and_then(Value::as_str) {
        Some("object") => object_type(schema),
        Some("array") => format!(
            "Array<{}>",
            schema
                .get("items")
                .map_or_else(|| "unknown".to_owned(), schema_type)
        ),
        Some("string") => "string".to_owned(),
        Some("integer" | "number") => "number".to_owned(),
        Some("boolean") => "boolean".to_owned(),
        Some("null") => "null".to_owned(),
        _ => "unknown".to_owned(),
    }
}

fn object_type(schema: &Value) -> String {
    let required = schema
        .get("required")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect::<std::collections::BTreeSet<_>>();
    let Some(properties) = schema.get("properties").and_then(Value::as_object) else {
        return "Record<string, unknown>".to_owned();
    };
    let fields = properties
        .iter()
        .map(|(name, value)| {
            let optional = if required.contains(name.as_str()) {
                ""
            } else {
                "?"
            };
            format!("{:?}{optional}: {}", name, schema_type(value))
        })
        .collect::<Vec<_>>()
        .join("; ");
    format!("{{ {fields} }}")
}

fn literal_type(value: &Value) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "unknown".to_owned())
}
