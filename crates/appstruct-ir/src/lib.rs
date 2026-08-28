//! Stable, serialization-friendly intermediate representation for `AppStruct`.
mod compatibility;
mod extension;
mod seed;
mod service;
mod validation;
mod views;
pub use compatibility::{IrCompatibilityError, from_compatible_json};
pub use extension::{CommandIr, OperationTypeIr, PageIr, QueryIr, ValueFieldIr, ValueObjectIr};
pub use seed::SeedIr;
use serde::{Deserialize, Serialize};
pub use service::{
    AuditIr, FileIr, FileProviderIr, JobQueueIr, JobsIr, MailIr, MailProviderIr, MailTemplateIr,
    PresetIr, TenantIr,
};
use std::fmt;
pub use validation::{IrValidationError, IrValidationErrors, validate_app_ir};
pub use views::EntityViewsIr;
pub const IR_VERSION: u32 = appstruct_contracts::IR.current;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppIr {
    pub ir_version: u32,
    pub app: AppMeta,
    pub database: DatabaseIr,
    pub preset: Option<PresetIr>,
    pub auth: AuthIr,
    pub tenant: TenantIr,
    pub audit: AuditIr,
    pub mail: MailIr,
    pub jobs: JobsIr,
    pub file: FileIr,
    pub enums: Vec<EnumIr>,
    pub value_objects: Vec<ValueObjectIr>,
    pub entities: Vec<EntityIr>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub seeds: Vec<SeedIr>,
    pub relations: Vec<RelationIr>,
    pub commands: Vec<CommandIr>,
    pub queries: Vec<QueryIr>,
    pub pages: Vec<PageIr>,
    pub modules: Vec<ResolvedModule>,
}

/// Application-level metadata.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppMeta {
    pub name: String,
}

/// Database settings relevant to deterministic generation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DatabaseIr {
    pub provider: DatabaseProvider,
    pub dev_mode: DatabaseDevMode,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DatabaseProvider {
    Postgres,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DatabaseDevMode {
    Managed,
    External,
}

/// Authentication facts known at compile time in the M0 compiler.
#[allow(clippy::struct_excessive_bools)]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthIr {
    pub enabled: bool,
    pub user_entity: Option<EntityId>,
    pub registration_enabled: bool,
    pub password_reset_enabled: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub oauth_enabled: bool,
    pub roles: Vec<String>,
    pub default_role: Option<String>,
}

/// Stable logical entity identifier. It never depends on vector position.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct EntityId(pub String);

impl fmt::Display for EntityId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

/// Stable logical field identifier.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct FieldId(pub String);

