use super::{find_entity, module_name, optional_type, parse_ident, render, rust_type};
use crate::CodegenError;
use appstruct_ir::{AppIr, Cardinality, EntityIr, FieldIr, GeneratedValueIr, RelationIr};
use proc_macro2::{Span, TokenStream};
use quote::quote;
use syn::LitStr;

pub(super) fn source(ir: &AppIr, entity: &EntityIr) -> Result<String, CodegenError> {
    let table = LitStr::new(&entity.table_name, Span::call_site());
    let fields = entity
        .fields
        .iter()
        .map(entity_field)
        .collect::<Result<Vec<_>, _>>()?;
    let relations = relation_fields(ir, entity)?;
    render(quote! {
        use sea_orm::entity::prelude::*;
        use serde::{Deserialize, Serialize};

        #[sea_orm::model]
        #[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
        #[sea_orm(table_name = #table)]
        pub struct Model {
            #(#fields,)*
            #(#relations,)*
        }

        impl ActiveModelBehavior for ActiveModel {}
    })
}

fn entity_field(field: &FieldIr) -> Result<TokenStream, CodegenError> {
    let name = parse_ident(&field.rust_name)?;
    let ty = optional_type(rust_type(&field.ty), field.nullable);
    let column = LitStr::new(&field.column_name, Span::call_site());
    let attributes = if field.primary_key {
        let auto_increment = matches!(field.generated, Some(GeneratedValueIr::AutoIncrement));
        quote! { #[sea_orm(primary_key, auto_increment = #auto_increment, column_name = #column)] }
    } else if field.unique {
        quote! { #[sea_orm(unique, column_name = #column)] }
    } else {
        quote! { #[sea_orm(column_name = #column)] }
    };
    let scalar = if matches!(field.generated, Some(GeneratedValueIr::Revision)) {
        TokenStream::new()
    } else {
        super::scalar::attributes(&field.ty, u8::from(field.nullable))
    };
    Ok(quote! { #attributes #scalar pub #name: #ty })
}

fn relation_fields(ir: &AppIr, entity: &EntityIr) -> Result<Vec<TokenStream>, CodegenError> {
    let mut fields = Vec::new();
    for relation in &ir.relations {
        if relation.source == entity.id {
            fields.push(belongs_to_field(ir, entity, relation)?);
        }
        if relation.target == entity.id {
            fields.push(inverse_relation_field(ir, relation)?);
        }
    }
    Ok(fields)
}

fn belongs_to_field(
    ir: &AppIr,
    entity: &EntityIr,
    relation: &RelationIr,
) -> Result<TokenStream, CodegenError> {
    let source_field = entity
        .fields
        .iter()
        .find(|field| relation.foreign_key_fields.contains(&field.id))
        .ok_or_else(|| CodegenError::new(format!("missing field for `{}`", relation.id.0)))?;
    let target = find_entity(ir, &relation.target.0)?;
    let target_key = target
        .fields
        .iter()
        .find(|field| field.primary_key)
        .ok_or_else(|| CodegenError::new(format!("missing key for `{}`", target.id)))?;
    let relation_name = source_field
        .rust_name
        .strip_suffix("_id")
        .unwrap_or(&source_field.rust_name);
    let name = parse_ident(relation_name)?;
    let from = LitStr::new(&source_field.rust_name, Span::call_site());
    let to = LitStr::new(&target_key.rust_name, Span::call_site());
    let target_module = parse_ident(&module_name(target))?;
    Ok(quote! {
        #[sea_orm(belongs_to, from = #from, to = #to)]
        pub #name: BelongsTo<super::#target_module::Entity>
    })
}

fn inverse_relation_field(ir: &AppIr, relation: &RelationIr) -> Result<TokenStream, CodegenError> {
    let source = find_entity(ir, &relation.source.0)?;
    let source_module = parse_ident(&module_name(source))?;
    match relation.cardinality {
        Cardinality::OneToOne => {
            let name = source_module.clone();
            Ok(quote! {
                #[sea_orm(has_one)]
                pub #name: HasOne<super::#source_module::Entity>
            })
        }
        Cardinality::ManyToOne => {
            let name = parse_ident(&source.table_name)?;
            Ok(quote! {
                #[sea_orm(has_many)]
                pub #name: HasMany<super::#source_module::Entity>
            })
        }
    }
}
