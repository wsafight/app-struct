use appstruct_ir::EntityIr;

pub(super) fn client(entity: &EntityIr) -> String {
    let variable = lower_camel(&entity.rust_name);
    let model = &entity.rust_name;
    let path = format!("/api/{}/", entity.table_name);
    let restore = if entity.views.soft_delete {
        format!(
            "  trash: (query: Pick<ListQuery, \"page\" | \"page_size\"> = {{}}, options: RequestOptions = {{}}) => request<ListResponse<{model}>>(listPath(\"{path}_trash\", query), options),\n  restore: (input: BulkDeleteRequest) => request<BulkResult>(\"{path}_restore\", {{ method: \"POST\", body: JSON.stringify(input) }}),\n"
        )
    } else {
        String::new()
    };
    format!(
        r#"export const {variable}Api = {{
  list: (query: ListQuery = {{}}, options: RequestOptions = {{}}) => request<ListResponse<{model}>>(listPath("{path}", query), options),
  listCursor: (query: CursorListQuery = {{}}, options: RequestOptions = {{}}) =>
    request<CursorListResponse<{model}>>(listPath("{path}", {{ limit: 25, ...query }}), options),
  aggregate: (query: AggregateQuery = {{}}, options: RequestOptions = {{}}) =>
    request<AggregateResponse>(aggregatePath("{path}_aggregate", query), options),
  get: (id: string, options: RequestOptions = {{}}) => {{
    const member = `{path}${{encodeURIComponent(id)}}`;
    return request<{model}>(member, options, member);
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
{restore}}};
"#
    )
}

fn lower_camel(value: &str) -> String {
    let mut characters = value.chars();
    characters.next().map_or_else(String::new, |first| {
        first.to_lowercase().chain(characters).collect()
    })
}
