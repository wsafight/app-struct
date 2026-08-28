use appstruct_ir::EntityIr;

pub(super) fn client(entity: &EntityIr) -> String {
    let variable = lower_camel(&entity.rust_name);
    let model = &entity.rust_name;
    let path = format!("/api/{}/", entity.table_name);
    format!(
        r#"export const {variable}Api = {{
  list: (query: ListQuery = {{}}) => request<ListResponse<{model}>>(listPath("{path}", query)),
  listCursor: (query: CursorListQuery = {{}}) =>
    request<CursorListResponse<{model}>>(listPath("{path}", {{ limit: 25, ...query }})),
  aggregate: (query: AggregateQuery = {{}}) =>
    request<AggregateResponse>(aggregatePath("{path}_aggregate", query)),
  get: (id: string) => {{
    const member = `{path}${{encodeURIComponent(id)}}`;
    return request<{model}>(member, undefined, member);
  }},
  create: (input: Create{model}Input) =>
    request<{model}>("{path}", {{ method: "POST", body: JSON.stringify(input) }}),
  update: (id: string, input: Update{model}Input) => {{
    const member = `{path}${{encodeURIComponent(id)}}`;
    return request<{model}>(member, {{ method: "PATCH", body: JSON.stringify(input) }}, member);
  }},
  remove: (id: string) => {{
    const member = `{path}${{encodeURIComponent(id)}}`;
    return request<void>(member, {{ method: "DELETE" }});
  }},
  bulkUpdate: (input: BulkUpdateRequest<Update{model}Input>) => request<BulkResult>("{path}_bulk", {{ method: "PATCH", body: JSON.stringify(input) }}),
  bulkDelete: (input: BulkDeleteRequest) => request<BulkResult>("{path}_bulk", {{ method: "DELETE", body: JSON.stringify(input) }}),
  exportCsv: () => requestText("{path}_export.csv"),
  importCsv: (csv: string) => request<BulkResult>("{path}_import.csv", {{ method: "POST", headers: {{ "Content-Type": "text/csv" }}, body: csv }}),
}};
"#
    )
}

fn lower_camel(value: &str) -> String {
    let mut characters = value.chars();
    characters.next().map_or_else(String::new, |first| {
        first.to_lowercase().chain(characters).collect()
    })
}
