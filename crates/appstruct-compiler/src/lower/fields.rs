use crate::field::{build_column, build_field_type, build_relation};
use crate::field_options::{build_generated, validate_field_options};
use crate::surface::{SurfaceEntity, SurfaceField};
use crate::validation::validate_primary_key;
use appstruct_ir::{
    AuthIr, Diagnostic, EntityId, FieldCapabilities, FieldId, FieldIr, FieldSemanticIr,
    FieldTypeIr, GeneratedValueIr, RelationIr, SourceSpan, ValidationIr,
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
        ui_semantic: None,
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
        ui_semantic: None,
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

pub(super) fn validate_field_semantics(
    entity: &SurfaceEntity,
    fields: &[FieldIr],
    diagnostics: &mut Vec<Diagnostic>,
) {
    let mut companions = BTreeMap::<&str, &SurfaceField>::new();
    for surface_field in &entity.fields {
        let Some(crate::surface::SurfaceFieldSemantic::Money { currency_field, .. }) =
            &surface_field.ui_semantic
        else {
            continue;
        };
        let amount = fields
            .iter()
            .find(|field| field.api_name == surface_field.name.value);
        let currency = fields
            .iter()
            .find(|field| field.api_name == currency_field.value);
        let Some(currency) = currency else {
            diagnostics.push(Diagnostic::error(
                "AS2020",
                format!(
                    "money currency field `{}` does not exist on this entity",
                    currency_field.value
                ),
                currency_field.span.clone(),
            ));
            continue;
        };
        let FieldTypeIr::Enum { values } = &currency.ty else {
            diagnostics.push(Diagnostic::error(
                "AS2020",
                format!(
                    "money currency field `{}` must be an enum",
                    currency_field.value
                ),
                currency_field.span.clone(),
            ));
            continue;
        };
        if values
            .iter()
            .any(|value| value.len() != 3 || !value.bytes().all(|byte| byte.is_ascii_uppercase()))
        {
            diagnostics.push(Diagnostic::error(
                "AS2020",
                "money currency enum values must be three uppercase ASCII letters",
                currency_field.span.clone(),
            ));
        }
        if let Some(amount) = amount {
            if amount.nullable != currency.nullable {
                diagnostics.push(Diagnostic::error(
                    "AS2020",
                    "money amount and currency fields must have matching requiredness",
                    currency_field.span.clone(),
                ));
            }
            if amount.read_access != currency.read_access
                || amount.write_access != currency.write_access
            {
                diagnostics.push(Diagnostic::error(
                    "AS2020",
                    "money amount and currency fields must have identical field access",
                    currency_field.span.clone(),
                ));
            }
        }
        let target_surface = entity
            .fields
            .iter()
            .find(|field| field.name.value == currency_field.value)
            .expect("lowered field has a surface field");
        if target_surface.ui_component.is_some() || target_surface.ui_semantic.is_some() {
            diagnostics.push(Diagnostic::error(
                "AS2020",
                "a money currency field cannot declare its own UI component or semantic",
                target_surface.span.clone(),
            ));
        }
        if let Some(first) = companions.insert(&currency_field.value, surface_field) {
            diagnostics.push(
                Diagnostic::error(
                    "AS2020",
                    format!(
                        "currency field `{}` cannot be shared by multiple money fields",
                        currency_field.value
                    ),
                    currency_field.span.clone(),
                )
                .with_secondary(first.span.clone(), "first money field declared here"),
            );
        }
    }
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
            ui_semantic: field.ui_semantic.as_ref().map(|semantic| match semantic {
                crate::surface::SurfaceFieldSemantic::Money {
                    currency_field,
                    fraction_digits,
                } => FieldSemanticIr::Money {
                    currency_field: FieldId(format!("{entity_id}.{}", currency_field.value)),
                    fraction_digits: u8::try_from(fraction_digits.value).unwrap_or(u8::MAX),
                },
            }),
        },
        relation,
    ))
}
