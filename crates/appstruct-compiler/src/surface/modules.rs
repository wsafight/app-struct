use super::Located;
use appstruct_ir::SourceSpan;

#[derive(Clone, Debug)]
pub(crate) struct SurfacePreset {
    pub name: Located<String>,
    pub version: Located<u64>,
}

#[allow(clippy::struct_excessive_bools)]
#[derive(Clone, Debug, Default)]
pub(crate) struct SurfaceAuth {
    pub enabled: bool,
    pub user_entity: Option<Located<String>>,
    pub registration_enabled: bool,
    pub password_reset_enabled: bool,
    pub oauth_enabled: bool,
    pub roles: Vec<Located<String>>,
    pub default_role: Option<Located<String>>,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct SurfaceTenant {
    pub enabled: bool,
    pub span: Option<SourceSpan>,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct SurfaceAudit {
    pub enabled: bool,
    pub reader_roles: Vec<Located<String>>,
    pub span: Option<SourceSpan>,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct SurfaceMail {
    pub enabled: bool,
    pub provider: Option<Located<String>>,
    pub from: Option<Located<String>>,
    pub templates: Vec<SurfaceMailTemplate>,
    pub span: Option<SourceSpan>,
}

#[derive(Clone, Debug)]
pub(crate) struct SurfaceMailTemplate {
    pub name: Located<String>,
    pub subject: Located<String>,
    pub text: Located<String>,
    pub html: Option<Located<String>>,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct SurfaceJobs {
    pub enabled: bool,
    pub poll_interval_ms: Option<Located<u64>>,
    pub lease_seconds: Option<Located<u64>>,
    pub queues: Vec<SurfaceJobQueue>,
    pub span: Option<SourceSpan>,
}

#[derive(Clone, Debug)]
pub(crate) struct SurfaceJobQueue {
    pub name: Located<String>,
    pub max_attempts: Option<Located<u64>>,
    pub backoff_seconds: Option<Located<u64>>,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct SurfaceFile {
    pub enabled: bool,
    pub provider: Option<Located<String>>,
    pub local_root: Option<Located<String>>,
    pub max_bytes: Option<Located<u64>>,
    pub allowed_content_types: Vec<Located<String>>,
    pub span: Option<SourceSpan>,
}
