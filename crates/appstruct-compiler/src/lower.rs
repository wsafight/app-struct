use crate::access::build_access;
use crate::extension::lower_extensions;
use crate::field::{build_column, build_field_type, build_relation};
use crate::field_options::{build_generated, validate_field_options};
use crate::naming::{pluralize, to_snake_case};
use crate::surface::{SurfaceDomain, SurfaceEntity, SurfaceField, SurfaceRoot};
use crate::validation::{validate_entity_declarations, validate_primary_key};
use appstruct_ir::{
    AppIr, AppMeta, AuthIr, ConcurrencyIr, DatabaseDevMode, DatabaseIr, DatabaseProvider,
    Diagnostic, EntityId, EntityIr, EntityViewsIr, FieldCapabilities, FieldId, FieldIr, HooksIr,
    IR_VERSION, RelationIr, SourceSpan, ValidationIr,
};
use std::collections::{BTreeMap, BTreeSet};

pub(crate) fn build_ir(
    root: SurfaceRoot,
    definitions: SurfaceDomain,
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
    let known_entities = surface_entities
        .iter()
        .map(|entity| entity.name.value.clone())
        .collect::<BTreeSet<_>>();
    let (mut entities, mut relations) =
        lower_entities(surface_entities, &known_entities, &mut diagnostics);
    let resource_paths = entities
        .iter()
        .map(|entity| entity.table_name.clone())
        .collect::<BTreeSet<_>>();
    let extensions = lower_extensions(
        value_objects,
        commands,
        queries,
        pages,
        &known_entities,
        &resource_paths,
        &mut diagnostics,
    );
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
            dev_mode: if root.database_mode.value == "external" {
                DatabaseDevMode::External
            } else {
                DatabaseDevMode::Managed
            },
        },
        auth: AuthIr {
            enabled: root.auth_enabled,
        },
        enums: Vec::new(),
        value_objects: extensions.value_objects,
        entities,
        relations,
        commands: extensions.commands,
        queries: extensions.queries,
        pages: extensions.pages,
        modules: Vec::new(),
    })
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
    let nullable = !(field.flags.required()
        || field.flags.primary_key()
        || generated.is_some()
        || field.default.is_some());
    Some((
        FieldIr {
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
            unique: field.flags.unique(),
            generated,
            default: field.default.as_ref().map(|value| value.value.clone()),
            validation: ValidationIr {
                min_length: field.min_length.as_ref().map(|value| value.value),
                max_length: field.max_length.as_ref().map(|value| value.value),
                minimum: field.minimum.as_ref().map(|value| value.value.clone()),
                maximum: field.maximum.as_ref().map(|value| value.value.clone()),
            },
            capabilities: FieldCapabilities {
                searchable: field.flags.searchable(),
                filterable: field.flags.filterable(),
                sortable: field.flags.sortable(),
            },
            ui_component: field
                .ui_component
                .as_ref()
                .map(|component| component.value.clone()),
        },
        relation,
    ))
}
