use super::ReportWork;
use base64::Engine as _;
use serde::{Deserialize, Serialize};
use std::time::Duration;

const MAX_HTML: usize = 2 * 1024 * 1024;
const MAX_PDF: usize = 50 * 1024 * 1024;
const READY_SIGNAL: &str = "__REPORT_READY__";

fn sha256_hex(bytes: &[u8]) -> String {
    appstruct_runtime::sha256_hex(bytes)
}

fn validate_page_settings(paper: &str, orientation: &str) -> Result<(), String> {
    let valid_paper = matches!(
        paper.to_ascii_lowercase().as_str(),
        "a3" | "a4" | "a5" | "letter" | "legal"
    );
    let valid_orientation = matches!(
        orientation.to_ascii_lowercase().as_str(),
        "portrait" | "landscape"
    );
    if valid_paper && valid_orientation {
        Ok(())
    } else {
        Err("REPORT_INVALID_TEMPLATE_ARTIFACT".to_owned())
    }
}

fn validate_renderer_url(raw: &str) -> Result<&str, String> {
    let trimmed = raw.trim();
    if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        Ok(trimmed)
    } else {
        Err("REPORT_ADAPTER_UNAVAILABLE".to_owned())
    }
}

#[derive(Serialize)]
struct RenderRequest<'a> {
    protocol: u32,
    request_id: uuid::Uuid,
    run_id: uuid::Uuid,
    tenant_id: Option<uuid::Uuid>,
    renderer: &'static str,
    template: &'a str,
    template_version: i32,
    artifact_digest: &'a str,
    html: String,
    html_sha256: String,
    locale: &'a str,
    timezone: &'a str,
    paper: &'a str,
    orientation: &'a str,
    ready_signal: &'static str,
    wait_for_fonts: bool,
    block_private_network: bool,
    deadline_ms: i64,
}

#[derive(Deserialize)]
struct RenderResponse {
    #[serde(default)]
    code: String,
    #[serde(default)]
    pdf: String,
    #[serde(default)]
    sha256: String,
    #[serde(default)]
    byte_length: usize,
}

pub(super) async fn render(
    work: &ReportWork,
    input: &serde_json::Value,
) -> Result<Vec<u8>, String> {
    validate_page_settings(&work.paper, &work.orientation)?;
    let mut environment = minijinja::Environment::new();
    environment.set_auto_escape_callback(|_| minijinja::AutoEscape::Html);
    environment.set_fuel(Some(100_000));
    let template = environment
        .template_from_str(&work.body)
        .map_err(|_| "REPORT_INVALID_TEMPLATE_ARTIFACT")?;
    let mut output = HtmlWriter(Vec::new());
    template
        .render_captured_to(
            minijinja::context! {
                input => minijinja::value::Serde(input),
                locale => &work.locale,
                timezone => &work.timezone,
                paper => &work.paper,
                orientation => &work.orientation,
            },
            &mut output,
        )
        .map_err(|_| "REPORT_TEMPLATE_RENDER_FAILED")?;
    let html = String::from_utf8(output.0).map_err(|_| "REPORT_INVALID_TEMPLATE_ARTIFACT")?;
    let request = RenderRequest {
        protocol: 1,
        request_id: uuid::Uuid::now_v7(),
        run_id: work.id,
        tenant_id: work.tenant_id,
        renderer: "http-v1",
        template: &work.template,
        template_version: work.template_version,
        artifact_digest: &work.artifact_digest,
        html_sha256: sha256_hex(html.as_bytes()),
        html,
        locale: &work.locale,
        timezone: &work.timezone,
        paper: &work.paper,
        orientation: &work.orientation,
        ready_signal: READY_SIGNAL,
        wait_for_fonts: true,
        block_private_network: true,
        deadline_ms: chrono::Utc::now().timestamp_millis() + 30_000,
    };
    tokio::time::timeout(Duration::from_secs(30), exchange(&request))
        .await
        .map_err(|_| "REPORT_RENDER_TIMEOUT".to_owned())?
}

