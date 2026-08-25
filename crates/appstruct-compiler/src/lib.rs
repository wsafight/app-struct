//! `AppStruct` configuration loading, validation, and normalization.

mod surface;
mod yaml;

use appstruct_ir::{
    AccessRuleIr, AppIr, AppMeta, AuthIr, Cardinality, ConcurrencyIr, CrudAccessIr,
    DatabaseDevMode, DatabaseIr, DatabaseProvider, Diagnostic, EntityId, EntityIr, EntityViewsIr,
    FieldCapabilities, FieldId, FieldIr, FieldTypeIr, GeneratedValueIr, HooksIr, IR_VERSION,
    OnDeleteIr, RelationId, RelationIr, SourceSpan, ValidationIr,
};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use surface::{
    Located, SurfaceAccess, SurfaceAccessRule, SurfaceEntity, SurfaceField, SurfaceRoot,
};

/// Find the nearest `AppStruct` project at or above `start`.
///
/// # Errors
///
/// Returns a diagnostic if the start path cannot be resolved or no `appstruct.yaml` exists.
pub fn discover_project(start: &Path) -> Result<PathBuf, Diagnostic> {
    let start = fs::canonicalize(start).map_err(|error| {
        Diagnostic::error(
            "AS1008",
            format!("cannot access project path `{}`: {error}", start.display()),
            synthetic_span(&start.to_string_lossy()),
        )
    })?;
    let mut directory = if start.is_file() {
        start.parent().map(Path::to_path_buf)
    } else {
        Some(start)
    };

    while let Some(candidate) = directory {
        if candidate.join("appstruct.yaml").is_file() {
            return Ok(candidate);
        }
        directory = candidate.parent().map(Path::to_path_buf);
    }

    Err(Diagnostic::error(
        "AS1008",
        "could not find `appstruct.yaml` in this directory or any parent",
        synthetic_span("appstruct.yaml"),
    ))
}

/// Compile a project root into normalized, deterministically ordered IR.
///
/// # Errors
///
/// Returns all diagnostics found during path resolution and semantic validation. YAML shape errors
/// are reported per source before semantic validation starts.
pub fn compile_project(project_root: &Path) -> Result<AppIr, Vec<Diagnostic>> {
    let root = match fs::canonicalize(project_root) {
        Ok(path) => path,
        Err(error) => {
            return Err(vec![Diagnostic::error(
                "AS1008",
                format!(
                    "cannot access project root `{}`: {error}",
                    project_root.display()
                ),
                synthetic_span(&project_root.to_string_lossy()),
            )]);
        }
    };
    let root_file = root.join("appstruct.yaml");
    let root_node = load_yaml(&root, &root_file)?;
    let surface_root = surface::decode_root(&root_node).map_err(|error| vec![error])?;

    let mut diagnostics = validate_root(&surface_root);
    let mut canonical_includes = BTreeMap::<PathBuf, SourceSpan>::new();
    let mut entities = Vec::new();

    for include in &surface_root.includes {
        let include_path = match resolve_include(&root, include) {
            Ok(path) => path,
            Err(error) => {
                diagnostics.push(error);
                continue;
            }
        };
        if let Some(first_span) =
            canonical_includes.insert(include_path.clone(), include.span.clone())
        {
            diagnostics.push(
                Diagnostic::error(
                    "AS1009",
                    format!("duplicate include `{}`", include.value),
                    include.span.clone(),
                )
                .with_secondary(first_span, "first included here"),
            );
            continue;
        }

        match load_yaml(&root, &include_path)
            .and_then(|node| surface::decode_domain(&node).map_err(|error| vec![error]))
        {
            Ok(domain) => entities.extend(domain.entities),
            Err(mut errors) => diagnostics.append(&mut errors),
        }
    }

    if !diagnostics.is_empty() {
        return Err(diagnostics);
    }
    build_ir(surface_root, entities)
}

