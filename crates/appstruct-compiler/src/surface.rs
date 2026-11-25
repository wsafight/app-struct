mod access;
mod activity;
mod aggregates;
mod audit;
mod auth;
mod billing;
mod charts;
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
mod root;
mod seeds;
mod server;
mod tenant;
mod value;
mod webhooks;
mod workflow;

pub(crate) use extension::{
    SurfaceHttpOperation, SurfaceOperation, SurfacePage, SurfaceValueField, SurfaceValueObject,
};
pub(crate) use model::{
    FieldFlags, Located, SurfaceAccess, SurfaceAccessRule, SurfaceChart, SurfaceDashboard,
    SurfaceDashboardWidget, SurfaceDomain, SurfaceEntity, SurfaceField, SurfaceFieldAccess,
    SurfaceFieldSemantic, SurfaceFieldUi, SurfaceFrameOptions, SurfaceHsts, SurfaceReferrerPolicy,
    SurfaceRoot, SurfaceSecurityHeaders, SurfaceServer, SurfaceWorkflow,
};
pub(crate) use modules::{
    SurfaceActivity, SurfaceAudit, SurfaceAuth, SurfaceBilling, SurfaceBillingPlan, SurfaceFile,
    SurfaceJobQueue, SurfaceJobSchedule, SurfaceJobs, SurfaceMail, SurfaceMailTemplate,
    SurfacePreset, SurfaceRealtime, SurfaceReport, SurfaceReportTemplate, SurfaceTenant,
    SurfaceWebhookEndpoint, SurfaceWebhooks,
};

