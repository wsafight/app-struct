use appstruct_ir::EntityIr;

pub(super) fn types() -> &'static str {
    r"export interface CollectionBatch<C = Record<string, unknown>, U = Record<string, unknown>> {
  creates?: { key: string; input: C }[];
  updates?: { id: string; revision: number; input: U }[];
  deletes?: { id: string; revision: number }[];
}
export interface CollectionResponse<P = Record<string, unknown>, C = Record<string, unknown>> {
  parent: P;
  rows: C[];
  created: Record<string, string>;
}
"
}

pub(super) fn client(entity: &EntityIr, path: &str) -> String {
    if entity.views.aggregates.is_empty() {
        return String::new();
    }
    let names = entity
        .views
        .aggregates
        .iter()
        .map(|aggregate| format!("{:?}", aggregate.name))
        .collect::<Vec<_>>()
        .join(" | ");
    let model = &entity.rust_name;
    format!(
        r#"  collection: (id: string, name: {names}, options: RequestOptions = {{}}) =>
    request<CollectionResponse<{model}>>(`{path}${{encodeURIComponent(id)}}/_aggregates/${{encodeURIComponent(name)}}`, options),
  saveCollection: (id: string, name: {names}, revision: number, input: CollectionBatch) =>
    request<CollectionResponse<{model}>>(`{path}${{encodeURIComponent(id)}}/_aggregates/${{encodeURIComponent(name)}}`, {{ method: "POST", headers: {{ "If-Match": `"rev-${{revision}}"` }}, body: JSON.stringify(input) }}),
"#
    )
}
