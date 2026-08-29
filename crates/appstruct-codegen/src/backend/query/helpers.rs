use super::super::parse_ident;
use crate::CodegenError;
use appstruct_ir::FieldIr;

pub(crate) fn column_ident(field: &FieldIr) -> Result<syn::Ident, CodegenError> {
    let name = field
        .rust_name
        .split('_')
        .map(|part| {
            let mut chars = part.chars();
            chars.next().map_or_else(String::new, |first| {
                first.to_uppercase().chain(chars).collect::<String>()
            })
        })
        .collect::<String>();
    parse_ident(&name)
}
