use appstruct_ir::AppIr;

pub(super) fn source(ir: &AppIr) -> String {
    include_str!("../../templates/web/file/client.ts")
        .replace("__MAX_BYTES__", &ir.file.max_bytes.to_string())
        .replace(
            "__ALLOWED_TYPES__",
            &serde_json::to_string(&ir.file.allowed_content_types).unwrap(),
        )
}
