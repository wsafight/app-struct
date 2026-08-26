use crate::{Artifact, ArtifactKind, CodegenError, format_rust, generated_header};
use appstruct_ir::{AppIr, EntityIr, FieldIr, FieldTypeIr, GeneratedValueIr};
use proc_macro2::{Ident, Span, TokenStream};
use quote::quote;
use syn::LitStr;

pub(crate) fn plan(ir: &AppIr) -> Result<Vec<Artifact>, CodegenError> {
    let mut artifacts = vec![
        Artifact::text(
            "backend/Cargo.toml",
            cargo_manifest(),
            ArtifactKind::RustManifest,
        ),
        Artifact::text(
            "backend/src/main.rs",
            rust_template(include_str!("../templates/backend/main.rs"))?,
            ArtifactKind::RustSource,
        ),
        Artifact::text(
            "backend/src/error.rs",
            rust_template(include_str!("../templates/backend/error.rs"))?,
            ArtifactKind::RustSource,
        ),
        Artifact::text(
            "backend/src/entities/mod.rs",
            entity_module(ir)?,
            ArtifactKind::RustSource,
        ),
        Artifact::text(
            "backend/src/api/mod.rs",
            api_module(ir)?,
            ArtifactKind::RustSource,
        ),
        Artifact::text(
            "backend/src/lib.rs",
            library_source(ir)?,
            ArtifactKind::RustSource,
        ),
        Artifact::text(
            "backend/src/openapi.rs",
            crate::openapi::rust_source(ir)?,
            ArtifactKind::RustSource,
        ),
    ];
    for entity in &ir.entities {
        let module = module_name(entity);
        artifacts.push(Artifact::text(
            format!("backend/src/entities/{module}.rs"),
            entity_source(ir, entity)?,
            ArtifactKind::RustSource,
        ));
        artifacts.push(Artifact::text(
            format!("backend/src/api/{module}.rs"),
            api_source(entity)?,
            ArtifactKind::RustSource,
        ));
    }
    Ok(artifacts)
}

fn cargo_manifest() -> &'static str {
    concat!(
        "[package]\n",
        "name = \"appstruct-generated-backend\"\n",
        "version = \"0.0.0\"\n",
        "edition = \"2024\"\n",
        "rust-version = \"1.98\"\n\n",
        "[dependencies]\n",
        "axum = \"0.8.9\"\n",
        "chrono = { version = \"0.4.45\", features = [\"serde\"] }\n",
        "rust_decimal = { version = \"1.42.1\", features = [\"serde-with-str\"] }\n",
        "sea-orm = { version = \"2.0.2\", default-features = false, features = [\"macros\", \"runtime-tokio-rustls\", \"sqlx-postgres\", \"with-chrono\", \"with-json\", \"with-rust_decimal\", \"with-uuid\"] }\n",
        "serde = { version = \"1.0.229\", features = [\"derive\"] }\n",
        "serde_json = \"1.0.151\"\n",
        "tokio = { version = \"1.53.1\", features = [\"macros\", \"net\", \"rt-multi-thread\"] }\n",
        "tower-http = { version = \"0.7.0\", features = [\"cors\", \"trace\"] }\n",
        "tracing = \"0.1.44\"\n",
        "tracing-subscriber = { version = \"0.3.22\", features = [\"env-filter\", \"fmt\"] }\n",
        "uuid = { version = \"1.25.0\", features = [\"serde\", \"v7\"] }\n",
    )
}

fn entity_module(ir: &AppIr) -> Result<String, CodegenError> {
    let modules = ir
        .entities
        .iter()
        .map(|entity| format!("pub mod {};", module_name(entity)))
        .collect::<Vec<_>>()
        .join("\n");
    format_rust(&format!("{}{}\n", generated_header("//"), modules))
}

fn api_module(ir: &AppIr) -> Result<String, CodegenError> {
    entity_module(ir)
}

