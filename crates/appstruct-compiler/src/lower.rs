use crate::access::build_access;
use crate::activity::lower_activity;
use crate::audit::lower_audit;
use crate::auth::lower_auth;
use crate::billing::lower_billing;
use crate::extension::{ExtensionContext, lower_extensions};
use crate::file::lower_file;
use crate::jobs::lower_jobs;
use crate::mail::lower_mail;
use crate::module::{LoadedModule, resolve_modules_for_app};
use crate::realtime::lower_realtime;
use crate::report::lower_report;
use appstruct_core::{pluralize, to_snake_case};
mod indexes;
use self::indexes::build_indexes;
mod charts;
use self::charts::build_charts;
mod seeds;
use self::seeds::build_seeds;
use crate::server::lower_server;
use crate::surface::{SurfaceDomain, SurfaceEntity, SurfaceRoot};
use crate::tenant::lower_tenant;
use crate::validation::validate_entity_declarations;
use crate::webhooks::lower_webhooks;
use crate::workflow::lower_workflow;
use appstruct_ir::{
    AppIr, AppMeta, AuthIr, ConcurrencyIr, DatabaseDevMode, DatabaseIr, DatabaseMigrationPolicy,
    DatabaseProvider, Diagnostic, EntityId, EntityIr, EntityViewsIr, FieldTypeIr, HooksIr,
    IR_VERSION, RelationIr,
};
use std::collections::BTreeSet;

mod fields;
use self::fields::{build_fields, revision_field, tenant_field, validate_field_semantics};