impl fmt::Display for FieldId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RelationId(pub String);

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityIr {
    pub id: EntityId,
    pub rust_name: String,
    pub api_name: String,
    pub label: String,
    pub table_name: String,
    pub fields: Vec<FieldIr>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub indexes: Vec<IndexIr>,
    pub access: CrudAccessIr,
    pub views: EntityViewsIr,
    pub hooks: HooksIr,
    pub concurrency: ConcurrencyIr,
    pub tenant_scoped: bool,
    pub audit_enabled: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndexIr {
    pub id: String,
    pub entity: EntityId,
    pub fields: Vec<FieldId>,
    pub unique: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub predicate: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FieldIr {
    pub id: FieldId,
    pub entity: EntityId,
    pub rust_name: String,
    pub api_name: String,
    pub column_name: String,
    pub ty: FieldTypeIr,
    pub nullable: bool,
    pub primary_key: bool,
    pub unique: bool,
    pub generated: Option<GeneratedValueIr>,
    pub default: Option<String>,
    pub validation: ValidationIr,
    pub capabilities: FieldCapabilities,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub read_access: Option<AccessRuleIr>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub write_access: Option<AccessRuleIr>,
    pub ui_component: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FieldTypeIr {
    Uuid,
    String,
    Text,
    Integer,
    Bigint,
    Decimal,
    Boolean,
    Date,
    Datetime,
    Json,
    Enum { values: Vec<String> },
    Relation { target: EntityId },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeneratedValueIr {
    UuidV7,
    Now,
    AutoIncrement,
    Revision,
    Tenant,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValidationIr {
    pub min_length: Option<u64>,
    pub max_length: Option<u64>,
    pub minimum: Option<String>,
    pub maximum: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FieldCapabilities {
    pub searchable: bool,
    pub filterable: bool,
    pub sortable: bool,
}

/// Explicit relation edge resolved by the compiler.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RelationIr {
    pub id: RelationId,
    pub source: EntityId,
    pub target: EntityId,
    pub cardinality: Cardinality,
    pub foreign_key_owner: EntityId,
    pub foreign_key_fields: Vec<FieldId>,
    pub inverse: Option<RelationId>,
    pub required: bool,
    pub unique: bool,
    pub on_delete: OnDeleteIr,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Cardinality {
    OneToOne,
    ManyToOne,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OnDeleteIr {
    Restrict,
    Cascade,
    SetNull,
}

/// CRUD rules are always explicit in normalized IR.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CrudAccessIr {
    pub list: AccessRuleIr,
    pub read: AccessRuleIr,
    pub create: AccessRuleIr,
    pub update: AccessRuleIr,
    pub delete: AccessRuleIr,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum AccessRuleIr {
    Public,
    Authenticated,
    Role { role: String },
    Owner { field: FieldId },
    Any { rules: Vec<AccessRuleIr> },
    All { rules: Vec<AccessRuleIr> },
}

/// M0 preserves the future IR shape while leaving hooks empty.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct HooksIr {}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConcurrencyIr {
    pub enabled: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnumIr {
    pub name: String,
    pub values: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedModule {
    pub name: String,
    pub version: String,
    pub origin: ModuleOrigin,
    /// Project-relative manifest path for a local module.
    pub manifest_path: Option<String>,
    /// SHA-256 of the exact local manifest bytes loaded by the compiler.
    pub content_sha256: Option<String>,
    pub provides: Vec<String>,
    pub requires: Vec<String>,
    pub startup_order: u32,
    pub artifacts: Vec<ModuleArtifactIr>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModuleOrigin {
    Official,
    Local,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModuleArtifactIr {
    pub path: String,
    /// Project-relative source path, or `None` for migrated legacy IR.
    pub source: Option<String>,
    pub sha256: String,
    pub byte_len: u64,
    pub content: String,
}

/// Byte offsets plus user-facing line and column for a source range.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceSpan {
    pub file: String,
    pub start: usize,
    pub end: usize,
    pub line: usize,
    pub column: usize,
    pub end_line: usize,
    pub end_column: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Label {
    pub span: SourceSpan,
    pub message: String,
}

/// Stable diagnostic shared by text and JSON consumers.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diagnostic {
    pub severity: Severity,
    pub code: String,
    pub message: String,
    pub primary: Box<Label>,
    pub secondary: Vec<Label>,
    pub help: Option<String>,
}

impl Diagnostic {
    #[must_use]
    pub fn error(code: &str, message: impl Into<String>, span: SourceSpan) -> Self {
        Self {
            severity: Severity::Error,
            code: code.to_owned(),
            message: message.into(),
            primary: Box::new(Label {
                span,
                message: String::new(),
            }),
            secondary: Vec::new(),
            help: None,
        }
    }

    #[must_use]
    pub fn warning(code: &str, message: impl Into<String>, span: SourceSpan) -> Self {
        Self {
            severity: Severity::Warning,
            code: code.to_owned(),
            message: message.into(),
            primary: Box::new(Label {
                span,
                message: String::new(),
            }),
            secondary: Vec::new(),
            help: None,
        }
    }

    #[must_use]
    pub fn with_primary_message(mut self, message: impl Into<String>) -> Self {
        self.primary.message = message.into();
        self
    }

    #[must_use]
    pub fn with_secondary(mut self, span: SourceSpan, message: impl Into<String>) -> Self {
        self.secondary.push(Label {
            span,
            message: message.into(),
        });
        self
    }

    #[must_use]
    pub fn with_help(mut self, help: impl Into<String>) -> Self {
        self.help = Some(help.into());
        self
    }
}

/// Serialize normalized IR using deterministic field and collection order.
///
/// # Errors
///
/// Returns a serialization error only if the IR representation becomes unsupported by Serde.
pub fn to_canonical_json(ir: &AppIr) -> Result<String, serde_json::Error> {
    let mut output = serde_json::to_string_pretty(ir)?;
    output.push('\n');
    Ok(output)
}
