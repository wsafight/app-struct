use appstruct_ir::AppIr;

#[allow(clippy::too_many_lines)]
pub(super) fn source(ir: &AppIr) -> String {
    let resources = ir
        .activity
        .resources
        .iter()
        .map(|resource| format!("{:?}", resource.resource))
        .collect::<Vec<_>>()
        .join(" | ");
    format!(
        r#"
export type ActivityResource = {resources};
export type ActivityEntryKind = "comment" | "system";

export interface ActivityEntry {{
  id: string;
  resource: ActivityResource;
  record_id: string;
  tenant_id: string | null;
  actor_id: string | null;
  kind: ActivityEntryKind;
  body: string | null;
  event: string | null;
  payload: Record<string, unknown> | null;
  attachment_file_id: string | null;
  attachment_name: string | null;
  attachment_content_type: string | null;
  withdrawn_at: string | null;
  withdrawn_by: string | null;
  governance_reason: string | null;
  occurred_at: string;
}}

export interface ActivityList {{
  data: ActivityEntry[];
  meta: {{ limit: number; next_cursor: string | null; has_more: boolean }};
}}

export interface ActivityAttachmentInput {{
  name: string;
  content_type: string;
  content_base64: string;
}}

export interface CreateActivityCommentInput {{
  body: string;
  attachment?: ActivityAttachmentInput;
}}

export const activityApi = {{
  list: (
    resource: ActivityResource,
    recordId: string,
    cursor?: string,
    limit = 20,
    options: RequestOptions = {{}},
  ) => {{
    const search = new URLSearchParams({{ limit: String(limit) }});
    if (cursor) search.set("cursor", cursor);
    return request<ActivityList>(`${{activityPath(resource, recordId)}}?${{search}}`, options);
  }},
  comment: (
    resource: ActivityResource,
    recordId: string,
    input: CreateActivityCommentInput,
  ) => request<ActivityEntry>(`${{activityPath(resource, recordId)}}/comments`, {{
    method: "POST",
    body: JSON.stringify(input),
  }}),
  withdraw: (resource: ActivityResource, recordId: string, entryId: string) =>
    request<ActivityEntry>(
      `${{activityPath(resource, recordId)}}/${{encodeURIComponent(entryId)}}/withdraw`,
      {{ method: "POST" }},
    ),
  moderate: (resource: ActivityResource, recordId: string, entryId: string, reason: string) =>
    request<ActivityEntry>(
      `${{activityPath(resource, recordId)}}/${{encodeURIComponent(entryId)}}/moderate`,
      {{ method: "POST", body: JSON.stringify({{ reason }}) }},
    ),
  download: (resource: ActivityResource, recordId: string, entryId: string) =>
    downloadActivityAttachment(resource, recordId, entryId),
}};

function activityPath(resource: ActivityResource, recordId: string): string {{
  return `/api/activity/${{encodeURIComponent(resource)}}/${{encodeURIComponent(recordId)}}`;
}}

async function downloadActivityAttachment(
  resource: ActivityResource,
  recordId: string,
  entryId: string,
): Promise<Blob> {{
  const path = `${{activityPath(resource, recordId)}}/${{encodeURIComponent(entryId)}}/attachment`;
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
