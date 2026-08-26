use crate::{Artifact, ArtifactKind, CodegenError, generated_header};
use appstruct_ir::{
    AppIr, EntityIr, FieldIr, FieldTypeIr, GeneratedValueIr, OnDeleteIr, RelationIr,
};
use serde_json::{Value, json};

pub(crate) fn plan(ir: &AppIr) -> Result<Vec<Artifact>, CodegenError> {
    let schema = schema_json(ir);
    let mut schema_text = serde_json::to_string_pretty(&schema)?;
    schema_text.push('\n');
    Ok(vec![
        Artifact::text(
            "database/schema.json",
            schema_text,
            ArtifactKind::DatabaseSchema,
        ),
        Artifact::text(
            "database/0001_initial.sql",
            initial_migration(ir)?,
            ArtifactKind::Migration,
        ),
    ])
}

pub(crate) fn initial_migration(ir: &AppIr) -> Result<String, CodegenError> {
    let mut statements = vec![generated_header("--")];
    for entity in &ir.entities {
        statements.push(create_table(ir, entity)?);
    }
    for relation in &ir.relations {
        statements.push(create_foreign_key(ir, relation)?);
    }
    Ok(format!("{}\n", statements.join("\n")))
}

fn create_table(ir: &AppIr, entity: &EntityIr) -> Result<String, CodegenError> {
    let columns = entity
        .fields
        .iter()
        .map(|field| column_definition(ir, field))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(format!(
        "CREATE TABLE {} (\n    {}\n);\n",
        quote_ident(&entity.table_name),
        columns.join(",\n    ")
    ))
}

fn column_definition(ir: &AppIr, field: &FieldIr) -> Result<String, CodegenError> {
    let mut fragments = vec![quote_ident(&field.column_name), sql_type(ir, &field.ty)?];
    if !field.nullable {
        fragments.push("NOT NULL".to_owned());
    }
    if field.primary_key {
        fragments.push("PRIMARY KEY".to_owned());
    }
    if let Some(default) = sql_default(field) {
        fragments.push(format!("DEFAULT {default}"));
    }
    if let FieldTypeIr::Enum { values } = &field.ty {
        let choices = values
            .iter()
            .map(|value| quote_literal(value))
            .collect::<Vec<_>>()
            .join(", ");
        fragments.push(format!(
            "CHECK ({} IN ({choices}))",
            quote_ident(&field.column_name)
        ));
    }
    Ok(fragments.join(" "))
}

fn sql_type(ir: &AppIr, field_type: &FieldTypeIr) -> Result<String, CodegenError> {
    let value = match field_type {
        FieldTypeIr::Uuid => "UUID",
        FieldTypeIr::String | FieldTypeIr::Text | FieldTypeIr::Enum { .. } => "TEXT",
        FieldTypeIr::Integer => "INTEGER",
        FieldTypeIr::Bigint => "BIGINT",
        FieldTypeIr::Decimal => "NUMERIC",
        FieldTypeIr::Boolean => "BOOLEAN",
        FieldTypeIr::Date => "DATE",
        FieldTypeIr::Datetime => "TIMESTAMPTZ",
        FieldTypeIr::Json => "JSONB",
        FieldTypeIr::Relation { target } => {
            let target = ir
                .entities
                .iter()
                .find(|entity| entity.id == *target)
                .ok_or_else(|| CodegenError::new(format!("missing relation target `{target}`")))?;
            let primary_key = target
                .fields
                .iter()
                .find(|field| field.primary_key)
                .ok_or_else(|| {
                    CodegenError::new(format!("entity `{}` has no primary key", target.id))
                })?;
            return sql_type(ir, &primary_key.ty);
        }
    };
    Ok(value.to_owned())
}

fn sql_default(field: &FieldIr) -> Option<String> {
    if matches!(field.generated, Some(GeneratedValueIr::Now)) {
        return Some("CURRENT_TIMESTAMP".to_owned());
    }
    let value = field.default.as_ref()?;
    Some(match field.ty {
        FieldTypeIr::Boolean
        | FieldTypeIr::Integer
        | FieldTypeIr::Bigint
        | FieldTypeIr::Decimal => value.clone(),
        _ => quote_literal(value),
    })
}

fn create_foreign_key(ir: &AppIr, relation: &RelationIr) -> Result<String, CodegenError> {
    let source = entity(ir, &relation.source.0)?;
    let target = entity(ir, &relation.target.0)?;
    let source_field = source
        .fields
        .iter()
        .find(|field| relation.foreign_key_fields.contains(&field.id))
        .ok_or_else(|| CodegenError::new(format!("missing foreign key for `{}`", relation.id.0)))?;
    let target_field = target
        .fields
        .iter()
        .find(|field| field.primary_key)
        .ok_or_else(|| CodegenError::new(format!("entity `{}` has no primary key", target.id)))?;
    let action = match relation.on_delete {
        OnDeleteIr::Restrict => "RESTRICT",
        OnDeleteIr::Cascade => "CASCADE",
        OnDeleteIr::SetNull => "SET NULL",
    };
    let constraint = format!("fk_{}_{}", source.table_name, source_field.column_name);
    Ok(format!(
        "ALTER TABLE {} ADD CONSTRAINT {} FOREIGN KEY ({}) REFERENCES {} ({}) ON DELETE {action};\n",
        quote_ident(&source.table_name),
        quote_ident(&constraint),
        quote_ident(&source_field.column_name),
        quote_ident(&target.table_name),
        quote_ident(&target_field.column_name),
    ))
}

fn entity<'ir>(ir: &'ir AppIr, id: &str) -> Result<&'ir EntityIr, CodegenError> {
    ir.entities
        .iter()
        .find(|entity| entity.id.0 == id)
        .ok_or_else(|| CodegenError::new(format!("missing entity `{id}`")))
}

fn schema_json(ir: &AppIr) -> Value {
    let tables = ir
        .entities
        .iter()
        .map(|entity| {
            json!({
                "id": entity.id,
                "name": entity.table_name,
                "columns": entity.fields.iter().map(|field| json!({
                    "id": field.id,
                    "name": field.column_name,
                    "type": field.ty,
                    "nullable": field.nullable,
                    "primary_key": field.primary_key,
                    "default": field.default,
                    "generated": field.generated,
                })).collect::<Vec<_>>()
            })
        })
        .collect::<Vec<_>>();
    json!({
        "schema_version": 1,
        "provider": ir.database.provider,
        "tables": tables,
        "relations": ir.relations,
    })
}

fn quote_ident(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\"\""))
}

fn quote_literal(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quotes_sql_values() {
        assert_eq!(quote_ident("project\"name"), "\"project\"\"name\"");
        assert_eq!(quote_literal("it's"), "'it''s'");
    }
}
