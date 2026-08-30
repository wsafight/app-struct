use crate::access::build_access;
use crate::audit::lower_audit;
use crate::auth::lower_auth;
use crate::extension::{ExtensionContext, lower_extensions};
use crate::file::lower_file;
use crate::jobs::lower_jobs;
use crate::mail::lower_mail;
use crate::module::{LoadedModule, resolve_modules_for_app};
use crate::naming::{pluralize, to_snake_case};
use crate::realtime::lower_realtime;
mod indexes;
use self::indexes::build_indexes;
mod seeds;
use self::seeds::build_seeds;
use crate::surface::{SurfaceDomain, SurfaceEntity, SurfaceRoot};
use crate::tenant::lower_tenant;
use crate::validation::validate_entity_declarations;
use crate::webhooks::lower_webhooks;
use appstruct_ir::{
    AppIr, AppMeta, AuthIr, ConcurrencyIr, DatabaseDevMode, DatabaseIr, DatabaseMigrationPolicy,
    DatabaseProvider, Diagnostic, EntityId, EntityIr, EntityViewsIr, FieldTypeIr, HooksIr,
    IR_VERSION, RelationIr,
};
use std::collections::BTreeSet;

mod fields;
use self::fields::{build_fields, revision_field, tenant_field};

#[allow(clippy::too_many_lines)]
pub(crate) fn build_ir(
    root: SurfaceRoot,
    definitions: SurfaceDomain,
    local_modules: Vec<LoadedModule>,
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
    let tenant = lower_tenant(&root, &surface_entities, &auth, &mut diagnostics);
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
    let known_entities = surface_entities
        .iter()
        .map(|entity| entity.name.value.clone())
        .collect::<BTreeSet<_>>();
    let (mut entities, mut relations, seeds) =
        lower_entities(surface_entities, &known_entities, &auth, &mut diagnostics);
    let resource_paths = entities
        .iter()
        .map(|entity| entity.table_name.clone())
        .collect::<BTreeSet<_>>();
    let extensions = lower_extensions(
        value_objects,
        commands,
        queries,
        pages,
        &ExtensionContext {
            known_entities: &known_entities,
            resource_paths: &resource_paths,
            auth: &auth,
        },
        &mut diagnostics,
    );
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
        tenant,
        audit,
        mail,
        jobs,
        webhooks,
        realtime,
        file,
        enums: Vec::new(),
        value_objects: extensions.value_objects,
        entities,
        seeds,
        relations,
        commands: extensions.commands,
        queries: extensions.queries,
        pages: extensions.pages,
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

fn lower_entities(
    surface_entities: Vec<SurfaceEntity>,
    known_entities: &BTreeSet<String>,
    auth: &AuthIr,
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
        let indexes = build_indexes(&entity, &entity_id, &fields, diagnostics);
        seeds.extend(build_seeds(&entity, &entity_id, &fields, diagnostics));
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
                    soft_delete: entity.soft_delete,
                },
                hooks: HooksIr::default(),
                concurrency: ConcurrencyIr { enabled: true },
                tenant_scoped: entity.tenant_scoped,
                audit_enabled: entity.audit_enabled,
            });
        }
    }
    seeds.sort_by(|left, right| left.id.cmp(&right.id));
    (entities, relations, seeds)
}
