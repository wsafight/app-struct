use super::{
    SurfaceActivity, SurfaceAudit, SurfaceAuth, SurfaceFile, SurfaceJobs, SurfaceMail,
    SurfaceOperation, SurfacePage, SurfacePreset, SurfaceRealtime, SurfaceReport, SurfaceTenant,
    SurfaceValueObject, SurfaceWebhooks,
};
use crate::yaml::MappingEntry;
use appstruct_ir::SourceSpan;

#[derive(Clone, Debug)]
pub(crate) struct Located<T> {
    pub value: T,
    pub span: SourceSpan,
}

#[derive(Clone, Debug)]
pub(crate) struct SurfaceRoot {
    pub version: Located<u64>,
    pub app_name: Located<String>,
    pub database_provider: Located<String>,
    pub database_mode: Located<String>,
    pub database_migration: Located<String>,
    pub preset: Option<SurfacePreset>,
    pub expanded_modules: Option<MappingEntry>,
    pub auth: SurfaceAuth,
    pub tenant: SurfaceTenant,
    pub audit: SurfaceAudit,
    pub mail: SurfaceMail,
    pub jobs: SurfaceJobs,
    pub webhooks: SurfaceWebhooks,
    pub realtime: SurfaceRealtime,
    pub file: SurfaceFile,
    pub report: SurfaceReport,
    pub activity: SurfaceActivity,
    pub includes: Vec<Located<String>>,
    pub module_manifests: Vec<Located<String>>,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct SurfaceDomain {
    pub entities: Vec<SurfaceEntity>,
    pub value_objects: Vec<SurfaceValueObject>,
    pub commands: Vec<SurfaceOperation>,
    pub queries: Vec<SurfaceOperation>,
    pub pages: Vec<SurfacePage>,
}

impl SurfaceDomain {
    pub(crate) fn extend(&mut self, mut domain: Self) {
        self.entities.append(&mut domain.entities);
        self.value_objects.append(&mut domain.value_objects);
        self.commands.append(&mut domain.commands);
        self.queries.append(&mut domain.queries);
        self.pages.append(&mut domain.pages);
    }
}

#[derive(Clone, Debug)]
pub(crate) struct SurfaceEntity {
    pub name: Located<String>,
    pub label: Option<Located<String>>,
    pub display_field: Option<Located<String>>,
    pub aggregates: Vec<appstruct_ir::AggregateIr>,
    pub table: Option<Located<String>>,
    pub fields: Vec<SurfaceField>,
    pub indexes: Vec<SurfaceIndex>,
    pub seeds: Vec<SurfaceSeed>,
    pub access: Option<SurfaceAccess>,
    pub tenant_scoped: bool,
    pub audit_enabled: bool,
    pub soft_delete: bool,
    pub workflow: Option<SurfaceWorkflow>,
    pub span: SourceSpan,
}

#[derive(Clone, Debug)]
pub(crate) struct SurfaceWorkflow {
    pub field: Located<String>,
    pub initial: Located<String>,
    pub transitions: Vec<SurfaceWorkflowTransition>,
    pub span: SourceSpan,
}

#[derive(Clone, Debug)]
pub(crate) struct SurfaceWorkflowTransition {
    pub name: Located<String>,
    pub from: Vec<Located<String>>,
    pub to: Located<String>,
    pub input: Option<Located<String>>,
    pub access: Located<SurfaceAccessRule>,
    pub span: SourceSpan,
}

#[derive(Clone, Debug)]
pub(crate) struct SurfaceSeed {
    pub name: Located<String>,
    pub values: Vec<(Located<String>, Located<String>)>,
    pub span: SourceSpan,
}

#[derive(Clone, Debug)]
pub(crate) struct SurfaceIndex {
    pub name: Option<Located<String>>,
    pub fields: Vec<Located<String>>,
    pub unique: bool,
    pub where_clause: Option<Located<String>>,
    pub span: SourceSpan,
}

#[derive(Clone, Debug)]
pub(crate) struct SurfaceField {
    pub name: Located<String>,
    pub type_name: Located<String>,
    pub column: Option<Located<String>>,
    pub flags: FieldFlags,
    pub generated: Option<Located<String>>,
    pub default: Option<Located<String>>,
    pub min_length: Option<Located<u64>>,
    pub max_length: Option<Located<u64>>,
    pub minimum: Option<Located<String>>,
    pub maximum: Option<Located<String>>,
    pub values: Option<Vec<Located<String>>>,
    pub target: Option<Located<String>>,
    pub on_delete: Option<Located<String>>,
    pub ui_component: Option<Located<String>>,
    pub ui_semantic: Option<SurfaceFieldSemantic>,
    pub access: Option<SurfaceFieldAccess>,
    pub span: SourceSpan,
}

#[derive(Clone, Debug)]
pub(crate) enum SurfaceFieldSemantic {
    Money {
        currency_field: Located<String>,
        fraction_digits: Located<u64>,
    },
}

#[derive(Clone, Debug)]
pub(crate) struct SurfaceFieldUi {
    pub component: Option<Located<String>>,
    pub semantic: Option<SurfaceFieldSemantic>,
}

#[derive(Clone, Debug)]
pub(crate) struct SurfaceFieldAccess {
    pub read: Option<Located<SurfaceAccessRule>>,
    pub write: Option<Located<SurfaceAccessRule>>,
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct FieldFlags(u8);

impl FieldFlags {
    pub(crate) const PRIMARY_KEY: u8 = 1 << 0;
    pub(crate) const REQUIRED: u8 = 1 << 1;
    pub(crate) const UNIQUE: u8 = 1 << 2;
    pub(crate) const SEARCHABLE: u8 = 1 << 3;
    pub(crate) const FILTERABLE: u8 = 1 << 4;
    pub(crate) const SORTABLE: u8 = 1 << 5;

    pub(crate) fn set(&mut self, flag: u8, enabled: bool) {
        if enabled {
            self.0 |= flag;
        }
    }

    pub fn primary_key(self) -> bool {
        self.0 & Self::PRIMARY_KEY != 0
    }

    pub fn required(self) -> bool {
        self.0 & Self::REQUIRED != 0
    }

    pub fn unique(self) -> bool {
        self.0 & Self::UNIQUE != 0
    }

    pub fn searchable(self) -> bool {
        self.0 & Self::SEARCHABLE != 0
    }

    pub fn filterable(self) -> bool {
        self.0 & Self::FILTERABLE != 0
    }

    pub fn sortable(self) -> bool {
        self.0 & Self::SORTABLE != 0
    }
}

#[derive(Clone, Debug)]
pub(crate) struct SurfaceAccess {
    pub list: Option<Located<SurfaceAccessRule>>,
    pub read: Option<Located<SurfaceAccessRule>>,
    pub create: Option<Located<SurfaceAccessRule>>,
    pub update: Option<Located<SurfaceAccessRule>>,
    pub delete: Option<Located<SurfaceAccessRule>>,
    pub span: SourceSpan,
}

#[derive(Clone, Debug)]
pub(crate) enum SurfaceAccessRule {
    Public,
    Authenticated,
    Role(String),
    Owner(String),
    Any(Vec<Located<SurfaceAccessRule>>),
    All(Vec<Located<SurfaceAccessRule>>),
}