use self::context::DecodeContext;
use self::indexes::decode_indexes;
use self::root::{decode_app_name, decode_string_list, decode_version};
use self::seeds::decode_seeds;
use self::value::{
    ensure_known_keys, expect_bool, expect_mapping, expect_scalar_string, expect_sequence,
    expect_string, optional_bool, optional_string, optional_u64, required, unknown_key_diagnostics,
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
            "server",
            "dashboard",
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
    let server = context.capture(server::decode(mapping.get("server")));
    let dashboard = context.capture(decode_dashboard(mapping.get("dashboard")));
    let modules = preset.as_ref().and_then(|preset| {
        context.capture(crate::preset::expand_modules(
            preset.as_ref(),
            mapping.get("modules"),
        ))
    });
    let decoded_modules = modules.as_ref().map(|modules| {
        (
            context.capture(auth::decode(modules.as_ref())),
            context.capture(billing::decode(modules.as_ref())),
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
        let (auth, billing, tenant, audit, mail, jobs, webhooks, realtime, file, report, activity) =
            decoded_modules?;
        Some(SurfaceRoot {
            version: version?,
            app_name: app_name?,
            database_provider: database.provider,
            database_mode: database.mode,
            database_migration: database.migration,
            database_allow_unmanaged_tables: database.allow_unmanaged_tables,
            preset: preset?,
            expanded_modules: modules?,
            auth: auth?,
            billing: billing?,
            server: server?,
            tenant: tenant?,
            audit: audit?,
            mail: mail?,
            jobs: jobs?,
            webhooks: webhooks?,
            realtime: realtime?,
            file: file?,
            report: report?,
            activity: activity?,
            dashboard: dashboard?,
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
            "tenant_index",
            "audit",
            "soft_delete",
            "display_field",
            "aggregates",
            "charts",
            "computed",
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
        charts: mapping
            .get("charts")
            .map(|entry| charts::decode(&entry.value))
            .transpose()?
            .unwrap_or_default(),
        computed: mapping
            .get("computed")
            .map(|entry| decode_computed(&entry.value))
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
        tenant_index: mapping
            .get("tenant_index")
            .map(|entry| {
                expect_bool(&entry.value, "entity `tenant_index`").map(|value| Located {
                    value,
                    span: entry.value.span.clone(),
                })
            })
            .transpose()?,
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
            "required_when",
            "greater_than",
            "searchable",
            "filterable",
            "sortable",
            "access",
            "values",
            "target",
            "on_delete",
            "ui",
            "mask",
        ],
        "field definition in M0",
    )?;
    let type_node = required(mapping, "type", &entry.value.span)?;
    let type_name = expect_string(&type_node.value, "field `type`")?;
    let flags = decode_field_flags(mapping)?;
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
        required_when: mapping
            .get("required_when")
            .map(|entry| decode_required_when(&entry.value))
            .transpose()?,
        greater_than: optional_string(mapping, "greater_than", "greater_than field")?,
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
        mask: mapping
            .get("mask")
            .map(|entry| decode_field_mask(&entry.value))
            .transpose()?,
        span: entry.value.span.clone(),
    })
}

fn decode_computed(node: &Node) -> Result<Vec<model::SurfaceComputedEntry>, Diagnostic> {
    let mapping = expect_mapping(node, "entity `computed`")?;
    let mut fields = Vec::with_capacity(mapping.len());
    for (name, entry) in mapping {
        let template = expect_string(&entry.value, "computed field template")?;
        fields.push((
            Located {
                value: name.clone(),
                span: entry.key_span.clone(),
            },
            template,
        ));
    }
    Ok(fields)
}

fn decode_dashboard(entry: Option<&MappingEntry>) -> Result<Option<SurfaceDashboard>, Diagnostic> {
    let Some(entry) = entry else {
        return Ok(None);
    };
    let mapping = expect_mapping(&entry.value, "`dashboard`")?;
    ensure_known_keys(mapping, &["title", "widgets"], "`dashboard`")?;
    let title = optional_string(mapping, "title", "dashboard `title`")?;
    let widgets_node = required(mapping, "widgets", &entry.value.span)?;
    let items = expect_sequence(&widgets_node.value, "dashboard `widgets`")?;
    let mut widgets = Vec::with_capacity(items.len());
    for item in items {
        let widget = expect_mapping(item, "dashboard widget")?;
        ensure_known_keys(
            widget,
            &["id", "title", "entity", "chart", "span"],
            "dashboard widget",
        )?;
        let id = required(widget, "id", &item.span)
            .and_then(|e| expect_string(&e.value, "widget `id`"))?;
        let widget_title = optional_string(widget, "title", "widget `title`")?;
        let entity = required(widget, "entity", &item.span)
            .and_then(|e| expect_string(&e.value, "widget `entity`"))?;
        let chart = required(widget, "chart", &item.span)
            .and_then(|e| expect_string(&e.value, "widget `chart`"))?;
        let span = optional_u64(widget, "span")?
            .map_or(6u8, |v| u8::try_from(v.value).unwrap_or(12).clamp(1, 12));
        widgets.push(SurfaceDashboardWidget {
            id,
            title: widget_title,
            entity,
            chart,
            span,
        });
    }
    Ok(Some(SurfaceDashboard { title, widgets }))
}

fn decode_field_mask(node: &Node) -> Result<appstruct_ir::FieldMaskIr, Diagnostic> {
    let mapping = expect_mapping(node, "field `mask`")?;
    ensure_known_keys(mapping, &["strategy", "except_roles"], "field `mask`")?;
    let strategy_node = required(mapping, "strategy", &node.span)?;
    let strategy_raw = expect_string(&strategy_node.value, "field `mask.strategy`")?;
    let strategy = match strategy_raw.value.as_str() {
        "email" => appstruct_ir::MaskStrategyIr::Email,
        "phone" => appstruct_ir::MaskStrategyIr::Phone,
        "last4" => appstruct_ir::MaskStrategyIr::Last4,
        "redact" => appstruct_ir::MaskStrategyIr::Redact,
        other => {
            return Err(Diagnostic::error(
                "AS2052",
                format!("unsupported field mask strategy `{other}`"),
                strategy_raw.span,
            )
            .with_help("use `email`, `phone`, `last4`, or `redact`"));
        }
    };
    let except_roles = mapping
        .get("except_roles")
        .map(|entry| {
            expect_sequence(&entry.value, "field `mask.except_roles`")?
                .iter()
                .map(|item| expect_string(item, "mask exempt role").map(|v| v.value))
                .collect::<Result<Vec<_>, _>>()
        })
        .transpose()?
        .unwrap_or_default();
    Ok(appstruct_ir::FieldMaskIr {
        strategy,
        except_roles,
    })
}

fn decode_field_flags(
    mapping: &std::collections::BTreeMap<String, MappingEntry>,
) -> Result<FieldFlags, Diagnostic> {
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
    Ok(flags)
}

fn decode_required_when(
    node: &crate::yaml::Node,
) -> Result<appstruct_ir::RequiredWhenIr, Diagnostic> {
    let mapping = expect_mapping(node, "required_when")?;
    ensure_known_keys(mapping, &["field", "equals"], "required_when")?;
    let field = expect_string(
        &required(mapping, "field", &node.span)?.value,
        "condition field",
    )?;
    let value = &required(mapping, "equals", &node.span)?.value;
    let Some((text, plain)) = value.scalar() else {
        return Err(Diagnostic::error(
            "AS2045",
            "required_when.equals must be a boolean or string",
            value.span.clone(),
        ));
    };
    let equals = if plain && ["true", "false"].contains(&text) {
        appstruct_ir::ConditionValueIr::Boolean(text == "true")
    } else {
        if plain && (text == "null" || text == "~" || text.parse::<f64>().is_ok()) {
            return Err(Diagnostic::error(
                "AS2045",
                "required_when.equals must be a boolean or string; quote text values",
                value.span.clone(),
            ));
        }
        appstruct_ir::ConditionValueIr::String(text.to_owned())
    };
    Ok(appstruct_ir::RequiredWhenIr {
        field: appstruct_ir::FieldId(field.value),
        equals,
    })
}
