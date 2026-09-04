use std::{io::Read, time::Duration};
use url::{Host, Url};

pub(super) fn package_url(registry: &str, name: &str, version: &str) -> Result<Url, String> {
    let mut url = validate_registry_url(registry)?;
    let mut segments = url
        .path_segments_mut()
        .map_err(|()| "registry URL cannot be used as a base URL".to_owned())?;
    segments.pop_if_empty().push("v1").push("modules");
    for segment in name.split('/') {
        segments.push(segment);
    }
    segments.push(version);
    drop(segments);
    Ok(url)
}

pub(super) fn download(url: &Url, reference: &str, max_bytes: usize) -> Result<Vec<u8>, String> {
    let mut response = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|error| format!("cannot create registry client: {error}"))?
        .get(url.clone())
        .header("accept", "application/json")
        .send()
        .map_err(|error| format!("cannot download `{url}`: {error}"))?
        .error_for_status()
        .map_err(|error| format!("registry rejected `{reference}`: {error}"))?;
    let content_length = response.content_length();
    read_bounded(&mut response, content_length, max_bytes)
}

fn validate_registry_url(registry: &str) -> Result<Url, String> {
    let url = Url::parse(registry).map_err(|_| {
        "registry URL must be an absolute HTTPS URL (HTTP is allowed only for localhost)".to_owned()
    })?;
    let has_safe_authority = url.username().is_empty()
        && url.password().is_none()
        && url.query().is_none()
        && url.fragment().is_none();
    let transport_allowed = match url.scheme() {
        "https" => url.host().is_some(),
        "http" => match url.host() {
            Some(Host::Domain(host)) => host.eq_ignore_ascii_case("localhost"),
            Some(Host::Ipv4(address)) => address.is_loopback(),
            Some(Host::Ipv6(address)) => address.is_loopback(),
            None => false,
        },
        _ => false,
    };
    if has_safe_authority && transport_allowed {
        Ok(url)
    } else {
        Err("registry URL must use HTTPS (HTTP is allowed only for localhost)".to_owned())
    }
}

fn read_bounded(
    reader: impl Read,
    content_length: Option<u64>,
    max_bytes: usize,
) -> Result<Vec<u8>, String> {
    let max_bytes_u64 = u64::try_from(max_bytes).map_err(|_| "response limit is too large")?;
    if content_length.is_some_and(|length| length > max_bytes_u64) {
        return Err(format!("registry response exceeds {max_bytes} bytes"));
    }
    let mut bytes = Vec::with_capacity(
        content_length
            .and_then(|length| usize::try_from(length).ok())
            .unwrap_or_default(),
    );
    reader
        .take(max_bytes_u64.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|error| format!("cannot read registry response: {error}"))?;
    if bytes.len() > max_bytes {
        return Err(format!("registry response exceeds {max_bytes} bytes"));
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::{package_url, read_bounded};
    use std::io::Cursor;

    #[test]
    fn registry_urls_require_secure_or_loopback_transports() {
        assert_eq!(
            package_url(
                "https://registry.example.com/base",
                "vendor/module",
                "1.2.3"
            )
            .unwrap()
            .as_str(),
            "https://registry.example.com/base/v1/modules/vendor/module/1.2.3"
        );
        assert!(package_url("http://127.0.0.1:8080", "vendor/module", "1",).is_ok());
        assert!(package_url("http://[::1]:8080", "vendor/module", "1").is_ok());
        assert!(package_url("http://example.com", "vendor/module", "1").is_err());
        assert!(
            package_url(
                "http://localhost:8080@external.example",
                "vendor/module",
                "1"
            )
            .is_err()
        );
        assert!(package_url("https://user@example.com", "vendor/module", "1").is_err());
        assert!(
            package_url(
                "https://registry.example.com?path=bad",
                "vendor/module",
                "1"
            )
            .is_err()
        );
    }

    #[test]
    fn registry_responses_are_bounded_while_reading() {
        assert_eq!(
            read_bounded(Cursor::new(b"test"), Some(4), 4).unwrap(),
            b"test"
        );
        assert!(read_bounded(Cursor::new(b"test"), Some(4), 3).is_err());
        assert!(read_bounded(Cursor::new(b"test"), None, 3).is_err());
    }
}
