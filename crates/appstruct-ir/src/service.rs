use serde::{Deserialize, Serialize};

/// Locked official preset selected by the root App Spec.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PresetIr {
    pub name: String,
    pub version: u32,
    pub digest: String,
}

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
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub schedules: Vec<JobScheduleIr>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobQueueIr {
    pub name: String,
    pub max_attempts: u32,
    pub backoff_seconds: u64,
}

/// Declarative recurring job schedule.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobScheduleIr {
    pub name: String,
    pub cron: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interval_seconds: Option<u64>,
    pub queue: String,
    pub kind: String,
    pub payload: String,
}

/// PostgreSQL outbox-backed signed webhook settings.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WebhooksIr {
    pub enabled: bool,
    pub poll_interval_ms: u64,
    #[serde(default = "default_webhook_connect_timeout_ms")]
    pub connect_timeout_ms: u64,
    #[serde(default = "default_webhook_read_timeout_ms")]
    pub read_timeout_ms: u64,
    #[serde(default = "default_webhook_request_timeout_ms")]
    pub request_timeout_ms: u64,
    pub endpoints: Vec<WebhookEndpointIr>,
}

fn default_webhook_connect_timeout_ms() -> u64 {
    3_000
}
fn default_webhook_read_timeout_ms() -> u64 {
    10_000
}
fn default_webhook_request_timeout_ms() -> u64 {
    15_000
}

impl WebhooksIr {
    #[must_use]
    pub fn is_disabled(&self) -> bool {
        !self.enabled && self.endpoints.is_empty()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WebhookEndpointIr {
    pub name: String,
    pub url: String,
    pub secret_env: String,
    pub events: Vec<String>,
    pub max_attempts: u32,
    pub backoff_seconds: u64,
}

/// Authenticated server-sent events and database-backed online presence.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RealtimeIr {
    pub enabled: bool,
    pub heartbeat_seconds: u64,
    pub presence_ttl_seconds: u64,
}

impl RealtimeIr {
    #[must_use]
    pub fn is_disabled(&self) -> bool {
        !self.enabled
    }
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

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReportIr {
    pub enabled: bool,
    pub queue: String,
    pub max_input_bytes: u64,
    pub retention_days: u32,
    pub reader_roles: Vec<String>,
    pub templates: Vec<ReportTemplateIr>,
}

impl ReportIr {
    #[must_use]
    pub fn is_disabled(&self) -> bool {
        !self.enabled && self.templates.is_empty()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReportTemplateIr {
    pub name: String,
    pub version: u32,
    pub document_type: String,
    pub body: String,
    pub artifact_digest: String,
    pub input_schema: String,
    pub data_schema_version: u32,
}

/// Record-scoped collaboration settings and the entities that expose a timeline.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActivityIr {
    pub enabled: bool,
    pub max_comment_bytes: u32,
    pub attachments: bool,
    pub admin_roles: Vec<String>,
    pub resources: Vec<ActivityResourceIr>,
}

impl ActivityIr {
    #[must_use]
    pub fn is_disabled(&self) -> bool {
        !self.enabled && self.resources.is_empty()
    }

    #[must_use]
    pub fn resource_for_entity(&self, entity: &crate::EntityId) -> Option<&ActivityResourceIr> {
        self.resources
            .iter()
            .find(|resource| resource.entity == *entity)
    }

    #[must_use]
    pub fn resource(&self, key: &str) -> Option<&ActivityResourceIr> {
        self.resources
            .iter()
            .find(|resource| resource.resource == key)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActivityResourceIr {
    pub entity: crate::EntityId,
    pub resource: String,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileProviderIr {
    #[default]
    Local,
    S3,
}
