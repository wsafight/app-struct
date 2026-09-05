use super::ReportWork;
use base64::Engine as _;
use serde::{Deserialize, Serialize};
use sha2::Digest as _;
use std::time::Duration;

const MAX_HTML: usize = 2 * 1024 * 1024;
const MAX_PDF: usize = 50 * 1024 * 1024;
const MAX_RESPONSE: usize = MAX_PDF * 4 / 3 + 8192;

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", sha2::Sha256::digest(bytes))
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
    deadline_ms: i64,
}

#[derive(Deserialize)]
struct RenderResponse {
    protocol: u32,
    request_id: uuid::Uuid,
    run_id: uuid::Uuid,
    renderer: String,
    artifact_digest: String,
    html_sha256: String,
    code: String,
    #[serde(default)]
    pdf: String,
    #[serde(default)]
    sha256: String,
    #[serde(default)]
    byte_length: usize,
    #[serde(default)]
    pages: u32,
}

pub(super) async fn render(
    work: &ReportWork,
    input: &serde_json::Value,
) -> Result<Vec<u8>, String> {
    let mut environment = minijinja::Environment::new();
    environment.set_auto_escape_callback(|_| minijinja::AutoEscape::Html);
    environment.set_fuel(Some(100_000));
    let template = environment
        .template_from_str(&work.body)
        .map_err(|_| "REPORT_INVALID_TEMPLATE_ARTIFACT")?;
    let mut output = HtmlWriter(Vec::new());
    template
        .render_to_write(
            minijinja::context! {
                input => input, locale => work.locale, timezone => work.timezone,
                paper => work.paper, orientation => work.orientation,
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
        renderer: "chromium-v1",
        template: &work.template,
        template_version: work.template_version,
        artifact_digest: &work.artifact_digest,
        html_sha256: sha256_hex(html.as_bytes()),
        html,
        locale: &work.locale,
        timezone: &work.timezone,
        paper: &work.paper,
        orientation: &work.orientation,
        deadline_ms: chrono::Utc::now().timestamp_millis() + 30_000,
    };
    tokio::time::timeout(Duration::from_secs(30), exchange(&request))
        .await
        .map_err(|_| "REPORT_RENDER_TIMEOUT".to_owned())?
}

#[cfg(unix)]
async fn exchange(request: &RenderRequest<'_>) -> Result<Vec<u8>, String> {
    use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
    let path =
        std::env::var_os("APPSTRUCT_REPORT_RENDERER_SOCKET").ok_or("REPORT_ADAPTER_UNAVAILABLE")?;
    let mut socket = tokio::net::UnixStream::connect(path)
        .await
        .map_err(|_| "REPORT_ADAPTER_UNAVAILABLE")?;
    let body = serde_json::to_vec(request).map_err(|_| "REPORT_INVALID_TEMPLATE_ARTIFACT")?;
    socket
        .write_u32(body.len() as u32)
        .await
        .map_err(|_| "REPORT_ADAPTER_UNAVAILABLE")?;
    socket
        .write_all(&body)
        .await
        .map_err(|_| "REPORT_ADAPTER_UNAVAILABLE")?;
    let size = socket
        .read_u32()
        .await
        .map_err(|_| "REPORT_ADAPTER_UNAVAILABLE")? as usize;
    if size == 0 || size > MAX_RESPONSE {
        return Err("REPORT_RESOURCE_LIMIT".into());
    }
    let mut bytes = vec![0; size];
    socket
        .read_exact(&mut bytes)
        .await
        .map_err(|_| "REPORT_ADAPTER_UNAVAILABLE")?;
    let response: RenderResponse =
        serde_json::from_slice(&bytes).map_err(|_| "REPORT_INVALID_OUTPUT")?;
    validate_response(request, response)
}

#[cfg(not(unix))]
async fn exchange(_request: &RenderRequest<'_>) -> Result<Vec<u8>, String> {
    Err("REPORT_ADAPTER_UNAVAILABLE".into())
}

fn validate_response(
    request: &RenderRequest<'_>,
    response: RenderResponse,
) -> Result<Vec<u8>, String> {
    if response.protocol != 1
        || response.request_id != request.request_id
        || response.run_id != request.run_id
        || response.renderer != request.renderer
        || response.artifact_digest != request.artifact_digest
        || response.html_sha256 != request.html_sha256
    {
        return Err("REPORT_INVALID_OUTPUT".into());
    }
    if response.code != "OK" {
        return Err(match response.code.as_str() {
            "REPORT_BLOCKED_RESOURCE"
            | "REPORT_RENDER_TIMEOUT"
            | "REPORT_RESOURCE_LIMIT"
            | "REPORT_BROWSER_CRASH"
            | "REPORT_ADAPTER_UNAVAILABLE"
            | "REPORT_INVALID_TEMPLATE_ARTIFACT" => response.code,
            _ => "REPORT_INVALID_OUTPUT".into(),
        });
    }
    if !(1..=100).contains(&response.pages)
        || response.byte_length > MAX_PDF
        || response.pdf.len() > MAX_RESPONSE - 8192
    {
        return Err("REPORT_RESOURCE_LIMIT".into());
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(response.pdf)
        .map_err(|_| "REPORT_INVALID_OUTPUT")?;
    if bytes.len() != response.byte_length
        || !bytes.starts_with(b"%PDF-")
        || sha256_hex(&bytes) != response.sha256
    {
        return Err("REPORT_INVALID_OUTPUT".into());
    }
    let document = lopdf::Document::load_mem(&bytes).map_err(|_| "REPORT_INVALID_OUTPUT")?;
    if document.get_pages().len() != response.pages as usize {
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
