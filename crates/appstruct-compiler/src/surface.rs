mod access;
mod audit;
mod auth;
mod extension;
mod file;
mod jobs;
mod mail;
mod modules;
mod preset;
mod tenant;
mod value;

pub(crate) use extension::{SurfaceOperation, SurfacePage, SurfaceValueField, SurfaceValueObject};
pub(crate) use modules::{
    SurfaceAudit, SurfaceAuth, SurfaceFile, SurfaceJobQueue, SurfaceJobs, SurfaceMail,
    SurfaceMailTemplate, SurfacePreset, SurfaceTenant,
};

use self::value::{
    ensure_known_keys, expect_mapping, expect_scalar_string, expect_sequence, expect_string,
    expect_u64, optional_bool, optional_string, optional_u64, required,
};
use crate::yaml::{MappingEntry, Node};
use appstruct_ir::{Diagnostic, SourceSpan};

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
    pub preset: Option<SurfacePreset>,
    pub auth: SurfaceAuth,
    pub tenant: SurfaceTenant,
    pub audit: SurfaceAudit,
    pub mail: SurfaceMail,
    pub jobs: SurfaceJobs,
    pub file: SurfaceFile,
    pub includes: Vec<Located<String>>,
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
    pub table: Option<Located<String>>,
    pub fields: Vec<SurfaceField>,
    pub access: Option<SurfaceAccess>,
    pub tenant_scoped: bool,
    pub audit_enabled: bool,
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
    pub span: SourceSpan,
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct FieldFlags(u8);

impl FieldFlags {
    const PRIMARY_KEY: u8 = 1 << 0;
    const REQUIRED: u8 = 1 << 1;
    const UNIQUE: u8 = 1 << 2;
    const SEARCHABLE: u8 = 1 << 3;
    const FILTERABLE: u8 = 1 << 4;
    const SORTABLE: u8 = 1 << 5;

