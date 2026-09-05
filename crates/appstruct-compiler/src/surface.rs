mod access;
mod activity;
mod aggregates;
mod audit;
mod auth;
mod context;
mod database;
mod extension;
mod file;
mod indexes;
mod jobs;
mod mail;
mod model;
mod modules;
mod preset;
mod realtime;
mod report;
mod seeds;
mod tenant;
mod value;
mod webhooks;
mod workflow;

pub(crate) use extension::{SurfaceOperation, SurfacePage, SurfaceValueField, SurfaceValueObject};
pub(crate) use model::{
    FieldFlags, Located, SurfaceAccess, SurfaceAccessRule, SurfaceDomain, SurfaceEntity,
    SurfaceField, SurfaceFieldAccess, SurfaceFieldSemantic, SurfaceFieldUi, SurfaceRoot,
    SurfaceWorkflow,
};
pub(crate) use modules::{
    SurfaceActivity, SurfaceAudit, SurfaceAuth, SurfaceFile, SurfaceJobQueue, SurfaceJobSchedule,
    SurfaceJobs, SurfaceMail, SurfaceMailTemplate, SurfacePreset, SurfaceRealtime, SurfaceReport,
    SurfaceReportTemplate, SurfaceTenant, SurfaceWebhookEndpoint, SurfaceWebhooks,
};

use self::context::DecodeContext;
use self::indexes::decode_indexes;
use self::seeds::decode_seeds;
use self::value::{
    ensure_known_keys, expect_mapping, expect_scalar_string, expect_sequence, expect_string,
    expect_u64, optional_bool, optional_string, optional_u64, required, unknown_key_diagnostics,
};
use self::workflow::decode_workflow;
use crate::yaml::{MappingEntry, Node};
use appstruct_ir::Diagnostic;

pub(crate) fn decode_root(root: &Node) -> Result<SurfaceRoot, Vec<Diagnostic>> {
    let mapping = expect_mapping(root, "root configuration").map_err(|error| vec![error])?;
    let mut context = DecodeContext::default();
    context.extend(unknown_key_diagnostics(
        mapping,
        &[
            "version",
            "app",
            "database",
            "preset",
            "modules",
            "module_manifests",
            "includes",
        ],
        "root configuration",
    ));
    let version = context.capture(decode_version(mapping, root));
    let app_name = context.capture(decode_app_name(mapping, root));
    let database = context.capture(database::decode(mapping, root));
    let includes = context.capture(decode_string_list(mapping, "includes", true, root));
    let module_manifests =
        context.capture(decode_string_list(mapping, "module_manifests", false, root));
    let preset = context.capture(preset::decode(mapping.get("preset")));
    let modules = preset.as_ref().and_then(|preset| {
        context.capture(crate::preset::expand_modules(
            preset.as_ref(),
            mapping.get("modules"),
        ))
    });
    let decoded_modules = modules.as_ref().map(|modules| {
        (
            context.capture(auth::decode(modules.as_ref())),
            context.capture(tenant::decode(modules.as_ref())),
            context.capture(audit::decode(modules.as_ref())),
            context.capture(mail::decode(modules.as_ref())),
            context.capture(jobs::decode(modules.as_ref())),
            context.capture(webhooks::decode(modules.as_ref())),
            context.capture(realtime::decode(modules.as_ref())),
            context.capture(file::decode(modules.as_ref())),
            context.capture(report::decode(modules.as_ref())),
            context.capture(activity::decode(modules.as_ref())),
        )
    });

    let value = (|| {
        let database = database?;
        let (auth, tenant, audit, mail, jobs, webhooks, realtime, file, report, activity) =
            decoded_modules?;
        Some(SurfaceRoot {
            version: version?,
            app_name: app_name?,
            database_provider: database.provider,
            database_mode: database.mode,
            database_migration: database.migration,
            preset: preset?,
            expanded_modules: modules?,
            auth: auth?,
            tenant: tenant?,
            audit: audit?,
            mail: mail?,
            jobs: jobs?,
            webhooks: webhooks?,
            realtime: realtime?,
            file: file?,
            report: report?,
            activity: activity?,
            includes: includes?,
            module_manifests: module_manifests?,
        })
    })();
    context.finish(value)
}

pub(crate) fn decode_domain(root: &Node) -> Result<SurfaceDomain, Vec<Diagnostic>> {
    let mapping = expect_mapping(root, "domain configuration").map_err(|error| vec![error])?;
    let mut context = DecodeContext::default();
    context.extend(unknown_key_diagnostics(
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
    ));
    if let Some(includes) = mapping.get("includes") {
        context.extend([Diagnostic::error(
            "AS1006",
            "domain files cannot include other files",
            includes.key_span.clone(),
        )
        .with_help("list every domain file in the root `appstruct.yaml`")]);
    }

    let domain = context.capture(
        required(mapping, "domain", &root.span)
            .and_then(|node| expect_string(&node.value, "`domain`")),
    );
    let entities = context.capture_many(decode_entities(mapping));
    let value_objects = context.capture_many(extension::decode_value_objects(mapping));
    let commands = context.capture_many(extension::decode_operations(mapping, "commands"));
    let queries = context.capture_many(extension::decode_operations(mapping, "queries"));
    let pages = context.capture_many(extension::decode_pages(mapping));
    let value = domain.and_then(|_| {
        Some(SurfaceDomain {
            entities: entities?,
            value_objects: value_objects?,
            commands: commands?,
            queries: queries?,
            pages: pages?,
        })
    });
    context.finish(value)
}