fn load_yaml(root: &Path, path: &Path) -> Result<yaml::Node, Vec<Diagnostic>> {
    let label = relative_label(root, path);
    let source = fs::read_to_string(path).map_err(|error| {
        vec![Diagnostic::error(
            "AS1008",
            format!("cannot read `{label}`: {error}"),
            synthetic_span(&label),
        )]
    })?;
    yaml::parse(&label, &source).map_err(|error| vec![error])
}

fn resolve_include(root: &Path, include: &Located<String>) -> Result<PathBuf, Diagnostic> {
    let declared = Path::new(&include.value);
    if declared.is_absolute() {
        return Err(Diagnostic::error(
            "AS1010",
            "include paths must be relative to the project root",
            include.span.clone(),
        ));
    }
    let candidate = root.join(declared);
    let canonical = fs::canonicalize(&candidate).map_err(|error| {
        Diagnostic::error(
            "AS1008",
            format!("cannot access include `{}`: {error}", include.value),
            include.span.clone(),
        )
    })?;
    if !canonical.starts_with(root) {
        return Err(Diagnostic::error(
            "AS1010",
            format!("include `{}` escapes the project root", include.value),
            include.span.clone(),
        ));
    }
    if !canonical.is_file() {
        return Err(Diagnostic::error(
            "AS1008",
            format!("include `{}` is not a file", include.value),
            include.span.clone(),
        ));
    }
    Ok(canonical)
}

fn validate_root(root: &SurfaceRoot) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    if root.version.value != 1 {
        diagnostics.push(
            Diagnostic::error(
                "AS1011",
                format!("unsupported App Spec version `{}`", root.version.value),
                root.version.span.clone(),
            )
            .with_help("this compiler supports App Spec version 1"),
        );
    }
    if !is_app_name(&root.app_name.value) {
        diagnostics.push(Diagnostic::error(
            "AS2001",
            "app name must start with a lowercase ASCII letter and contain only lowercase letters, digits, or hyphens",
            root.app_name.span.clone(),
        ));
    }
    if root.database_provider.value != "postgres" {
        diagnostics.push(
            Diagnostic::error(
                "AS4001",
                format!(
                    "unsupported database provider `{}`",
                    root.database_provider.value
                ),
                root.database_provider.span.clone(),
            )
            .with_help("M0 supports only `postgres`"),
        );
    }
    if !matches!(root.database_mode.value.as_str(), "managed" | "external") {
        diagnostics.push(Diagnostic::error(
            "AS4002",
            format!(
                "unsupported database dev mode `{}`",
                root.database_mode.value
            ),
            root.database_mode.span.clone(),
        ));
    }
    diagnostics
}

fn build_ir(
    root: SurfaceRoot,
    mut surface_entities: Vec<SurfaceEntity>,
) -> Result<AppIr, Vec<Diagnostic>> {
    surface_entities.sort_by(|left, right| left.name.value.cmp(&right.name.value));
    let mut diagnostics = validate_entity_declarations(&surface_entities);
    if !diagnostics.is_empty() {
        return Err(diagnostics);
    }

    let known_entities = surface_entities
        .iter()
        .map(|entity| entity.name.value.clone())
        .collect::<BTreeSet<_>>();
    let (mut entities, mut relations) =
        lower_entities(surface_entities, &known_entities, &mut diagnostics);
    if !diagnostics.is_empty() {
        return Err(diagnostics);
    }

    for entity in &mut entities {
        entity.fields.sort_by(|left, right| left.id.cmp(&right.id));
    }
    entities.sort_by(|left, right| left.id.cmp(&right.id));
    relations.sort_by(|left, right| left.id.cmp(&right.id));

    Ok(AppIr {
        ir_version: IR_VERSION,
        app: AppMeta {
            name: root.app_name.value,
        },
        database: DatabaseIr {
            provider: DatabaseProvider::Postgres,
            dev_mode: match root.database_mode.value.as_str() {
                "external" => DatabaseDevMode::External,
                _ => DatabaseDevMode::Managed,
            },
        },
        auth: AuthIr {
            enabled: root.auth_enabled,
        },
        enums: Vec::new(),
        value_objects: Vec::new(),
        entities,
        relations,
        commands: Vec::new(),
        queries: Vec::new(),
        pages: Vec::new(),
        modules: Vec::new(),
    })
}