    fn set(&mut self, flag: u8, enabled: bool) {
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

pub(crate) fn decode_root(root: &Node) -> Result<SurfaceRoot, Diagnostic> {
    let mapping = expect_mapping(root, "root configuration")?;
    ensure_known_keys(
        mapping,
        &[
            "version", "app", "database", "preset", "modules", "includes",
        ],
        "root configuration",
    )?;
    let version_node = required(mapping, "version", &root.span)?;
    let version = expect_u64(&version_node.value, "`version`")?;

    let app_node = required(mapping, "app", &root.span)?;
    let app = expect_mapping(&app_node.value, "`app`")?;
    ensure_known_keys(app, &["name"], "`app`")?;
    let name_node = required(app, "name", &app_node.value.span)?;
    let app_name = expect_string(&name_node.value, "`app.name`")?;

    let database_config = required(mapping, "database", &root.span)?;
    let database = expect_mapping(&database_config.value, "`database`")?;
    ensure_known_keys(database, &["provider", "dev"], "`database`")?;
    let provider_node = required(database, "provider", &database_config.value.span)?;
    let database_provider = expect_string(&provider_node.value, "`database.provider`")?;
    let dev_mode = if let Some(dev_node) = database.get("dev") {
        let dev = expect_mapping(&dev_node.value, "`database.dev`")?;
        ensure_known_keys(dev, &["mode"], "`database.dev`")?;
        if let Some(mode_node) = dev.get("mode") {
            expect_string(&mode_node.value, "`database.dev.mode`")?
        } else {
            Located {
                value: "managed".to_owned(),
                span: dev_node.value.span.clone(),
            }
        }
    } else {
        Located {
            value: "managed".to_owned(),
            span: database_config.value.span.clone(),
        }
    };

    let includes_node = required(mapping, "includes", &root.span)?;
    let include_nodes = expect_sequence(&includes_node.value, "`includes`")?;
    let includes = include_nodes
        .iter()
        .map(|node| expect_string(node, "include path"))
        .collect::<Result<Vec<_>, _>>()?;

    let preset = preset::decode(mapping.get("preset"))?;
    let modules = crate::preset::expand_modules(preset.as_ref(), mapping.get("modules"))?;
    let auth = auth::decode(modules.as_ref())?;
    let tenant = tenant::decode(modules.as_ref())?;
    let audit = audit::decode(modules.as_ref())?;
    let mail = mail::decode(modules.as_ref())?;
    let jobs = jobs::decode(modules.as_ref())?;
    let file = file::decode(modules.as_ref())?;

    Ok(SurfaceRoot {
        version,
        app_name,
        database_provider,
        database_mode: dev_mode,
        preset,
        auth,
        tenant,
        audit,
        mail,
        jobs,
        file,
        includes,
    })
}

pub(crate) fn decode_domain(root: &Node) -> Result<SurfaceDomain, Diagnostic> {
    let mapping = expect_mapping(root, "domain configuration")?;
    ensure_known_keys(
        mapping,
        &[
            "domain",
            "entities",
            "value_objects",
            "commands",
            "queries",
            "pages",
            "includes",
        ],
        "domain configuration",
    )?;
    if let Some(includes) = mapping.get("includes") {
        return Err(Diagnostic::error(
            "AS1006",
            "domain files cannot include other files",
            includes.key_span.clone(),
        )
        .with_help("list every domain file in the root `appstruct.yaml`"));
    }

    let domain_node = required(mapping, "domain", &root.span)?;
    let _domain = expect_string(&domain_node.value, "`domain`")?;
    let mut entities = Vec::new();
    if let Some(entities_node) = mapping.get("entities") {
        let entities_mapping = expect_mapping(&entities_node.value, "`entities`")?;
        entities.reserve(entities_mapping.len());
        for (name, entry) in entities_mapping {
            entities.push(decode_entity(name, entry)?);
        }
    }
    Ok(SurfaceDomain {
        entities,
        value_objects: extension::decode_value_objects(mapping)?,
        commands: extension::decode_operations(mapping, "commands")?,
        queries: extension::decode_operations(mapping, "queries")?,
        pages: extension::decode_pages(mapping)?,
    })
}

fn decode_entity(name: &str, entry: &MappingEntry) -> Result<SurfaceEntity, Diagnostic> {
    let mapping = expect_mapping(&entry.value, "entity definition")?;
    ensure_known_keys(
        mapping,
        &["label", "table", "fields", "access", "tenant", "audit"],
        "entity definition",
    )?;
    let fields_node = required(mapping, "fields", &entry.value.span)?;
    let fields_mapping = expect_mapping(&fields_node.value, "entity `fields`")?;
    let mut fields = Vec::with_capacity(fields_mapping.len());
    for (field_name, field_entry) in fields_mapping {
        fields.push(decode_field(field_name, field_entry)?);
    }

    Ok(SurfaceEntity {
        name: Located {
            value: name.to_owned(),
            span: entry.key_span.clone(),
        },
        label: optional_string(mapping, "label", "entity `label`")?,
        table: optional_string(mapping, "table", "entity `table`")?,
        fields,
        access: mapping
            .get("access")
            .map(|access| access::decode_crud_access(&access.value))
            .transpose()?,
        tenant_scoped: optional_bool(mapping, "tenant")?,
        audit_enabled: optional_bool(mapping, "audit")?,
        span: entry.value.span.clone(),
    })
}

fn decode_field(name: &str, entry: &MappingEntry) -> Result<SurfaceField, Diagnostic> {
    let mapping = expect_mapping(&entry.value, "field definition")?;
    ensure_known_keys(
        mapping,
        &[
            "type",
            "column",
            "primary_key",
            "required",
            "unique",
            "generated",
            "default",
            "min_length",
            "max_length",
            "minimum",
            "maximum",
            "searchable",
            "filterable",
            "sortable",
            "values",
            "target",
            "on_delete",
            "ui",
        ],
        "field definition in M0",
    )?;
    let type_node = required(mapping, "type", &entry.value.span)?;
    let type_name = expect_string(&type_node.value, "field `type`")?;

    let mut flags = FieldFlags::default();
    flags.set(
        FieldFlags::PRIMARY_KEY,
        optional_bool(mapping, "primary_key")?,
    );
    flags.set(FieldFlags::REQUIRED, optional_bool(mapping, "required")?);
    flags.set(FieldFlags::UNIQUE, optional_bool(mapping, "unique")?);
    flags.set(
        FieldFlags::SEARCHABLE,
        optional_bool(mapping, "searchable")?,
    );
    flags.set(
        FieldFlags::FILTERABLE,
        optional_bool(mapping, "filterable")?,
    );
    flags.set(FieldFlags::SORTABLE, optional_bool(mapping, "sortable")?);

    Ok(SurfaceField {
        name: Located {
            value: name.to_owned(),
            span: entry.key_span.clone(),
        },
        type_name,
        column: optional_string(mapping, "column", "field `column`")?,
        flags,
        generated: optional_string(mapping, "generated", "field `generated`")?,
        default: mapping
            .get("default")
            .map(|default| expect_scalar_string(&default.value, "field `default`"))
            .transpose()?,
        min_length: optional_u64(mapping, "min_length")?,
        max_length: optional_u64(mapping, "max_length")?,
        minimum: mapping
            .get("minimum")
            .map(|value| expect_scalar_string(&value.value, "field `minimum`"))
            .transpose()?,
        maximum: mapping
            .get("maximum")
            .map(|value| expect_scalar_string(&value.value, "field `maximum`"))
            .transpose()?,
        values: mapping
            .get("values")
            .map(|values| {
                expect_sequence(&values.value, "enum `values`")?
                    .iter()
                    .map(|value| expect_string(value, "enum value"))
                    .collect()
            })
            .transpose()?,
        target: optional_string(mapping, "target", "relation `target`")?,
        on_delete: optional_string(mapping, "on_delete", "relation `on_delete`")?,
        ui_component: mapping
            .get("ui")
            .map(|ui| extension::decode_ui_component(&ui.value))
            .transpose()?,
        span: entry.value.span.clone(),
    })
}
