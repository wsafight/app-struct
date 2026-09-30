use appstruct_ir::{AppIr, FrameOptionsIr, HstsIr, ReferrerPolicyIr};
use proc_macro2::TokenStream;
use quote::quote;

const HSTS_AUTO_MAX_AGE: u64 = 31_536_000;

/// Emits the import fragment and the `apply_security_headers` helper for the generated backend.
///
/// The helper is emitted even when the feature is disabled, because the router always calls it; a
/// disabled configuration emits a pass-through. Header literals are compile-time constants, so the
/// generated crate stays deterministic and reads no environment at request time.
pub(super) fn source(ir: &AppIr) -> TokenStream {
    let headers = &ir.server.security_headers;
    if !headers.enabled {
        return quote! {
            fn apply_security_headers<S>(router: Router<S>) -> Router<S>
            where
                S: Clone + Send + Sync + 'static,
            {
                router
            }
        };
    }
    let static_layers = static_layers(ir);
    let hsts = hsts_layer(&headers.hsts);
    quote! {
        use tower_http::set_header::SetResponseHeaderLayer;

        fn apply_security_headers<S>(router: Router<S>) -> Router<S>
        where
            S: Clone + Send + Sync + 'static,
        {
            let router = router #(#static_layers)*;
            #hsts
            router
        }
    }
}

fn static_layers(ir: &AppIr) -> Vec<TokenStream> {
    let headers = &ir.server.security_headers;
    let mut layers = vec![quote! {
        .layer(SetResponseHeaderLayer::overriding(
            axum::http::header::X_CONTENT_TYPE_OPTIONS,
            axum::http::HeaderValue::from_static("nosniff"),
        ))
    }];
    match headers.frame_options {
        FrameOptionsIr::Deny => layers.push(frame_options_layer("DENY")),
        FrameOptionsIr::SameOrigin => layers.push(frame_options_layer("SAMEORIGIN")),
        FrameOptionsIr::Off => {}
    }
    match headers.referrer_policy {
        ReferrerPolicyIr::NoReferrer => layers.push(referrer_policy_layer("no-referrer")),
        ReferrerPolicyIr::SameOrigin => layers.push(referrer_policy_layer("same-origin")),
        ReferrerPolicyIr::StrictOriginWhenCrossOrigin => {
            layers.push(referrer_policy_layer("strict-origin-when-cross-origin"));
        }
        ReferrerPolicyIr::Off => {}
    }
    if let Some(policy) = &headers.content_security_policy {
        layers.push(quote! {
            .layer(SetResponseHeaderLayer::overriding(
                axum::http::header::CONTENT_SECURITY_POLICY,
                axum::http::HeaderValue::from_static(#policy),
            ))
        });
    }
    for (name, value) in &headers.custom {
        // Custom headers must not clobber a header an inner layer already produced (CORS, ETag).
        layers.push(quote! {
            .layer(SetResponseHeaderLayer::if_not_present(
                axum::http::HeaderName::from_static(#name),
                axum::http::HeaderValue::from_static(#value),
            ))
        });
    }
    layers
}

fn frame_options_layer(value: &str) -> TokenStream {
    quote! {
        .layer(SetResponseHeaderLayer::overriding(
            axum::http::header::X_FRAME_OPTIONS,
            axum::http::HeaderValue::from_static(#value),
        ))
    }
}

fn referrer_policy_layer(value: &str) -> TokenStream {
    quote! {
        .layer(SetResponseHeaderLayer::overriding(
            axum::http::header::REFERRER_POLICY,
            axum::http::HeaderValue::from_static(#value),
        ))
    }
}

fn hsts_layer(hsts: &HstsIr) -> TokenStream {
    match hsts {
        HstsIr::Off => TokenStream::new(),
        HstsIr::MaxAge(max_age) => {
            let value = format!("max-age={max_age}; includeSubDomains");
            quote! {
                let router = router.layer(SetResponseHeaderLayer::overriding(
                    axum::http::header::STRICT_TRANSPORT_SECURITY,
                    axum::http::HeaderValue::from_static(#value),
                ));
            }
        }
        HstsIr::Auto => {
            let value = format!("max-age={HSTS_AUTO_MAX_AGE}; includeSubDomains");
            quote! {
                let router = if std::env::var("APPSTRUCT_ENV").as_deref() == Ok("production") {
                    router.layer(SetResponseHeaderLayer::overriding(
                        axum::http::header::STRICT_TRANSPORT_SECURITY,
                        axum::http::HeaderValue::from_static(#value),
                    ))
                } else {
                    router
                };
            }
        }
    }
}
