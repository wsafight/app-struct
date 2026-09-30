use crate::surface::{SurfaceFrameOptions, SurfaceHsts, SurfaceReferrerPolicy, SurfaceRoot};
use appstruct_ir::{
    Diagnostic, FrameOptionsIr, HstsIr, ReferrerPolicyIr, SecurityHeadersIr, ServerIr,
};

pub(crate) fn lower_server(root: &SurfaceRoot, diagnostics: &mut Vec<Diagnostic>) -> ServerIr {
    let headers = &root.server.security_headers;
    if !headers.enabled {
        let span = headers
            .span
            .clone()
            .unwrap_or_else(|| root.app_name.span.clone());
        if headers.content_security_policy.is_some() || !headers.custom.is_empty() {
            diagnostics.push(Diagnostic::warning(
                "AS3108",
                "`server.security_headers` is disabled, so the configured headers are ignored",
                span,
            ));
        }
    }
    ServerIr {
        security_headers: SecurityHeadersIr {
            enabled: headers.enabled,
            hsts: match headers.hsts {
                SurfaceHsts::Auto => HstsIr::Auto,
                SurfaceHsts::Off => HstsIr::Off,
                SurfaceHsts::MaxAge(seconds) => HstsIr::MaxAge(seconds),
            },
            frame_options: match headers.frame_options {
                SurfaceFrameOptions::Deny => FrameOptionsIr::Deny,
                SurfaceFrameOptions::SameOrigin => FrameOptionsIr::SameOrigin,
                SurfaceFrameOptions::Off => FrameOptionsIr::Off,
            },
            referrer_policy: match headers.referrer_policy {
                SurfaceReferrerPolicy::NoReferrer => ReferrerPolicyIr::NoReferrer,
                SurfaceReferrerPolicy::SameOrigin => ReferrerPolicyIr::SameOrigin,
                SurfaceReferrerPolicy::StrictOriginWhenCrossOrigin => {
                    ReferrerPolicyIr::StrictOriginWhenCrossOrigin
                }
                SurfaceReferrerPolicy::Off => ReferrerPolicyIr::Off,
            },
            content_security_policy: headers
                .content_security_policy
                .as_ref()
                .map(|policy| policy.value.clone()),
            custom: headers.custom.clone(),
        },
    }
}