async fn exchange(request: &RenderRequest<'_>) -> Result<Vec<u8>, String> {
    let raw_url = std::env::var("APPSTRUCT_REPORT_RENDERER_URL")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .ok_or("REPORT_ADAPTER_UNAVAILABLE")?;
    let url = validate_renderer_url(&raw_url)?;
    let body = serde_json::to_vec(request).map_err(|_| "REPORT_INVALID_TEMPLATE_ARTIFACT")?;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|_| "REPORT_ADAPTER_UNAVAILABLE")?;
    let mut builder = client
        .post(url)
        .header("content-type", "application/json")
        .header("x-appstruct-request-id", request.request_id.to_string());
    if let Ok(secret) = std::env::var("APPSTRUCT_REPORT_RENDERER_SECRET")
        && !secret.is_empty()
    {
        let key = ring::hmac::Key::new(ring::hmac::HMAC_SHA256, secret.as_bytes());
        let tag = ring::hmac::sign(&key, &body);
        let mut hex = String::with_capacity(tag.as_ref().len() * 2);
        for byte in tag.as_ref() {
            use std::fmt::Write as _;
            let _ = write!(&mut hex, "{byte:02x}");
        }
        builder = builder.header("x-appstruct-signature", format!("sha256={hex}"));
    }
    let response = builder
        .body(body)
        .send()
        .await
        .map_err(|_| "REPORT_ADAPTER_UNAVAILABLE")?;
    let status = response.status();
    if status.is_client_error() {
        if let Ok(bytes) = response.bytes().await
            && let Ok(payload) = serde_json::from_slice::<RenderResponse>(&bytes)
            && !payload.code.is_empty()
        {
            return validate_json_response(payload);
        }
        return Err("REPORT_INVALID_TEMPLATE_ARTIFACT".into());
    }
    if !status.is_success() {
        return Err("REPORT_ADAPTER_UNAVAILABLE".into());
    }
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("")
        .to_ascii_lowercase();
    let bytes = response
        .bytes()
        .await
        .map_err(|_| "REPORT_ADAPTER_UNAVAILABLE")?;
    if content_type.starts_with("application/pdf") {
        return validate_pdf(bytes.to_vec());
    }
    let payload: RenderResponse =
        serde_json::from_slice(&bytes).map_err(|_| "REPORT_INVALID_OUTPUT")?;
    validate_json_response(payload)
}

fn validate_json_response(response: RenderResponse) -> Result<Vec<u8>, String> {
    if !response.code.is_empty() && response.code != "OK" {
        return Err(match response.code.as_str() {
            "REPORT_BLOCKED_RESOURCE"
            | "REPORT_RENDER_TIMEOUT"
            | "REPORT_RESOURCE_LIMIT"
            | "REPORT_ADAPTER_UNAVAILABLE"
            | "REPORT_INVALID_TEMPLATE_ARTIFACT" => response.code,
            _ => "REPORT_INVALID_OUTPUT".into(),
        });
    }
    if response.byte_length > MAX_PDF {
        return Err("REPORT_RESOURCE_LIMIT".into());
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(response.pdf)
        .map_err(|_| "REPORT_INVALID_OUTPUT")?;
    if response.byte_length > 0 && bytes.len() != response.byte_length {
        return Err("REPORT_INVALID_OUTPUT".into());
    }
    if !response.sha256.is_empty() && sha256_hex(&bytes) != response.sha256 {
        return Err("REPORT_INVALID_OUTPUT".into());
    }
    validate_pdf(bytes)
}

fn validate_pdf(bytes: Vec<u8>) -> Result<Vec<u8>, String> {
    if bytes.is_empty() || bytes.len() > MAX_PDF {
        return Err("REPORT_RESOURCE_LIMIT".into());
    }
    if !bytes.starts_with(b"%PDF-") {
        return Err("REPORT_INVALID_OUTPUT".into());
    }
    Ok(bytes)
}

struct HtmlWriter(Vec<u8>);
impl std::io::Write for HtmlWriter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if self.0.len().saturating_add(bytes.len()) > MAX_HTML {
            return Err(std::io::Error::other("HTML limit exceeded"));
        }
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
