use super::{CodegenError, render};
use quote::quote;

pub(super) fn disabled_source() -> Result<String, CodegenError> {
    render(quote! {
        use crate::AppState;
        use axum::Router;
        pub fn router() -> Router<AppState> { Router::new() }
    })
}