fn validate_entity_declarations(surface_entities: &[SurfaceEntity]) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut entity_declarations = BTreeMap::<String, SourceSpan>::new();
    let mut table_declarations = BTreeMap::<String, SourceSpan>::new();

    for entity in surface_entities {
        if !is_rust_type_name(&entity.name.value) {
            diagnostics.push(Diagnostic::error(
                "AS2001",
                format!("invalid entity name `{}`", entity.name.value),
                entity.name.span.clone(),
            ));
        }
        if let Some(first) =
            entity_declarations.insert(entity.name.value.clone(), entity.name.span.clone())
        {
            diagnostics.push(
                Diagnostic::error(
                    "AS2002",
                    format!("entity `{}` is declared more than once", entity.name.value),
                    entity.name.span.clone(),
                )
                .with_secondary(first, "first declared here"),
            );
        }

        let table_name = entity.table.as_ref().map_or_else(
            || pluralize(&to_snake_case(&entity.name.value)),
            |table| table.value.clone(),
        );
        let table_span = entity
            .table
            .as_ref()
            .map_or_else(|| entity.name.span.clone(), |table| table.span.clone());
        if !is_sql_name(&table_name) {
            diagnostics.push(Diagnostic::error(
                "AS2001",
                format!("invalid table name `{table_name}`"),
                table_span.clone(),
            ));
        }
        if let Some(first) = table_declarations.insert(table_name.clone(), table_span.clone()) {
            diagnostics.push(
                Diagnostic::error(
                    "AS2003",
                    format!("table `{table_name}` is used by multiple entities"),
                    table_span,
                )
                .with_secondary(first, "first used here"),
            );
        }
    }
    diagnostics
}

fn lower_entities(
    surface_entities: Vec<SurfaceEntity>,
    known_entities: &BTreeSet<String>,
    diagnostics: &mut Vec<Diagnostic>,
) -> (Vec<EntityIr>, Vec<RelationIr>) {
    let mut entities = Vec::with_capacity(surface_entities.len());
    let mut relations = Vec::new();

    for entity in surface_entities {
        let entity_id = EntityId(format!("app::{}", entity.name.value));
        let table_name = entity.table.as_ref().map_or_else(
            || pluralize(&to_snake_case(&entity.name.value)),
            |table| table.value.clone(),
        );
        let access = build_access(&entity, diagnostics);
        let (fields, mut entity_relations) =
            build_fields(&entity, &entity_id, known_entities, diagnostics);
        relations.append(&mut entity_relations);

        if let Some(access) = access {
            entities.push(EntityIr {
                id: entity_id,
                rust_name: entity.name.value.clone(),
                api_name: entity.name.value.clone(),
                label: entity
                    .label
                    .map_or_else(|| entity.name.value.clone(), |label| label.value),
                table_name,
                fields,
                access,
                views: EntityViewsIr::default(),
                hooks: HooksIr::default(),
                concurrency: ConcurrencyIr { enabled: false },
            });
        }
    }
    (entities, relations)
}

fn build_fields(
    entity: &SurfaceEntity,
    entity_id: &EntityId,
    known_entities: &BTreeSet<String>,
    diagnostics: &mut Vec<Diagnostic>,
) -> (Vec<FieldIr>, Vec<RelationIr>) {
    let mut fields = Vec::with_capacity(entity.fields.len());
    let mut relations = Vec::new();
    let mut columns = BTreeMap::<String, SourceSpan>::new();
    validate_primary_key(entity, diagnostics);

    for field in &entity.fields {
        if let Some((field_ir, relation)) =
            build_field(field, entity_id, known_entities, &mut columns, diagnostics)
        {
            fields.push(field_ir);
            relations.extend(relation);
        }
    }

    (fields, relations)
}

