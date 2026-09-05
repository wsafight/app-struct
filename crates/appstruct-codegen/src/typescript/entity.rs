use appstruct_ir::{AppIr, EntityIr};

pub(super) fn client(ir: &AppIr, entity: &EntityIr) -> String {
    let variable = lower_camel(&entity.rust_name);
    let model = &entity.rust_name;
    let path = format!("/api/{}/", entity.table_name);
    let collections = super::collections::client(entity, &path);
    let restore = if entity.views.soft_delete {
        format!(
            "  trash: (query: Pick<ListQuery, \"page\" | \"page_size\"> = {{}}, options: RequestOptions = {{}}) => request<ListResponse<{model}>>(listPath(\"{path}_trash\", query), options),\n  restore: (input: BulkDeleteRequest) => request<BulkResult>(\"{path}_restore\", {{ method: \"POST\", body: JSON.stringify(input) }}),\n"
        )
    } else {
        String::new()
    };
    let workflow = entity.workflow.as_ref().map_or_else(String::new, |workflow| {
        let typed = workflow
            .transitions
            .iter()
            .map(|transition| {
                let name = &transition.name;
                let endpoint = format!("${{member}}/_transitions/{name}");
                transition.input.as_ref().map_or_else(
                    || format!(
                        "  {name}: (id: string) => {{\n    const member = `{path}${{encodeURIComponent(id)}}`;\n    return request<{model}>(`{endpoint}`, {{ method: \"POST\", body: \"{{}}\" }}, member, true);\n  }},\n",
                    ),
                    |input| {
                        let input_type = ir
                            .value_objects
                            .iter()
                            .find(|value| value.id == *input)
                            .expect("IR validation guarantees workflow input exists")
                            .rust_name
                            .as_str();
                        format!(
                            "  {name}: (id: string, input: {input_type}) => {{\n    const member = `{path}${{encodeURIComponent(id)}}`;\n    return request<{model}>(`{endpoint}`, {{ method: \"POST\", body: JSON.stringify(input) }}, member, true);\n  }},\n",
                        )
                    },
                )
            })
            .collect::<String>();
        format!(
            "  transitions: (id: string, options: RequestOptions = {{}}) => {{\n    const member = `{path}${{encodeURIComponent(id)}}`;\n    return request<WorkflowCapabilities>(`${{member}}/_transitions`, options, member);\n  }},\n  transition: (id: string, action: {model}WorkflowTransition, input: unknown = {{}}) => {{\n    const member = `{path}${{encodeURIComponent(id)}}`;\n    return request<{model}>(`${{member}}/_transitions/${{encodeURIComponent(action)}}`, {{ method: \"POST\", body: JSON.stringify(input) }}, member, true);\n  }},\n{typed}",
        )
    });
    format!(
        r#"export const {variable}Api = {{
  list: (query: ListQuery = {{}}, options: RequestOptions = {{}}) => request<ListResponse<{model}>>(listPath("{path}", query), options),
  lookup: (ids: string[], options: RequestOptions = {{}}) => request<{model}[]>("{path}_lookup?" + new URLSearchParams({{ ids: ids.join(",") }}), options),
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
{restore}{workflow}{collections}}};
"#
    )
}

fn lower_camel(value: &str) -> String {
    let mut characters = value.chars();
    characters.next().map_or_else(String::new, |first| {
        first.to_lowercase().chain(characters).collect()
    })
}