fn library_source(ir: &AppIr) -> Result<String, CodegenError> {
    let routes = ir
        .entities
        .iter()
        .map(|entity| {
            let module = parse_ident(&module_name(entity))?;
            let path = LitStr::new(&format!("/api/{}/", entity.table_name), Span::call_site());
            Ok(quote! { .nest(#path, api::#module::router()) })
        })
        .collect::<Result<Vec<_>, CodegenError>>()?;
    render(quote! {
        pub mod api;
        pub mod entities;
        mod error;
        mod openapi;

        pub use error::{ApiError, FieldViolation};

        use axum::{Router, http::StatusCode, response::IntoResponse, routing::get};
        use sea_orm::DatabaseConnection;
        use tower_http::{cors::CorsLayer, trace::TraceLayer};

        #[derive(Clone)]
        pub struct AppState {
            pub database: DatabaseConnection,
        }

        pub fn router(database: DatabaseConnection) -> Router {
            Router::new()
                #(#routes)*
                .route("/health/live", get(health))
                .route("/openapi.json", get(openapi))
                .layer(CorsLayer::permissive())
                .layer(TraceLayer::new_for_http())
                .with_state(AppState { database })
        }

        async fn health() -> StatusCode {
            StatusCode::NO_CONTENT
        }

        async fn openapi() -> impl IntoResponse {
            ([
                (axum::http::header::CONTENT_TYPE, "application/json"),
            ], openapi::OPENAPI_JSON)
        }
    })
}

fn entity_source(ir: &AppIr, entity: &EntityIr) -> Result<String, CodegenError> {
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
    let base_type = rust_type(&field.ty);
    let ty = optional_type(base_type, field.nullable);
    let column = LitStr::new(&field.column_name, Span::call_site());
    let attributes = if field.primary_key {
        let auto_increment = matches!(field.generated, Some(GeneratedValueIr::AutoIncrement));
        quote! { #[sea_orm(primary_key, auto_increment = #auto_increment, column_name = #column)] }
    } else {
        quote! { #[sea_orm(column_name = #column)] }
    };
    Ok(quote! { #attributes pub #name: #ty })
}

fn relation_fields(ir: &AppIr, entity: &EntityIr) -> Result<Vec<TokenStream>, CodegenError> {
    let mut fields = Vec::new();
    for relation in ir
        .relations
        .iter()
        .filter(|relation| relation.source == entity.id)
    {
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
        fields.push(quote! {
            #[sea_orm(belongs_to, from = #from, to = #to)]
            pub #name: BelongsTo<super::#target_module::Entity>
        });
    }
    Ok(fields)
}

#[allow(clippy::too_many_lines)]
fn api_source(entity: &EntityIr) -> Result<String, CodegenError> {
    let module = parse_ident(&module_name(entity))?;
    let create_fields = create_fields(entity)?;
    let update_fields = update_fields(entity)?;
    let create_values = create_values(entity)?;
    let active_default = entity
        .fields
        .iter()
        .any(|field| matches!(field.generated, Some(GeneratedValueIr::AutoIncrement)))
        .then(|| quote! { ..Default::default() });
    let updates = update_values(entity)?;
    let create_validation = validation_rules(entity, false)?;
    let update_validation = validation_rules(entity, true)?;
    let primary_key = primary_key(entity)?;
    let parse_id = parse_id_expression(primary_key);
    render(quote! {
        use crate::{AppState, ApiError, FieldViolation, entities::#module};
        use axum::{
            Json, Router,
            extract::{Path, State},
            http::StatusCode,
            routing::get,
        };
        use sea_orm::{
            ActiveModelTrait, ActiveValue::Set, EntityTrait, IntoActiveModel, ModelTrait,
        };
        use serde::Deserialize;

        #[derive(Debug, Deserialize)]
        pub struct CreateInput {
            #(#create_fields,)*
        }

        #[derive(Debug, Deserialize)]
        pub struct UpdateInput {
            #(#update_fields,)*
        }

        pub fn router() -> Router<AppState> {
            Router::new()
                .route("/", get(list).post(create))
                .route("/{id}", get(read).patch(update).delete(delete))
        }

        async fn list(State(state): State<AppState>) -> Result<Json<Vec<#module::Model>>, ApiError> {
            let models = #module::Entity::find().all(&state.database).await?;
            Ok(Json(models))
        }

        async fn read(
            State(state): State<AppState>,
            Path(id): Path<String>,
        ) -> Result<Json<#module::Model>, ApiError> {
            let id = #parse_id;
            let model = #module::Entity::find_by_id(id)
                .one(&state.database)
                .await?
                .ok_or(ApiError::NotFound)?;
            Ok(Json(model))
        }

        async fn create(
            State(state): State<AppState>,
            Json(input): Json<CreateInput>,
        ) -> Result<(StatusCode, Json<#module::Model>), ApiError> {
            validate_create(&input)?;
            let active = #module::ActiveModel {
                #(#create_values,)*
                #active_default
            };
            let model = active.insert(&state.database).await?;
            Ok((StatusCode::CREATED, Json(model)))
        }

        async fn update(
            State(state): State<AppState>,
            Path(id): Path<String>,
            Json(input): Json<UpdateInput>,
        ) -> Result<Json<#module::Model>, ApiError> {
            validate_update(&input)?;
            let id = #parse_id;
            let model = #module::Entity::find_by_id(id)
                .one(&state.database)
                .await?
                .ok_or(ApiError::NotFound)?;
            let mut active = model.into_active_model();
            #(#updates)*
            Ok(Json(active.update(&state.database).await?))
        }

        async fn delete(
            State(state): State<AppState>,
            Path(id): Path<String>,
        ) -> Result<StatusCode, ApiError> {
            let id = #parse_id;
            let model = #module::Entity::find_by_id(id)
                .one(&state.database)
                .await?
                .ok_or(ApiError::NotFound)?;
            model.delete(&state.database).await?;
            Ok(StatusCode::NO_CONTENT)
        }

        fn validate_create(input: &CreateInput) -> Result<(), ApiError> {
            let mut violations = Vec::new();
            #(#create_validation)*
            finish_validation(violations)
        }

        fn validate_update(input: &UpdateInput) -> Result<(), ApiError> {
            let mut violations = Vec::new();
            #(#update_validation)*
            finish_validation(violations)
        }

        fn finish_validation(violations: Vec<FieldViolation>) -> Result<(), ApiError> {
            if violations.is_empty() {
                Ok(())
            } else {
                Err(ApiError::Validation(violations))
            }
        }
    })
}

fn create_fields(entity: &EntityIr) -> Result<Vec<TokenStream>, CodegenError> {
    writable_create(entity)
        .map(|field| dto_field(field, false))
        .collect()
}

fn update_fields(entity: &EntityIr) -> Result<Vec<TokenStream>, CodegenError> {
    writable_update(entity)
        .map(|field| dto_field(field, true))
        .collect()
}

fn dto_field(field: &FieldIr, update: bool) -> Result<TokenStream, CodegenError> {
    let name = parse_ident(&field.rust_name)?;
    let mut ty = rust_type(&field.ty);
    if field.nullable {
        ty = quote! { Option<#ty> };
    }
    if update {
        ty = quote! { Option<#ty> };
    }
    Ok(quote! { pub #name: #ty })
}

fn create_values(entity: &EntityIr) -> Result<Vec<TokenStream>, CodegenError> {
    entity
        .fields
        .iter()
        .filter_map(|field| create_value(field).transpose())
        .collect()
}

fn create_value(field: &FieldIr) -> Result<Option<TokenStream>, CodegenError> {
    let name = parse_ident(&field.rust_name)?;
    let value = match field.generated {
        Some(GeneratedValueIr::UuidV7) => quote! { uuid::Uuid::now_v7() },
        Some(GeneratedValueIr::Now) if matches!(field.ty, FieldTypeIr::Date) => {
            quote! { chrono::Utc::now().date_naive() }
        }
        Some(GeneratedValueIr::Now) => quote! { chrono::Utc::now() },
        Some(GeneratedValueIr::AutoIncrement) => return Ok(None),
        None => quote! { input.#name },
    };
    Ok(Some(quote! { #name: Set(#value) }))
}

fn update_values(entity: &EntityIr) -> Result<Vec<TokenStream>, CodegenError> {
    writable_update(entity)
        .map(|field| {
            let name = parse_ident(&field.rust_name)?;
            Ok(quote! {
                if let Some(value) = input.#name {
                    active.#name = Set(value);
                }
            })
        })
        .collect()
}

fn validation_rules(entity: &EntityIr, update: bool) -> Result<Vec<TokenStream>, CodegenError> {
    let fields = if update {
        writable_update(entity).collect::<Vec<_>>()
    } else {
        writable_create(entity).collect::<Vec<_>>()
    };
    let mut rules = Vec::new();
    for field in fields {
        rules.extend(field_validation(field, update)?);
    }
    Ok(rules)
}

fn field_validation(field: &FieldIr, update: bool) -> Result<Vec<TokenStream>, CodegenError> {
    if !matches!(field.ty, FieldTypeIr::String | FieldTypeIr::Text) {
        return Ok(Vec::new());
    }
    let name = parse_ident(&field.rust_name)?;
    let field_name = LitStr::new(&field.api_name, Span::call_site());
    let value = if update && field.nullable {
        quote! { input.#name.as_ref().and_then(Option::as_ref) }
    } else if update || field.nullable {
        quote! { input.#name.as_ref() }
    } else {
        quote! { Some(&input.#name) }
    };
    let mut rules = Vec::new();
    if let Some(limit) = field.validation.min_length {
        let message = LitStr::new(
            &format!("must contain at least {limit} characters"),
            Span::call_site(),
        );
        rules.push(quote! {
            if let Some(value) = #value && value.chars().count() < #limit as usize {
                violations.push(FieldViolation { field: #field_name.to_owned(), message: #message.to_owned() });
            }
        });
    }
    if let Some(limit) = field.validation.max_length {
        let message = LitStr::new(
            &format!("must contain at most {limit} characters"),
            Span::call_site(),
        );
        rules.push(quote! {
            if let Some(value) = #value && value.chars().count() > #limit as usize {
                violations.push(FieldViolation { field: #field_name.to_owned(), message: #message.to_owned() });
            }
        });
    }
    Ok(rules)
}

fn writable_create(entity: &EntityIr) -> impl Iterator<Item = &FieldIr> {
    entity
        .fields
        .iter()
        .filter(|field| field.generated.is_none())
}

fn writable_update(entity: &EntityIr) -> impl Iterator<Item = &FieldIr> {
    entity
        .fields
        .iter()
        .filter(|field| !field.primary_key && field.generated.is_none())
}

fn primary_key(entity: &EntityIr) -> Result<&FieldIr, CodegenError> {
    entity
        .fields
        .iter()
        .find(|field| field.primary_key)
        .ok_or_else(|| CodegenError::new(format!("entity `{}` has no primary key", entity.id)))
}

fn parse_id_expression(field: &FieldIr) -> TokenStream {
    match field.ty {
        FieldTypeIr::Uuid | FieldTypeIr::Relation { .. } => {
            quote! { uuid::Uuid::parse_str(&id).map_err(|_| ApiError::InvalidId)? }
        }
        FieldTypeIr::Integer => quote! { id.parse::<i32>().map_err(|_| ApiError::InvalidId)? },
        FieldTypeIr::Bigint => quote! { id.parse::<i64>().map_err(|_| ApiError::InvalidId)? },
        _ => quote! { id },
    }
}

fn rust_type(field_type: &FieldTypeIr) -> TokenStream {
    match field_type {
        FieldTypeIr::Uuid | FieldTypeIr::Relation { .. } => quote! { uuid::Uuid },
        FieldTypeIr::String | FieldTypeIr::Text | FieldTypeIr::Enum { .. } => quote! { String },
        FieldTypeIr::Integer => quote! { i32 },
        FieldTypeIr::Bigint => quote! { i64 },
        FieldTypeIr::Decimal => quote! { rust_decimal::Decimal },
        FieldTypeIr::Boolean => quote! { bool },
        FieldTypeIr::Date => quote! { chrono::NaiveDate },
        FieldTypeIr::Datetime => quote! { chrono::DateTime<chrono::Utc> },
        FieldTypeIr::Json => quote! { serde_json::Value },
    }
}

fn optional_type(base: TokenStream, nullable: bool) -> TokenStream {
    if nullable {
        quote! { Option<#base> }
    } else {
        base
    }
}

fn find_entity<'ir>(ir: &'ir AppIr, id: &str) -> Result<&'ir EntityIr, CodegenError> {
    ir.entities
        .iter()
        .find(|entity| entity.id.0 == id)
        .ok_or_else(|| CodegenError::new(format!("missing entity `{id}`")))
}

fn module_name(entity: &EntityIr) -> String {
    to_snake_case(&entity.rust_name)
}

fn to_snake_case(value: &str) -> String {
    let mut output = String::new();
    for (index, character) in value.chars().enumerate() {
        if character.is_ascii_uppercase() {
            if index > 0 {
                output.push('_');
            }
            output.push(character.to_ascii_lowercase());
        } else {
            output.push(character);
        }
    }
    output
}

fn parse_ident(value: &str) -> Result<Ident, CodegenError> {
    syn::parse_str(value)
        .map_err(|error| CodegenError::new(format!("invalid Rust identifier `{value}`: {error}")))
}

fn render(tokens: TokenStream) -> Result<String, CodegenError> {
    let syntax = syn::parse2(tokens)
        .map_err(|error| CodegenError::new(format!("generated Rust did not parse: {error}")))?;
    format_rust(&format!(
        "{}{}",
        generated_header("//"),
        prettyplease::unparse(&syntax)
    ))
}

fn rust_template(source: &str) -> Result<String, CodegenError> {
    format_rust(&format!("{}{}", generated_header("//"), source))
}
