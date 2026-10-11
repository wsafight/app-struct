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
    let filterable = json.then(|| quote! { filterable = false, });
    let mut operators = vec![quote! { Eq }];
    if !json {
        operators.push(quote! { InList });
    }
    if super::query::filters::supports_range(&field.ty) {
        operators.extend([quote! { Ge }, quote! { Le }]);
    }
    let scalar = if matches!(field.generated, Some(GeneratedValueIr::Revision)) {
        TokenStream::new()
    } else {
        super::scalar::attributes(&field.ty, u8::from(field.nullable))
    };
    Ok(quote! {
        #[sqlx(rename = #column)]
        #[search_field(column = #column, #sortable #cursor #filterable operators = [#(#operators),*])]
        #scalar pub #name: #ty
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