fn validate_primary_key(entity: &SurfaceEntity, diagnostics: &mut Vec<Diagnostic>) {
    let primary_key_count = entity
        .fields
        .iter()
        .filter(|field| field.flags.primary_key())
        .count();
    if primary_key_count != 1 {
        diagnostics.push(Diagnostic::error(
            "AS2004",
            format!(
                "entity `{}` must define exactly one primary key; found {primary_key_count}",
                entity.name.value
            ),
            entity.span.clone(),
        ));
    }
}

fn build_field(
    field: &SurfaceField,
    entity_id: &EntityId,
    known_entities: &BTreeSet<String>,
    columns: &mut BTreeMap<String, SourceSpan>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<(FieldIr, Option<RelationIr>)> {
    let (column_name, is_relation) = build_column(field, columns, diagnostics)?;
    let field_type = build_field_type(field, known_entities, diagnostics)?;
    validate_field_options(field, &field_type, diagnostics);
    let generated = build_generated(field, &field_type, diagnostics);
    let field_id = FieldId(format!("{entity_id}.{}", field.name.value));
    let relation = build_relation(field, entity_id, &field_id, &field_type, diagnostics);
    let nullable = !(field.flags.required() || field.flags.primary_key() || generated.is_some());

    let normalized_field = FieldIr {
        id: field_id,
        entity: entity_id.clone(),
        rust_name: if is_relation {
            format!("{}_id", field.name.value)
        } else {
            field.name.value.clone()
        },
        api_name: field.name.value.clone(),
        column_name,
        ty: field_type,
        nullable,
        primary_key: field.flags.primary_key(),
        generated,
        default: field.default.as_ref().map(|value| value.value.clone()),
        validation: ValidationIr {
            min_length: field.min_length.as_ref().map(|value| value.value),
            max_length: field.max_length.as_ref().map(|value| value.value),
        },
        capabilities: FieldCapabilities {
            searchable: field.flags.searchable(),
            filterable: field.flags.filterable(),
            sortable: field.flags.sortable(),
        },
    };
    Some((normalized_field, relation))
}

fn build_column(
    field: &SurfaceField,
    columns: &mut BTreeMap<String, SourceSpan>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<(String, bool)> {
    if !is_rust_field_name(&field.name.value) {
        diagnostics.push(Diagnostic::error(
            "AS2001",
            format!("invalid field name `{}`", field.name.value),
            field.name.span.clone(),
        ));
        return None;
    }
    let is_relation = field.type_name.value == "relation";
    let default_column = if is_relation {
        format!("{}_id", field.name.value)
    } else {
        field.name.value.clone()
    };
    let column_name = field
        .column
        .as_ref()
        .map_or(default_column, |column| column.value.clone());
    let column_span = field
        .column
        .as_ref()
        .map_or_else(|| field.name.span.clone(), |column| column.span.clone());
    if !is_sql_name(&column_name) {
        diagnostics.push(Diagnostic::error(
            "AS2001",
            format!("invalid column name `{column_name}`"),
            column_span.clone(),
        ));
    }
    if let Some(first) = columns.insert(column_name.clone(), column_span.clone()) {
        diagnostics.push(
            Diagnostic::error(
                "AS2005",
                format!("column `{column_name}` is used by multiple fields"),
                column_span,
            )
            .with_secondary(first, "first used here"),
        );
    }
    Some((column_name, is_relation))
}

fn build_relation(
    field: &SurfaceField,
    entity_id: &EntityId,
    field_id: &FieldId,
    field_type: &FieldTypeIr,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<RelationIr> {
    let FieldTypeIr::Relation { target } = field_type else {
        return None;
    };
    let on_delete = build_on_delete(field, diagnostics);
    if field.flags.required() && on_delete == OnDeleteIr::SetNull {
        diagnostics.push(Diagnostic::error(
            "AS2006",
            "a required relation cannot use `on_delete: set_null`",
            field
                .on_delete
                .as_ref()
                .map_or_else(|| field.span.clone(), |value| value.span.clone()),
        ));
    }
    Some(RelationIr {
        id: RelationId(field_id.0.clone()),
        source: entity_id.clone(),
        target: target.clone(),
        cardinality: if field.flags.unique() {
            Cardinality::OneToOne
        } else {
            Cardinality::ManyToOne
        },
        foreign_key_owner: entity_id.clone(),
        foreign_key_fields: vec![field_id.clone()],
        inverse: None,
        required: field.flags.required(),
        unique: field.flags.unique(),
        on_delete,
    })
}

fn build_field_type(
    field: &SurfaceField,
    known_entities: &BTreeSet<String>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<FieldTypeIr> {
    let scalar = match field.type_name.value.as_str() {
        "uuid" => Some(FieldTypeIr::Uuid),
        "string" => Some(FieldTypeIr::String),
        "text" => Some(FieldTypeIr::Text),
        "integer" => Some(FieldTypeIr::Integer),
        "bigint" => Some(FieldTypeIr::Bigint),
        "decimal" => Some(FieldTypeIr::Decimal),
        "boolean" => Some(FieldTypeIr::Boolean),
        "date" => Some(FieldTypeIr::Date),
        "datetime" => Some(FieldTypeIr::Datetime),
        "json" => Some(FieldTypeIr::Json),
        "enum" | "relation" => None,
        other => {
            diagnostics.push(
                Diagnostic::error(
                    "AS2007",
                    format!("unsupported field type `{other}`"),
                    field.type_name.span.clone(),
                )
                .with_help("use uuid, string, text, integer, bigint, decimal, boolean, date, datetime, json, enum, or relation"),
            );
            return None;
        }
    };
    if let Some(scalar) = scalar {
        return Some(scalar);
    }

    if field.type_name.value == "enum" {
        let Some(values) = &field.values else {
            diagnostics.push(Diagnostic::error(
                "AS2008",
                "enum fields require a non-empty `values` sequence",
                field.span.clone(),
            ));
            return None;
        };
        if values.is_empty() {
            diagnostics.push(Diagnostic::error(
                "AS2008",
                "enum fields require a non-empty `values` sequence",
                field.span.clone(),
            ));
            return None;
        }
        let mut unique = BTreeSet::new();
        for value in values {
            if !unique.insert(value.value.clone()) {
                diagnostics.push(Diagnostic::error(
                    "AS2009",
                    format!("duplicate enum value `{}`", value.value),
                    value.span.clone(),
                ));
            }
        }
        return Some(FieldTypeIr::Enum {
            values: values.iter().map(|value| value.value.clone()).collect(),
        });
    }

    let Some(target) = &field.target else {
        diagnostics.push(Diagnostic::error(
            "AS2010",
            "relation fields require `target`",
            field.span.clone(),
        ));
        return None;
    };
    let target_name = target.value.strip_prefix("app::").unwrap_or(&target.value);
    if target.value.contains("::") && !target.value.starts_with("app::") {
        diagnostics.push(
            Diagnostic::error(
                "AS2011",
                format!(
                    "module relation target `{}` is not available in M0",
                    target.value
                ),
                target.span.clone(),
            )
            .with_help("use an application entity or wait for module IR fragments"),
        );
        return None;
    }
    if !known_entities.contains(target_name) {
        diagnostics.push(Diagnostic::error(
            "AS2011",
            format!("unknown relation target `{}`", target.value),
            target.span.clone(),
        ));
        return None;
    }
    Some(FieldTypeIr::Relation {
        target: EntityId(format!("app::{target_name}")),
    })
}

fn validate_field_options(
    field: &SurfaceField,
    field_type: &FieldTypeIr,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let is_text = matches!(field_type, FieldTypeIr::String | FieldTypeIr::Text);
    if !is_text && (field.min_length.is_some() || field.max_length.is_some()) {
        diagnostics.push(Diagnostic::error(
            "AS2012",
            "`min_length` and `max_length` are valid only for string or text fields",
            field.span.clone(),
        ));
    }
    if let (Some(minimum), Some(maximum)) = (&field.min_length, &field.max_length)
        && minimum.value > maximum.value
    {
        diagnostics.push(
            Diagnostic::error(
                "AS2013",
                "`min_length` cannot be greater than `max_length`",
                minimum.span.clone(),
            )
            .with_secondary(maximum.span.clone(), "maximum declared here"),
        );
    }
    if !matches!(field_type, FieldTypeIr::Enum { .. }) && field.values.is_some() {
        diagnostics.push(Diagnostic::error(
            "AS2012",
            "`values` is valid only for enum fields",
            field.span.clone(),
        ));
    }
    if !matches!(field_type, FieldTypeIr::Relation { .. })
        && (field.target.is_some() || field.on_delete.is_some())
    {
        diagnostics.push(Diagnostic::error(
            "AS2012",
            "`target` and `on_delete` are valid only for relation fields",
            field.span.clone(),
        ));
    }
    if let (Some(default), FieldTypeIr::Enum { values }) = (&field.default, field_type)
        && !values.contains(&default.value)
    {
        diagnostics.push(Diagnostic::error(
            "AS2014",
            format!(
                "enum default `{}` is not one of its declared values",
                default.value
            ),
            default.span.clone(),
        ));
    }
    if field.flags.primary_key()
        && !matches!(
            field_type,
            FieldTypeIr::Uuid | FieldTypeIr::Integer | FieldTypeIr::Bigint
        )
    {
        diagnostics.push(Diagnostic::error(
            "AS2012",
            "primary keys must use uuid, integer, or bigint",
            field.span.clone(),
        ));
    }
}

fn build_generated(
    field: &SurfaceField,
    field_type: &FieldTypeIr,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<GeneratedValueIr> {
    let generated = field.generated.as_ref()?;
    let value = match generated.value.as_str() {
        "uuid_v7" if matches!(field_type, FieldTypeIr::Uuid) => GeneratedValueIr::UuidV7,
        "now" if matches!(field_type, FieldTypeIr::Date | FieldTypeIr::Datetime) => {
            GeneratedValueIr::Now
        }
        "auto_increment" if matches!(field_type, FieldTypeIr::Integer | FieldTypeIr::Bigint) => {
            GeneratedValueIr::AutoIncrement
        }
        _ => {
            diagnostics.push(Diagnostic::error(
                "AS2015",
                format!(
                    "generated value `{}` is incompatible with field type `{}`",
                    generated.value, field.type_name.value
                ),
                generated.span.clone(),
            ));
            return None;
        }
    };
    Some(value)
}

fn build_on_delete(field: &SurfaceField, diagnostics: &mut Vec<Diagnostic>) -> OnDeleteIr {
    let Some(on_delete) = &field.on_delete else {
        return OnDeleteIr::Restrict;
    };
    match on_delete.value.as_str() {
        "restrict" => OnDeleteIr::Restrict,
        "cascade" => OnDeleteIr::Cascade,
        "set_null" => OnDeleteIr::SetNull,
        _ => {
            diagnostics.push(Diagnostic::error(
                "AS2016",
                format!("unknown `on_delete` policy `{}`", on_delete.value),
                on_delete.span.clone(),
            ));
            OnDeleteIr::Restrict
        }
    }
}

fn build_access(entity: &SurfaceEntity, diagnostics: &mut Vec<Diagnostic>) -> Option<CrudAccessIr> {
    let Some(access) = &entity.access else {
        diagnostics.push(
            Diagnostic::error(
                "AS3001",
                format!("entity `{}` has no access policy", entity.name.value),
                entity.span.clone(),
            )
            .with_help("declare list, read, create, update, and delete rules under `access`"),
        );
        return None;
    };

    let missing = missing_access_operations(access);
    if !missing.is_empty() {
        diagnostics.push(
            Diagnostic::error(
                "AS3002",
                format!("access policy is missing: {}", missing.join(", ")),
                access.span.clone(),
            )
            .with_help("every CRUD operation must be explicitly authorized"),
        );
        return None;
    }

    Some(CrudAccessIr {
        list: convert_access_rule(access.list.as_ref().expect("validated above")),
        read: convert_access_rule(access.read.as_ref().expect("validated above")),
        create: convert_access_rule(access.create.as_ref().expect("validated above")),
        update: convert_access_rule(access.update.as_ref().expect("validated above")),
        delete: convert_access_rule(access.delete.as_ref().expect("validated above")),
    })
}

fn missing_access_operations(access: &SurfaceAccess) -> Vec<&'static str> {
    [
        ("list", access.list.is_none()),
        ("read", access.read.is_none()),
        ("create", access.create.is_none()),
        ("update", access.update.is_none()),
        ("delete", access.delete.is_none()),
    ]
    .into_iter()
    .filter_map(|(name, missing)| missing.then_some(name))
    .collect()
}

fn convert_access_rule(rule: &Located<SurfaceAccessRule>) -> AccessRuleIr {
    match &rule.value {
        SurfaceAccessRule::Public => AccessRuleIr::Public,
        SurfaceAccessRule::Role(role_name) => AccessRuleIr::Role {
            role: role_name.clone(),
        },
    }
}

fn is_app_name(value: &str) -> bool {
    value
        .bytes()
        .next()
        .is_some_and(|first| first.is_ascii_lowercase())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

fn is_rust_type_name(value: &str) -> bool {
    value
        .bytes()
        .next()
        .is_some_and(|first| first.is_ascii_uppercase())
        && value.bytes().all(|byte| byte.is_ascii_alphanumeric())
        && !is_rust_keyword(value)
}

fn is_rust_field_name(value: &str) -> bool {
    is_sql_name(value) && !is_rust_keyword(value)
}

fn is_rust_keyword(value: &str) -> bool {
    matches!(
        value,
        "Self"
            | "abstract"
            | "as"
            | "async"
            | "await"
            | "become"
            | "box"
            | "break"
            | "const"
            | "continue"
            | "crate"
            | "do"
            | "dyn"
            | "else"
            | "enum"
            | "extern"
            | "false"
            | "final"
            | "fn"
            | "for"
            | "gen"
            | "if"
            | "impl"
            | "in"
            | "let"
            | "loop"
            | "macro"
            | "match"
            | "mod"
            | "move"
            | "mut"
            | "override"
            | "priv"
            | "pub"
            | "ref"
            | "return"
            | "self"
            | "static"
            | "struct"
            | "super"
            | "trait"
            | "true"
            | "try"
            | "type"
            | "typeof"
            | "unsafe"
            | "unsized"
            | "use"
            | "virtual"
            | "where"
            | "while"
            | "yield"
    )
}

fn is_sql_name(value: &str) -> bool {
    value
        .bytes()
        .next()
        .is_some_and(|first| first.is_ascii_lowercase())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
}

fn to_snake_case(value: &str) -> String {
    let mut output = String::new();
    for (index, character) in value.chars().enumerate() {
        if character.is_ascii_uppercase() {
            if index > 0 {
                output.push('_');
            }
            output.push(character.to_ascii_lowercase());
        } else {
            output.push(character);
        }
    }
    output
}

fn pluralize(value: &str) -> String {
    if value.ends_with('s') {
        format!("{value}es")
    } else {
        format!("{value}s")
    }
}

fn relative_label(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

fn synthetic_span(file: &str) -> SourceSpan {
    SourceSpan {
        file: file.to_owned(),
        start: 0,
        end: 0,
        line: 1,
        column: 1,
        end_line: 1,
        end_column: 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_type_names() {
        assert_eq!(to_snake_case("Project"), "project");
        assert_eq!(to_snake_case("ProjectMember"), "project_member");
    }

    #[test]
    fn validates_names() {
        assert!(is_app_name("project-hub"));
        assert!(!is_app_name("ProjectHub"));
        assert!(is_rust_type_name("Project"));
        assert!(is_sql_name("project_owner"));
        assert!(!is_rust_field_name("type"));
    }
}
