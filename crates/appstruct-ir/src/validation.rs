mod graph;
mod indexes;
use self::graph::{validate_modules, validate_operations, validate_services};
use self::indexes::validate_indexes;
use crate::{
    AccessRuleIr, AppIr, EntityId, EntityIr, FieldIr, FieldTypeIr, GeneratedValueIr, IR_VERSION,
    RelationIr,
};
use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt;
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IrValidationError {
    pub path: String,
    pub message: String,
}
impl fmt::Display for IrValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.path, self.message)
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IrValidationErrors(Vec<IrValidationError>);
impl IrValidationErrors {
    #[must_use]
    pub fn errors(&self) -> &[IrValidationError] {
        &self.0
    }
}
impl fmt::Display for IrValidationErrors {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "invalid AppStruct IR: {}",
            self.0
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("; ")
        )
    }
}
impl Error for IrValidationErrors {}
/// # Errors
///
/// Returns every independently detectable invariant violation in deterministic path order.
pub fn validate_app_ir(ir: &AppIr) -> Result<(), IrValidationErrors> {
    let mut errors = Vec::new();
    if ir.ir_version != IR_VERSION {
        push(
            &mut errors,
            "ir_version",
            format!("expected {IR_VERSION}, found {}", ir.ir_version),
        );
    }
    let entities = entity_index(ir, &mut errors);
    let value_objects = unique_ids(
        ir.value_objects.iter().map(|value| value.id.as_str()),
        "value_objects",
        &mut errors,
    );
    validate_entities(ir, &entities, &mut errors);
    validate_relations(ir, &entities, &mut errors);
    validate_services(ir, &entities, &mut errors);
    validate_operations(ir, &entities, &value_objects, &mut errors);
    validate_modules(ir, &mut errors);
    errors.sort_by(|left, right| {
        left.path
            .cmp(&right.path)
            .then(left.message.cmp(&right.message))
    });
    if errors.is_empty() {
        Ok(())
    } else {
        Err(IrValidationErrors(errors))
    }
}
fn entity_index<'ir>(
    ir: &'ir AppIr,
    errors: &mut Vec<IrValidationError>,
) -> BTreeMap<&'ir str, &'ir EntityIr> {
    let mut entities = BTreeMap::new();
    for (index, entity) in ir.entities.iter().enumerate() {
        if entities.insert(entity.id.0.as_str(), entity).is_some() {
            push(
                errors,
                format!("entities[{index}].id"),
                format!("duplicate entity id `{}`", entity.id),
            );
        }
    }
    entities
}
fn validate_entities(
    ir: &AppIr,
    entities: &BTreeMap<&str, &EntityIr>,
    errors: &mut Vec<IrValidationError>,
) {
    let mut field_ids = BTreeSet::new();
    for (entity_index, entity) in ir.entities.iter().enumerate() {
        let path = format!("entities[{entity_index}]");
        let primary_keys = entity
            .fields
            .iter()
            .filter(|field| field.primary_key)
            .count();
        if primary_keys != 1 {
            push(
                errors,
                format!("{path}.fields"),
                format!("must contain exactly one primary key, found {primary_keys}"),
            );
        }
        for (field_index, field) in entity.fields.iter().enumerate() {
            let field_path = format!("{path}.fields[{field_index}]");
            if field.entity != entity.id {
                push(
                    errors,
                    format!("{field_path}.entity"),
                    format!("must reference containing entity `{}`", entity.id),
                );
            }
            if !field_ids.insert(field.id.0.as_str()) {
                push(
                    errors,
                    format!("{field_path}.id"),
                    format!("duplicate field id `{}`", field.id),
                );
            }
            validate_field(field, &field_path, entities, errors);
            if let Some(rule) = &field.read_access {
                validate_field_access_rule(rule, &format!("{field_path}.read_access"), errors);
            }
            if let Some(rule) = &field.write_access {
                validate_field_access_rule(rule, &format!("{field_path}.write_access"), errors);
            }
        }
        for (name, rule) in entity_access(entity) {
            validate_access_rule(rule, entity, &format!("{path}.access.{name}"), errors);
        }
        validate_indexes(entity, &path, errors);
    }
}
fn validate_field_access_rule(
    rule: &AccessRuleIr,
    path: &str,
    errors: &mut Vec<IrValidationError>,
) {
    match rule {
        AccessRuleIr::Owner { .. } => push(
            errors,
            path,
            "owner rules are not supported for field-level access",
        ),
        AccessRuleIr::Any { rules } | AccessRuleIr::All { rules } => {
            for (index, child) in rules.iter().enumerate() {
                validate_field_access_rule(child, &format!("{path}.rules[{index}]"), errors);
            }
        }
        _ => {}
    }
}
fn validate_field(
    field: &FieldIr,
    path: &str,
    entities: &BTreeMap<&str, &EntityIr>,
    errors: &mut Vec<IrValidationError>,
) {
    if let FieldTypeIr::Relation { target } = &field.ty
        && !entities.contains_key(target.0.as_str())
    {
        push(
            errors,
            format!("{path}.type.target"),
            format!("references missing entity `{target}`"),
        );
    }
    if field.generated.is_some()
        && field.default.is_some()
        && !matches!(
            (field.generated, field.default.as_deref()),
            (Some(GeneratedValueIr::Revision), Some("1"))
        )
    {
        push(
            errors,
            path,
            "cannot declare both `generated` and `default`",
        );
    }
    if let Some(generated) = field.generated
        && !generated_compatible(generated, &field.ty)
    {
        push(
            errors,
            format!("{path}.generated"),
            "is incompatible with the field type",
        );
    }
    if let Some(default) = &field.default
        && !default_is_valid(default, &field.ty)
    {
        push(
            errors,
            format!("{path}.default"),
            format!("`{default}` is invalid for the field type"),
        );
    }
    for (name, bound) in [
        ("minimum", field.validation.minimum.as_deref()),
        ("maximum", field.validation.maximum.as_deref()),
    ] {
        if let Some(bound) = bound
            && !numeric_value_is_valid(bound, &field.ty)
        {
            push(
                errors,
                format!("{path}.validation.{name}"),
                format!("`{bound}` is invalid for the field type"),
            );
        }
    }
}
fn validate_relations(
    ir: &AppIr,
    entities: &BTreeMap<&str, &EntityIr>,
    errors: &mut Vec<IrValidationError>,
) {
    let relation_ids = unique_ids(
        ir.relations.iter().map(|relation| relation.id.0.as_str()),
        "relations",
        errors,
    );
    for (index, relation) in ir.relations.iter().enumerate() {
        let path = format!("relations[{index}]");
        validate_relation_endpoint(&relation.source, "source", &path, entities, errors);
        validate_relation_endpoint(&relation.target, "target", &path, entities, errors);
        validate_relation_endpoint(
            &relation.foreign_key_owner,
            "foreign_key_owner",
            &path,
            entities,
            errors,
        );
        if relation.foreign_key_owner != relation.source {
            push(
                errors,
                format!("{path}.foreign_key_owner"),
                "must match the relation source",
            );
        }
        validate_foreign_key_fields(relation, &path, entities, errors);
        if let Some(inverse) = &relation.inverse
            && !relation_ids.contains(inverse.0.as_str())
        {
            push(
                errors,
                format!("{path}.inverse"),
                format!("references missing relation `{}`", inverse.0),
            );
        }
    }
}
fn validate_foreign_key_fields(
    relation: &RelationIr,
    path: &str,
    entities: &BTreeMap<&str, &EntityIr>,
    errors: &mut Vec<IrValidationError>,
) {
    if relation.foreign_key_fields.is_empty() {
        push(
            errors,
            format!("{path}.foreign_key_fields"),
            "must not be empty",
        );
        return;
    }
    let Some(owner) = entities.get(relation.foreign_key_owner.0.as_str()) else {
        return;
    };
    for (field_index, field) in relation.foreign_key_fields.iter().enumerate() {
        if !owner.fields.iter().any(|candidate| candidate.id == *field) {
            push(
                errors,
                format!("{path}.foreign_key_fields[{field_index}]"),
                format!("references missing owner field `{field}`"),
            );
        }
    }
}
fn validate_relation_endpoint(
    entity: &EntityId,
    name: &str,
    path: &str,
    entities: &BTreeMap<&str, &EntityIr>,
    errors: &mut Vec<IrValidationError>,
) {
    if !entities.contains_key(entity.0.as_str()) {
        push(
            errors,
            format!("{path}.{name}"),
            format!("references missing entity `{entity}`"),
        );
    }
}
fn validate_access_rule(
    rule: &AccessRuleIr,
    entity: &EntityIr,
    path: &str,
    errors: &mut Vec<IrValidationError>,
) {
    match rule {
        AccessRuleIr::Owner { field }
            if !entity.fields.iter().any(|candidate| candidate.id == *field) =>
        {
            push(
                errors,
                path,
                format!("references missing owner field `{field}`"),
            );
        }
        AccessRuleIr::Any { rules } | AccessRuleIr::All { rules } => {
            for (index, rule) in rules.iter().enumerate() {
                validate_access_rule(rule, entity, &format!("{path}.rules[{index}]"), errors);
            }
        }
        _ => {}
    }
}
fn entity_access(entity: &EntityIr) -> [(&'static str, &AccessRuleIr); 5] {
    [
        ("list", &entity.access.list),
        ("read", &entity.access.read),
        ("create", &entity.access.create),
        ("update", &entity.access.update),
        ("delete", &entity.access.delete),
    ]
}
pub(super) fn unique_ids<'id>(
    ids: impl IntoIterator<Item = &'id str>,
    collection: &str,
    errors: &mut Vec<IrValidationError>,
) -> BTreeSet<&'id str> {
    let mut unique = BTreeSet::new();
    for (index, id) in ids.into_iter().enumerate() {
        if !unique.insert(id) {
            push(
                errors,
                format!("{collection}[{index}].id"),
                format!("duplicate id `{id}`"),
            );
        }
    }
    unique
}
fn generated_compatible(generated: GeneratedValueIr, ty: &FieldTypeIr) -> bool {
    matches!(
        (generated, ty),
        (
            GeneratedValueIr::UuidV7 | GeneratedValueIr::Tenant,
            FieldTypeIr::Uuid
        ) | (
            GeneratedValueIr::Now,
            FieldTypeIr::Date | FieldTypeIr::Datetime
        ) | (
            GeneratedValueIr::AutoIncrement,
            FieldTypeIr::Integer | FieldTypeIr::Bigint
        ) | (GeneratedValueIr::Revision, FieldTypeIr::Bigint)
    )
}
fn default_is_valid(value: &str, ty: &FieldTypeIr) -> bool {
    match ty {
        FieldTypeIr::Enum { values } => values.iter().any(|candidate| candidate == value),
        FieldTypeIr::Integer => value.parse::<i32>().is_ok(),
        FieldTypeIr::Bigint => value.parse::<i64>().is_ok(),
        FieldTypeIr::Decimal => value.parse::<f64>().is_ok_and(f64::is_finite),
        FieldTypeIr::Boolean => value.parse::<bool>().is_ok(),
        FieldTypeIr::Relation { .. } => false,
        _ => true,
    }
}
fn numeric_value_is_valid(value: &str, ty: &FieldTypeIr) -> bool {
    match ty {
        FieldTypeIr::Integer => value.parse::<i32>().is_ok(),
        FieldTypeIr::Bigint => value.parse::<i64>().is_ok(),
        FieldTypeIr::Decimal => value.parse::<f64>().is_ok_and(f64::is_finite),
        _ => false,
    }
}

pub(super) fn push(
    errors: &mut Vec<IrValidationError>,
    path: impl Into<String>,
    message: impl Into<String>,
) {
    errors.push(IrValidationError {
        path: path.into(),
        message: message.into(),
    });
}
