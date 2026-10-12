use super::{optional_type, parse_ident, render, rust_type};
use crate::CodegenError;
use appstruct_ir::{AppIr, EntityIr, FieldIr, FieldTypeIr, GeneratedValueIr};
use proc_macro2::TokenStream;
use quote::{format_ident, quote};

#[allow(clippy::too_many_lines)]
pub(super) fn source(_ir: &AppIr, entity: &EntityIr) -> Result<String, CodegenError> {
    let table = &entity.table_name;
    let fields = entity
        .fields
        .iter()
        .map(entity_field)
        .collect::<Result<Vec<_>, _>>()?;
    let projection = projection_support(entity)?;
    let columns = entity
        .fields
        .iter()
        .map(|field| {
            let variant = super::query::helpers::column_ident(field)?;
            let column = &field.column_name;
            Ok(quote! { #[iden = #column] #variant })
        })
        .collect::<Result<Vec<_>, CodegenError>>()?;
    let primary = super::query::filters::primary_key(entity)?;
    let primary_name = parse_ident(&primary.rust_name)?;
    let primary_column = super::query::helpers::column_ident(primary)?;
    let primary_type = rust_type(&primary.ty);
    let order = format!("{}:asc", primary.rust_name);
    let insert_fields = entity
        .fields
        .iter()
        .filter(|f| !matches!(f.generated, Some(GeneratedValueIr::AutoIncrement)))
        .collect::<Vec<_>>();
    let insert_sql = format!(
        "INSERT INTO {} ({}) VALUES ({}) RETURNING *",
        identifier(table),
        insert_fields
            .iter()
            .map(|f| identifier(&f.column_name))
            .collect::<Vec<_>>()
            .join(", "),
        (1..=insert_fields.len())
            .map(|i| format!("${i}"))
            .collect::<Vec<_>>()
            .join(", ")
    );
    let inserts = bind_fields(&insert_fields)?;
    let update_fields = entity
        .fields
        .iter()
        .filter(|f| !f.primary_key)
        .collect::<Vec<_>>();
    let update_sql = format!(
        "UPDATE {} SET {} WHERE {} = ${} RETURNING *",
        identifier(table),
        update_fields
            .iter()
            .enumerate()
            .map(|(i, f)| format!("{} = ${}", identifier(&f.column_name), i + 1))
            .collect::<Vec<_>>()
            .join(", "),
        identifier(&primary.column_name),
        update_fields.len() + 1
    );
    let updates = bind_fields(&update_fields)?;
    let delete_sql = format!(
        "DELETE FROM {} WHERE {} = $1",
        identifier(table),
        identifier(&primary.column_name)
    );
    let comparisons = ["eq", "ne", "gt", "gte", "lt", "lte"].iter().map(|name| {
        let method = format_ident!("{name}");
        quote! { pub fn #method(self, value: impl Into<sea_query::Value>) -> sea_query::SimpleExpr { self.expression().#method(value) } }
    });
    let aggregates = ["count", "sum", "avg", "min", "max"].iter().map(|name| {
        let method = format_ident!("{name}");
        quote! { pub fn #method(self) -> sea_query::SimpleExpr { self.expression().#method() } }
    });
    render(quote! {
        use sea_query::{ExprTrait as _, Iden};
        use serde::{Deserialize, Serialize};
        use crate::database::{DatabaseExecutor, DbErr, Select, Sql};
        pub const TABLE: &str = #table;
        #[derive(Clone, Debug, Default, PartialEq, sqlx::FromRow, sift_query::SearchEntity, Serialize, Deserialize)]
        #[search_entity(from = #table, default_order = #order, stable_order = #order)]
        pub struct Model { #(#fields,)* }
        #projection
        #[derive(Clone, Copy, Debug, Iden)]
        pub enum Column { #(#columns,)* }
        impl Column {
            fn expression(self) -> sea_query::Expr { sea_query::Expr::col((sea_query::Alias::new(TABLE), self)) }
            #(#comparisons)*
            #(#aggregates)*
            pub fn is_null(self) -> sea_query::SimpleExpr { self.expression().is_null() }
            pub fn is_not_null(self) -> sea_query::SimpleExpr { self.expression().is_not_null() }
            pub fn is_in(self, values: impl IntoIterator<Item = impl Into<sea_query::Value>>) -> sea_query::SimpleExpr { self.expression().is_in(values) }
            pub fn in_subquery(self, query: sea_query::SelectStatement) -> sea_query::SimpleExpr { self.expression().in_subquery(query) }
            pub fn like(self, value: sea_query::LikeExpr) -> sea_query::SimpleExpr { self.expression().like(value) }
        }
        impl From<Column> for sea_query::SimpleExpr {
            fn from(column: Column) -> Self { column.expression().into() }
        }
        pub fn select() -> Select<Model> { Select::new(TABLE) }
        pub fn select_by_id(id: #primary_type) -> Select<Model> { select().filter(Column::#primary_column.eq(id)) }
        impl Model {
            pub async fn insert(self, db: &impl DatabaseExecutor) -> Result<Self, DbErr> {
                crate::database::fetch_model(db, Sql::bind(#insert_sql, vec![#(#inserts,)*])).await
            }
            pub async fn update(self, db: &impl DatabaseExecutor) -> Result<Self, DbErr> {
                crate::database::fetch_model(db, Sql::bind(#update_sql, vec![#(#updates,)* self.#primary_name.into()])).await
            }
            pub async fn delete(self, db: &impl DatabaseExecutor) -> Result<(), DbErr> {
                db.execute(Sql::bind(#delete_sql, [self.#primary_name.into()])).await?;
                Ok(())
            }
        }
    })
}
fn entity_field(field: &FieldIr) -> Result<TokenStream, CodegenError> {
    let name = parse_ident(&field.rust_name)?;
    let ty = optional_type(rust_type(&field.ty), field.nullable);
    let column = &field.column_name;
    let sortable = (field.primary_key || field.capabilities.sortable).then(|| quote! { sortable, });
    let cursor = (field.primary_key && !field.nullable).then(|| quote! { cursor_non_null, });
    let json = matches!(field.ty, FieldTypeIr::Json);
    let filterable = field.capabilities.filterable;
    let custom = json
        .then(|| quote! { custom_operators = ["json_contains", "json_contained_by", "json_key"], });
    let mut operators = vec![
        quote! { Eq },
        quote! { Ne },
        quote! { IsNull },
        quote! { NotNull },
    ];
    if !json {
        operators.extend([quote! { InList }, quote! { NotIn }]);
    }
    if super::query::filters::supports_range(&field.ty) {
        operators.extend([quote! { Gt }, quote! { Ge }, quote! { Lt }, quote! { Le }]);
    }
    if matches!(
        field.ty,
        FieldTypeIr::String | FieldTypeIr::Text | FieldTypeIr::Enum { .. }
    ) {
        operators.extend([quote! { Contain }, quote! { StartWith }, quote! { EndWith }]);
    }
    let scalar = if matches!(field.generated, Some(GeneratedValueIr::Revision)) {
        TokenStream::new()
    } else {
        super::scalar::attributes(&field.ty, u8::from(field.nullable))
    };
    Ok(quote! {
        #[sqlx(rename = #column)]
        #[search_field(column = #column, #sortable #cursor filterable = #filterable, #custom operators = [#(#operators),*])]
        #scalar pub #name: #ty
    })
}

fn projection_support(entity: &EntityIr) -> Result<TokenStream, CodegenError> {
    let mut fields = Vec::new();
    let mut columns = Vec::new();
    let mut cursor_values = Vec::new();
    for field in &entity.fields {
        let name = parse_ident(&field.rust_name)?;
        let ty = rust_type(&field.ty);
        let column = &field.column_name;
        let key = &field.rust_name;
        let variant = super::query::helpers::column_ident(field)?;
        let scalar = super::scalar::attributes(&field.ty, 1);
        fields.push(quote! {
            #[sqlx(default, rename = #column)]
            #scalar pub #name: Option<#ty>
        });
        columns.push(quote! { #key => { select = select.column(Column::#variant); } });
        if !matches!(field.ty, FieldTypeIr::Json) {
            cursor_values.push(quote! { #key => self.#name.clone().map_or(sift_query::CursorValue::Null, |value| sift_query::CursorValue::Scalar(sift_query::IntoScalarValue::into_scalar_value(value))) });
        }
    }
    Ok(quote! {
        #[derive(Clone, Debug, sqlx::FromRow, Serialize)]
        pub struct ProjectedModel { #(#fields,)* }
        impl ProjectedModel {
            pub fn cursor_values(&self, order: &[sift_query::OrderBy]) -> Result<Vec<sift_query::CursorValue>, crate::ApiError> {
                use sift_query::SearchEntity as _;
                order.iter().map(|item| {
                    let field = Model::META.field(item.field).ok_or(crate::ApiError::Internal)?;
                    Ok(match field.public_name {
                        #(#cursor_values,)*
                        _ => return Err(crate::ApiError::InvalidQuery("field cannot be used for cursor ordering".to_owned())),
                    })
                }).collect()
            }
            pub fn json(self, fields: &[String]) -> Result<serde_json::Value, crate::ApiError> {
                let mut value = serde_json::to_value(self).map_err(|_| crate::ApiError::Internal)?;
                value.as_object_mut().ok_or(crate::ApiError::Internal)?.retain(|key, _| fields.contains(key));
                Ok(value)
            }
        }
        pub fn projected(select: Select<Model>, fields: &[String], order: &[sift_query::OrderBy]) -> Result<Select<ProjectedModel>, crate::ApiError> {
            use sift_query::SearchEntity as _;
            let mut selected = fields.to_vec();
            for item in order {
                let field = Model::META.field(item.field).ok_or(crate::ApiError::Internal)?;
                if !selected.iter().any(|name| name == field.public_name) { selected.push(field.public_name.to_owned()); }
            }
            let mut select = select.select_only();
            for field in &selected {
                match field.as_str() { #(#columns,)* _ => return Err(crate::ApiError::Internal) }
            }
            Ok(select.projection())
        }
    })
}
fn bind_fields(fields: &[&FieldIr]) -> Result<Vec<TokenStream>, CodegenError> {
    fields
        .iter()
        .map(|field| {
            let name = parse_ident(&field.rust_name)?;
            Ok(quote! { self.#name.into() })
        })
        .collect()
}
fn identifier(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}
