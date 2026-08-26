use serde::{Deserialize, Serialize};

/// Tenant module settings normalized by the compiler.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TenantIr {
    pub enabled: bool,
}

/// Audit module settings normalized by the compiler.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditIr {
    pub enabled: bool,
    pub reader_roles: Vec<String>,
}

/// Mail module settings and compile-time validated templates.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MailIr {
    pub enabled: bool,
    pub provider: MailProviderIr,
    pub from: String,
    pub templates: Vec<MailTemplateIr>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MailProviderIr {
    #[default]
    Capture,
    Smtp,
    Resend,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MailTemplateIr {
    pub name: String,
    pub subject: String,
    pub text: String,
    pub html: Option<String>,
}

/// PostgreSQL-backed job worker settings.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobsIr {
    pub enabled: bool,
    pub poll_interval_ms: u64,
    pub lease_seconds: u64,
    pub queues: Vec<JobQueueIr>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobQueueIr {
    pub name: String,
    pub max_attempts: u32,
    pub backoff_seconds: u64,
}

/// File metadata and object-storage settings.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileIr {
    pub enabled: bool,
    pub provider: FileProviderIr,
    pub local_root: String,
    pub max_bytes: u64,
    pub allowed_content_types: Vec<String>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileProviderIr {
    #[default]
    Local,
    S3,
}
