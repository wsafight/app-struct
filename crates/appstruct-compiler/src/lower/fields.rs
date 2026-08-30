use crate::field::{build_column, build_field_type, build_relation};
use crate::field_options::{build_generated, validate_field_options};
use crate::surface::{SurfaceEntity, SurfaceField};
use crate::validation::validate_primary_key;
use appstruct_ir::{
    AuthIr, Diagnostic, EntityId, FieldCapabilities, FieldId, FieldIr, FieldTypeIr,
    GeneratedValueIr, RelationIr, SourceSpan, ValidationIr,
};
use std::collections::{BTreeMap, BTreeSet};

pub(super) fn tenant_field(entity_id: &EntityId) -> FieldIr {
    FieldIr {
        id: FieldId(format!("{entity_id}.tenant_id")),
        entity: entity_id.clone(),
        rust_name: "tenant_id".to_owned(),
        api_name: "tenant_id".to_owned(),
        column_name: "tenant_id".to_owned(),
        ty: FieldTypeIr::Uuid,
        nullable: false,
        primary_key: false,
        unique: false,
        generated: Some(GeneratedValueIr::Tenant),
        default: None,
        validation: ValidationIr::default(),
        capabilities: FieldCapabilities {
            searchable: false,
            filterable: false,
            sortable: false,
        },
        read_access: None,
        write_access: None,
        ui_component: None,
    }
}

pub(super) fn revision_field(entity_id: &EntityId) -> FieldIr {
    FieldIr {
        id: FieldId(format!("{entity_id}.revision")),
        entity: entity_id.clone(),
        rust_name: "revision".to_owned(),
        api_name: "revision".to_owned(),
        column_name: "revision".to_owned(),
        ty: FieldTypeIr::Bigint,
        nullable: false,
        primary_key: false,
        unique: false,
        generated: Some(GeneratedValueIr::Revision),
        default: Some("1".to_owned()),
        validation: ValidationIr::default(),
        capabilities: FieldCapabilities {
            searchable: false,
            filterable: false,
            sortable: false,
        },
        read_access: None,
        write_access: None,
        ui_component: None,
    }
}

pub(super) fn build_fields(
    entity: &SurfaceEntity,
    entity_id: &EntityId,
    known_entities: &BTreeSet<String>,
    auth: &AuthIr,
    diagnostics: &mut Vec<Diagnostic>,
) -> (Vec<FieldIr>, Vec<RelationIr>) {
    let mut fields = Vec::with_capacity(entity.fields.len());
    let mut relations = Vec::new();
    let mut columns = BTreeMap::<String, SourceSpan>::new();
    validate_primary_key(entity, diagnostics);
    for field in &entity.fields {
        if let Some((field_ir, relation)) = build_field(
            field,
            entity_id,
            known_entities,
            auth,
            &mut columns,
            diagnostics,
        ) {
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
    auth: &AuthIr,
    columns: &mut BTreeMap<String, SourceSpan>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<(FieldIr, Option<RelationIr>)> {
    let (column_name, is_relation) = build_column(field, columns, diagnostics)?;
    let field_type = build_field_type(field, known_entities, diagnostics)?;
    validate_field_options(field, &field_type, diagnostics);
    let generated = build_generated(field, &field_type, diagnostics);
    let (read_access, write_access) = crate::access::build_field_access(field, auth, diagnostics);
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
            read_access,
            write_access,
            ui_component: field
                .ui_component
                .as_ref()
                .map(|component| component.value.clone()),
        },
        relation,
    ))
}