fn decode_version(
    mapping: &std::collections::BTreeMap<String, MappingEntry>,
    root: &Node,
) -> Result<Located<u64>, Diagnostic> {
    let node = required(mapping, "version", &root.span)?;
    expect_u64(&node.value, "`version`")
}

fn decode_app_name(
    mapping: &std::collections::BTreeMap<String, MappingEntry>,
    root: &Node,
) -> Result<Located<String>, Diagnostic> {
    let node = required(mapping, "app", &root.span)?;
    let app = expect_mapping(&node.value, "`app`")?;
    ensure_known_keys(app, &["name"], "`app`")?;
    let name = required(app, "name", &node.value.span)?;
    expect_string(&name.value, "`app.name`")
}

fn decode_string_list(
    mapping: &std::collections::BTreeMap<String, MappingEntry>,
    key: &str,
    required_key: bool,
    root: &Node,
) -> Result<Vec<Located<String>>, Diagnostic> {
    let entry = if required_key {
        Some(required(mapping, key, &root.span)?)
    } else {
        mapping.get(key)
    };
    let Some(entry) = entry else {
        return Ok(Vec::new());
    };
    expect_sequence(&entry.value, &format!("`{key}`"))?
        .iter()
        .map(|node| expect_string(node, &format!("{key} path")))
        .collect()
}

fn decode_entities(
    mapping: &std::collections::BTreeMap<String, MappingEntry>,
) -> Result<Vec<SurfaceEntity>, Vec<Diagnostic>> {
    let Some(node) = mapping.get("entities") else {
        return Ok(Vec::new());
    };
    let definitions = expect_mapping(&node.value, "`entities`").map_err(|error| vec![error])?;
    let mut entities = Vec::with_capacity(definitions.len());
    let mut diagnostics = Vec::new();
    for (name, entry) in definitions {
        match decode_entity(name, entry) {
            Ok(entity) => entities.push(entity),
            Err(diagnostic) => diagnostics.push(diagnostic),
        }
    }
    if diagnostics.is_empty() {
        Ok(entities)
    } else {
        Err(diagnostics)
    }
}

fn decode_entity(name: &str, entry: &MappingEntry) -> Result<SurfaceEntity, Diagnostic> {
    let mapping = expect_mapping(&entry.value, "entity definition")?;
    ensure_known_keys(
        mapping,
        &[
            "label",
            "table",
            "fields",
            "indexes",
            "seeds",
            "access",
            "tenant",
            "audit",
            "soft_delete",
            "display_field",
            "aggregates",
            "workflow",
        ],
        "entity definition",
    )?;
    let fields_node = required(mapping, "fields", &entry.value.span)?;
    let fields_mapping = expect_mapping(&fields_node.value, "entity `fields`")?;
    let mut fields = Vec::with_capacity(fields_mapping.len());
    for (field_name, field_entry) in fields_mapping {
        fields.push(decode_field(field_name, field_entry)?);
    }
    let indexes = mapping
        .get("indexes")
        .map(|entry| decode_indexes(&entry.value))
        .transpose()?
        .unwrap_or_default();
    let seeds = mapping
        .get("seeds")
        .map(|entry| decode_seeds(&entry.value))
        .transpose()?
        .unwrap_or_default();

    Ok(SurfaceEntity {
        name: Located {
            value: name.to_owned(),
            span: entry.key_span.clone(),
        },
        label: optional_string(mapping, "label", "entity `label`")?,
        display_field: optional_string(mapping, "display_field", "entity `display_field`")?,
        aggregates: mapping
            .get("aggregates")
            .map(|entry| aggregates::decode(&entry.value))
            .transpose()?
            .unwrap_or_default(),
        table: optional_string(mapping, "table", "entity `table`")?,
        fields,
        indexes,
        seeds,
        access: mapping
            .get("access")
            .map(|access| access::decode_crud_access(&access.value))
            .transpose()?,
        tenant_scoped: optional_bool(mapping, "tenant")?,
        audit_enabled: optional_bool(mapping, "audit")?,
        soft_delete: optional_bool(mapping, "soft_delete")?,
        workflow: mapping
            .get("workflow")
            .map(|workflow| decode_workflow(&workflow.value))
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
            "access",
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
    let ui = mapping
        .get("ui")
        .map(|ui| extension::decode_field_ui(&ui.value))
        .transpose()?;

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
        ui_component: ui.as_ref().and_then(|ui| ui.component.clone()),
        ui_semantic: ui.and_then(|ui| ui.semantic),
        access: mapping
            .get("access")
            .map(|access| access::decode_field_access(&access.value))
            .transpose()?,
        span: entry.value.span.clone(),
    })
}