#[allow(clippy::too_many_lines)]
pub(crate) fn build_ir(
    root: SurfaceRoot,
    definitions: SurfaceDomain,
    local_modules: Vec<LoadedModule>,
    warnings: &mut Vec<Diagnostic>,
) -> Result<AppIr, Vec<Diagnostic>> {
    let SurfaceDomain {
        entities: mut surface_entities,
        value_objects,
        commands,
        queries,
        pages,
    } = definitions;
    surface_entities.sort_by(|left, right| left.name.value.cmp(&right.name.value));
    let mut diagnostics = validate_entity_declarations(&surface_entities);
    if !diagnostics.is_empty() {
        return Err(diagnostics);
    }
    let auth = lower_auth(
        &root.auth,
        &surface_entities,
        &root.app_name.span,
        &mut diagnostics,
    );
    let billing = lower_billing(
        &root.billing,
        auth.enabled,
        root.tenant.enabled,
        &root.app_name.span,
        &mut diagnostics,
    );
    let tenant = lower_tenant(&root, &surface_entities, &auth, &mut diagnostics);
    let server = lower_server(&root, warnings);
    let audit = lower_audit(
        &root.audit,
        &root.auth,
        &surface_entities,
        &root.app_name.span,
        &mut diagnostics,
    );
    let mail = lower_mail(&root.mail, &root.app_name.span, &mut diagnostics);
    let jobs = lower_jobs(&root.jobs, &root.app_name.span, &mut diagnostics);
    let webhooks = lower_webhooks(&root.webhooks, &root.app_name.span, &mut diagnostics);
    let realtime = lower_realtime(&root.realtime, &auth, &root.app_name.span, &mut diagnostics);
    let file = lower_file(&root.file, &root.app_name.span, &mut diagnostics);
    let report = lower_report(
        &root.report,
        &auth,
        &jobs,
        &file,
        &billing,
        &root.app_name.span,
        &mut diagnostics,
    );
    let known_entities = surface_entities
        .iter()
        .map(|entity| entity.name.value.clone())
        .collect::<BTreeSet<_>>();
    let known_values = value_objects
        .iter()
        .map(|value| value.name.value.clone())
        .collect::<BTreeSet<_>>();
    let (mut entities, mut relations, seeds) = lower_entities(
        surface_entities,
        &known_entities,
        &known_values,
        &auth,
        root.tenant.sortable_indexes,
        &mut diagnostics,
    );
    if let Err(errors) = appstruct_ir::validate_form_rules(&entities) {
        diagnostics.extend(errors.errors().iter().map(|error| {
            Diagnostic::error(
                "AS2045",
                format!("{}: {}", error.path, error.message),
                root.app_name.span.clone(),
            )
        }));
    }
    if (!file.enabled || !auth.enabled)
        && entities.iter().any(|entity| {
            entity.fields.iter().any(|field| {
                matches!(
                    field.ui_semantic,
                    Some(
                        appstruct_ir::FieldSemanticIr::File | appstruct_ir::FieldSemanticIr::Files
                    )
                )
            })
        })
    {
        diagnostics.push(Diagnostic::error(
            "AS2020",
            "the file UI semantic requires both `modules.auth` and `modules.file` to be enabled",
            root.app_name.span.clone(),
        ));
    }
    let activity = lower_activity(
        &root.activity,
        &auth,
        &audit,
        &file,
        &entities,
        &root.app_name.span,
        &mut diagnostics,
    );
    if let Err(errors) = appstruct_ir::validate_aggregates(&entities) {
        diagnostics.extend(errors.errors().iter().map(|error| {
            Diagnostic::error(
                "AS2043",
                format!("{}: {}", error.path, error.message),
                root.app_name.span.clone(),
            )
        }));
    }
    if let Err(errors) = appstruct_ir::validate_charts(&entities) {
        diagnostics.extend(errors.errors().iter().map(|error| {
            Diagnostic::error(
                "AS2044",
                format!("{}: {}", error.path, error.message),
                root.app_name.span.clone(),
            )
        }));
    }
    let resource_paths = entities
        .iter()
        .map(|entity| entity.table_name.clone())
        .collect::<BTreeSet<_>>();
    for entity in &entities {
        let path = entity.table_name.as_str();
        let conflicts = auth.enabled && matches!(path, "account" | "notifications")
            || tenant.enabled && matches!(path, "team" | "credits")
            || report.enabled && path == "results"
            || auth.enabled && file.enabled && path == "files"
            || tenant.enabled && path == "onboarding"
            || path == "workspace" && pages.iter().any(|page| page.path.value == "/");
        if conflicts {
            diagnostics.push(Diagnostic::error(
                "AS3012",
                format!("resource path `{path}` conflicts with a generated route"),
                root.app_name.span.clone(),
            ));
        }
    }
    let extensions = lower_extensions(
        value_objects,
        commands,
        queries,
        pages,
        &ExtensionContext {
            known_entities: &known_entities,
            resource_paths: &resource_paths,
            auth: &auth,
            billing: &billing,
        },
        &mut diagnostics,
    );
    let dashboard = lower_dashboard(root.dashboard.as_ref(), &entities, &mut diagnostics);
    if !diagnostics.is_empty() {
        return Err(diagnostics);
    }
    let modules = resolve_modules_for_app(
        &auth,
        &tenant,
        &audit,
        &mail,
        &jobs,
        &webhooks,
        &realtime,
        &file,
        &report,
        &activity,
        local_modules,
    )
    .map_err(|error| {
        vec![Diagnostic::error(
            "AS3062",
            error.to_string(),
            root.app_name.span.clone(),
        )]
    })?;
    canonicalize_entities(&mut entities, &mut relations);
    let database = lower_database(&root);
    Ok(AppIr {
        ir_version: IR_VERSION,
        app: AppMeta {
            name: root.app_name.value,
        },
        database,
        preset: root.preset.map(|preset| appstruct_ir::PresetIr {
            name: preset.name.value,
            version: u32::try_from(preset.version.value).unwrap_or(u32::MAX),
            digest: crate::preset::preset_digest(),
        }),
        auth,
        billing,
        server,
        tenant,
        audit,
        mail,
        jobs,
        webhooks,
        realtime,
        file,
        report,
        activity,
        enums: Vec::new(),
        value_objects: extensions.value_objects,
        entities,
        seeds,
        relations,
        commands: extensions.commands,
        queries: extensions.queries,
        pages: extensions.pages,
        dashboard,
        modules,
    })
}

fn lower_database(root: &SurfaceRoot) -> DatabaseIr {
    DatabaseIr {
        provider: DatabaseProvider::Postgres,
        dev_mode: if root.database_mode.value == "external" {
            DatabaseDevMode::External
        } else {
            DatabaseDevMode::Managed
        },
        dev_migration: match root.database_migration.value.as_str() {
            "auto" => DatabaseMigrationPolicy::Auto,
            "never" => DatabaseMigrationPolicy::Never,
            "unmanaged" => DatabaseMigrationPolicy::Unmanaged,
            _ => DatabaseMigrationPolicy::Prompt,
        },
        allow_unmanaged_tables: root.database_allow_unmanaged_tables,
    }
}

