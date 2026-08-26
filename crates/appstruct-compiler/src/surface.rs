mod value;

use self::value::{
    ensure_known_keys, expect_bool, expect_mapping, expect_scalar_string, expect_sequence,
    expect_string, expect_u64, optional_bool, optional_string, optional_u64, required,
};
use crate::yaml::{MappingEntry, Node};
use appstruct_ir::{Diagnostic, SourceSpan};
use std::collections::BTreeMap;

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
    pub auth_enabled: bool,
    pub includes: Vec<Located<String>>,
}

#[derive(Clone, Debug)]
pub(crate) struct SurfaceDomain {
    pub entities: Vec<SurfaceEntity>,
}

#[derive(Clone, Debug)]
pub(crate) struct SurfaceEntity {
    pub name: Located<String>,
    pub label: Option<Located<String>>,
    pub table: Option<Located<String>>,
    pub fields: Vec<SurfaceField>,
    pub access: Option<SurfaceAccess>,
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
    Role(String),
}

pub(crate) fn decode_root(root: &Node) -> Result<SurfaceRoot, Diagnostic> {
    let mapping = expect_mapping(root, "root configuration")?;
    ensure_known_keys(
        mapping,
        &["version", "app", "database", "modules", "includes"],
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

    let auth_enabled = if let Some(modules_node) = mapping.get("modules") {
        let modules = expect_mapping(&modules_node.value, "`modules`")?;
        ensure_known_keys(modules, &["auth"], "`modules` in M0")?;
        if let Some(auth_node) = modules.get("auth") {
            let auth = expect_mapping(&auth_node.value, "`modules.auth`")?;
            ensure_known_keys(auth, &["enabled"], "`modules.auth`")?;
            auth.get("enabled")
                .map(|enabled| expect_bool(&enabled.value, "`modules.auth.enabled`"))
                .transpose()?
                .unwrap_or(false)
        } else {
            false
        }
    } else {
        false
    };

    Ok(SurfaceRoot {
        version,
        app_name,
        database_provider,
        database_mode: dev_mode,
        auth_enabled,
        includes,
    })
}

pub(crate) fn decode_domain(root: &Node) -> Result<SurfaceDomain, Diagnostic> {
    let mapping = expect_mapping(root, "domain configuration")?;
    ensure_known_keys(
        mapping,
        &["domain", "entities", "includes"],
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
    let entities_node = required(mapping, "entities", &root.span)?;
    let entities_mapping = expect_mapping(&entities_node.value, "`entities`")?;
    let mut entities = Vec::with_capacity(entities_mapping.len());

    for (name, entry) in entities_mapping {
        entities.push(decode_entity(name, entry)?);
    }
    Ok(SurfaceDomain { entities })
}

fn decode_entity(name: &str, entry: &MappingEntry) -> Result<SurfaceEntity, Diagnostic> {
    let mapping = expect_mapping(&entry.value, "entity definition")?;
    ensure_known_keys(
        mapping,
        &["label", "table", "fields", "access"],
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
            .map(|access| decode_access(&access.value))
            .transpose()?,
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
        span: entry.value.span.clone(),
    })
}

fn decode_access(node: &Node) -> Result<SurfaceAccess, Diagnostic> {
    let mapping = expect_mapping(node, "entity `access`")?;
    ensure_known_keys(
        mapping,
        &["list", "read", "create", "update", "delete"],
        "entity `access`",
    )?;
    Ok(SurfaceAccess {
        list: optional_access_rule(mapping, "list")?,
        read: optional_access_rule(mapping, "read")?,
        create: optional_access_rule(mapping, "create")?,
        update: optional_access_rule(mapping, "update")?,
        delete: optional_access_rule(mapping, "delete")?,
        span: node.span.clone(),
    })
}

fn optional_access_rule(
    mapping: &BTreeMap<String, MappingEntry>,
    operation: &str,
) -> Result<Option<Located<SurfaceAccessRule>>, Diagnostic> {
    mapping
        .get(operation)
        .map(|entry| {
            let rule = expect_mapping(&entry.value, "access rule")?;
            ensure_known_keys(rule, &["role", "public"], "access rule")?;
            let value = if let Some(role) = rule.get("role") {
                SurfaceAccessRule::Role(expect_string(&role.value, "access `role`")?.value)
            } else if let Some(public) = rule.get("public") {
                if !expect_bool(&public.value, "access `public`")? {
                    return Err(Diagnostic::error(
                        "AS1007",
                        "`public` must be true when present",
                        public.value.span.clone(),
                    ));
                }
                SurfaceAccessRule::Public
            } else {
                return Err(Diagnostic::error(
                    "AS1007",
                    "access rule requires `role` or `public: true`",
                    entry.value.span.clone(),
                ));
            };
            Ok(Located {
                value,
                span: entry.value.span.clone(),
            })
        })
        .transpose()
}
