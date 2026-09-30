use super::{IrValidationError, push};
use crate::AppIr;

pub(super) fn validate_server(ir: &AppIr, errors: &mut Vec<IrValidationError>) {
    let headers = &ir.server.security_headers;
    if let Some(policy) = &headers.content_security_policy
        && !is_header_value(policy)
    {
        push(
            errors,
            "server.security_headers.content_security_policy",
            "must contain only visible ASCII characters or horizontal tabs",
        );
    }
    for (name, value) in &headers.custom {
        let path = format!("server.security_headers.custom.{name}");
        if !is_header_name(name) {
            push(
                errors,
                &path,
                "header name must use lowercase ASCII letters, digits, and hyphens",
            );
        }
        if !is_header_value(value) {
            push(
                errors,
                path,
                "header value must contain only visible ASCII characters or horizontal tabs",
            );
        }
    }
}

fn is_header_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

fn is_header_value(value: &str) -> bool {
    value
        .bytes()
        .all(|byte| byte == b'\t' || (32..127).contains(&byte))
}