fn canonicalize_entities(entities: &mut [EntityIr], relations: &mut [RelationIr]) {
    for entity in &mut *entities {
        entity.fields.sort_by(|left, right| left.id.cmp(&right.id));
        entity.indexes.sort_by(|left, right| left.id.cmp(&right.id));
    }
    entities.sort_by(|left, right| left.id.cmp(&right.id));
    relations.sort_by(|left, right| left.id.cmp(&right.id));
}

#[allow(clippy::too_many_lines)]
fn lower_entities(
    surface_entities: Vec<SurfaceEntity>,
    known_entities: &BTreeSet<String>,
    known_values: &BTreeSet<String>,
    auth: &AuthIr,
    tenant_sortable_indexes: bool,
    diagnostics: &mut Vec<Diagnostic>,
) -> (Vec<EntityIr>, Vec<RelationIr>, Vec<appstruct_ir::SeedIr>) {
    let mut entities = Vec::with_capacity(surface_entities.len());
    let mut relations = Vec::new();
    let mut seeds = Vec::new();
    for entity in surface_entities {
        let entity_id = EntityId(format!("app::{}", entity.name.value));
        let table_name = entity.table.as_ref().map_or_else(
            || pluralize(&to_snake_case(&entity.name.value)),
            |table| table.value.clone(),
        );
        let access = build_access(&entity, auth, diagnostics);
        let (mut fields, mut entity_relations) =
            build_fields(&entity, &entity_id, known_entities, auth, diagnostics);
        validate_field_semantics(&entity, &fields, diagnostics);
        let revision_conflict = entity.fields.iter().find(|field| {
            field
                .column
                .as_ref()
                .map_or(field.name.value.as_str(), |column| column.value.as_str())
                == "revision"
        });
        if let Some(field) = revision_conflict {
            diagnostics.push(Diagnostic::error(
                "AS2012",
                "`revision` is reserved for optimistic concurrency control",
                field.span.clone(),
            ));
        } else {
            fields.push(revision_field(&entity_id));
        }
        if entity.tenant_scoped {
            if let Some(field) = entity.fields.iter().find(|field| {
                field
                    .column
                    .as_ref()
                    .map_or(field.name.value.as_str(), |column| column.value.as_str())
                    == "tenant_id"
            }) {
                diagnostics.push(Diagnostic::error(
                    "AS3036",
                    "`tenant_id` is reserved for tenant isolation",
                    field.span.clone(),
                ));
            } else {
                fields.push(tenant_field(&entity_id));
            }
        }
        if entity.soft_delete
            && !fields.iter().any(|field| {
                field.rust_name == "deleted_at"
                    && field.nullable
                    && matches!(field.ty, FieldTypeIr::Datetime)
            })
        {
            diagnostics.push(Diagnostic::error(
                "AS2041",
                "soft_delete requires a nullable datetime field named `deleted_at`",
                entity.span.clone(),
            ));
        }
        let indexes = build_indexes(
            &entity,
            &entity_id,
            &fields,
            tenant_sortable_indexes,
            diagnostics,
        );
        let display_field = lower_display_field(&entity, &fields, diagnostics);
        let charts = build_charts(&entity, &fields, diagnostics);
        let computed = lower_computed(&entity, &fields, diagnostics);
        seeds.extend(build_seeds(&entity, &entity_id, &fields, diagnostics));
        let workflow = lower_workflow(&entity, &fields, known_values, auth, diagnostics);
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
                indexes,
                access,
                views: EntityViewsIr {
                    aggregates: entity.aggregates,
                    charts,
                    computed,
                    display_field,
                    soft_delete: entity.soft_delete,
                },
                hooks: HooksIr::default(),
                concurrency: ConcurrencyIr { enabled: true },
                tenant_scoped: entity.tenant_scoped,
                audit_enabled: entity.audit_enabled,
                workflow,
            });
        }
    }
    seeds.sort_by(|left, right| left.id.cmp(&right.id));
    (entities, relations, seeds)
}

