use appstruct_ir::FieldTypeIr;
use proc_macro2::TokenStream;
use quote::quote;

pub(super) fn attributes(ty: &FieldTypeIr, option_depth: u8) -> TokenStream {
    if !matches!(ty, FieldTypeIr::Bigint) {
        return if option_depth >= 2 {
            quote! { #[serde(default, deserialize_with = "appstruct_runtime::deserialize_patch")] }
        } else {
            TokenStream::new()
        };
    }
    match option_depth {
        0 => quote! { #[serde(with = "appstruct_runtime::bigint")] },
        1 => quote! { #[serde(default, with = "appstruct_runtime::bigint::optional")] },
        _ => quote! { #[serde(default, with = "appstruct_runtime::bigint::patch")] },
    }
}
