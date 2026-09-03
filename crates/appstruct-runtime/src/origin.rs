// Browser origin validation used by generated Auth configuration.

/// Validates an exact HTTP(S) origin used for CORS, CSRF, and frontend links.
///
/// # Errors
///
/// Returns an error when the value is empty, not `http`/`https`, or includes
/// credentials, a path, query, or fragment.
pub fn validate_browser_origin(name: &str, value: &str) -> Result<String, String> {
    let value = value.trim();
    if value.is_empty() {
        return Err(format!("{name} must not be empty"));
    }
    if value
        .bytes()
        .any(|byte| byte == b'\\' || !byte.is_ascii_graphic())
    {
        return Err(format!("{name} must be an ASCII HTTP origin"));
    }
    let rest = value
        .strip_prefix("https://")
        .or_else(|| value.strip_prefix("http://"))
        .ok_or_else(|| format!("{name} must be an http(s) origin"))?;
    if rest.is_empty() || rest.contains(['/', '?', '#', '@']) {
        return Err(format!(
            "{name} must be an origin without credentials, path, query, or fragment"
        ));
    }
    Ok(value.to_owned())
}

#[cfg(test)]
mod tests {
    use super::validate_browser_origin;

    #[test]
    fn accepts_http_and_https_origins() {
        assert_eq!(
            validate_browser_origin("ORIGIN", "http://127.0.0.1:5173").unwrap(),
            "http://127.0.0.1:5173"
        );
        assert_eq!(
            validate_browser_origin("ORIGIN", "https://app.example.com").unwrap(),
            "https://app.example.com"
        );
        assert_eq!(
            validate_browser_origin("ORIGIN", "  http://[::1]:5173  ").unwrap(),
            "http://[::1]:5173"
        );
    }

    #[test]
    fn rejects_empty_non_http_and_non_origin_urls() {
        for value in [
            "",
            "ftp://example.com",
            "http://example.com/",
            "https://example.com/path",
            "http://user@host",
            "https://example.com?q=1",
            "http://example.com#frag",
        ] {
            assert!(validate_browser_origin("ORIGIN", value).is_err(), "{value}");
        }
    }
}