fn lower_display_field(
    entity: &SurfaceEntity,
    fields: &[appstruct_ir::FieldIr],
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<appstruct_ir::FieldId> {
    let name = entity.display_field.as_ref()?;
    match fields.iter().find(|field| field.api_name == name.value) {
        Some(field)
            if matches!(
                field.ty,
                FieldTypeIr::String
                    | FieldTypeIr::Text
                    | FieldTypeIr::Enum { .. }
                    | FieldTypeIr::Uuid
                    | FieldTypeIr::Integer
                    | FieldTypeIr::Bigint
            ) =>
        {
            Some(field.id.clone())
        }
        _ => {
            diagnostics.push(Diagnostic::error(
                "AS2042",
                "display_field must reference a text, enum, UUID or integer field",
                name.span.clone(),
            ));
            None
        }
    }
}

fn lower_computed(
    entity: &SurfaceEntity,
    fields: &[appstruct_ir::FieldIr],
    diagnostics: &mut Vec<Diagnostic>,
) -> Vec<appstruct_ir::ComputedFieldIr> {
    let mut out = Vec::new();
    for (name, template) in &entity.computed {
        if fields
            .iter()
            .any(|field| field.api_name == name.value || field.rust_name == name.value)
        {
            diagnostics.push(Diagnostic::error(
                "AS2054",
                format!(
                    "computed field `{}` collides with an existing field on `{}`",
                    name.value, entity.name.value
                ),
                name.span.clone(),
            ));
            continue;
        }
        let mut source_fields = Vec::new();
        let mut rest = template.value.as_str();
        while let Some(start) = rest.find('{') {
            rest = &rest[start + 1..];
            let Some(end) = rest.find('}') else {
                diagnostics.push(Diagnostic::error(
                    "AS2054",
                    format!(
                        "computed field `{}` has an unclosed `{{` placeholder",
                        name.value
                    ),
                    template.span.clone(),
                ));
                break;
            };
            let placeholder = &rest[..end];
            rest = &rest[end + 1..];
            if let Some(field) = fields
                .iter()
                .find(|field| field.api_name == placeholder || field.rust_name == placeholder)
            {
                if !source_fields.contains(&field.id) {
                    source_fields.push(field.id.clone());
                }
            } else {
                diagnostics.push(Diagnostic::error(
                    "AS2054",
                    format!(
                        "computed field `{}` references unknown field `{placeholder}` on `{}`",
                        name.value, entity.name.value
                    ),
                    template.span.clone(),
                ));
            }
        }
        out.push(appstruct_ir::ComputedFieldIr {
            name: name.value.clone(),
            template: template.value.clone(),
            source_fields,
        });
    }
    out
}

fn lower_dashboard(
    dashboard: Option<&crate::surface::SurfaceDashboard>,
    entities: &[EntityIr],
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<appstruct_ir::DashboardIr> {
    let dashboard = dashboard?;
    let mut widgets = Vec::with_capacity(dashboard.widgets.len());
    let mut seen_ids = BTreeSet::new();
    for widget in &dashboard.widgets {
        if !seen_ids.insert(widget.id.value.clone()) {
            diagnostics.push(Diagnostic::error(
                "AS2055",
                format!("duplicate dashboard widget id `{}`", widget.id.value),
                widget.id.span.clone(),
            ));
            continue;
        }
        let Some(entity) = entities
            .iter()
            .find(|entity| entity.rust_name == widget.entity.value)
        else {
            diagnostics.push(Diagnostic::error(
                "AS2055",
                format!(
                    "dashboard widget `{}` references unknown entity `{}`",
                    widget.id.value, widget.entity.value
                ),
                widget.entity.span.clone(),
            ));
            continue;
        };
        if !entity
            .views
            .charts
            .iter()
            .any(|chart| chart.name == widget.chart.value)
        {
            diagnostics.push(Diagnostic::error(
                "AS2055",
                format!(
                    "dashboard widget `{}` references unknown chart `{}` on entity `{}`",
                    widget.id.value, widget.chart.value, widget.entity.value
                ),
                widget.chart.span.clone(),
            ));
            continue;
        }
        widgets.push(appstruct_ir::DashboardWidgetIr {
            id: widget.id.value.clone(),
            title: widget.title.as_ref().map(|t| t.value.clone()),
            entity: entity.id.clone(),
            chart: widget.chart.value.clone(),
            span: widget.span,
        });
    }
    Some(appstruct_ir::DashboardIr {
        title: dashboard.title.as_ref().map(|t| t.value.clone()),
        widgets,
    })
}
