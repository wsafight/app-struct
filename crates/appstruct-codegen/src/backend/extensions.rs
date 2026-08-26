use super::{find_entity, module_name, parse_ident, render, rust_type};
use crate::CodegenError;
use appstruct_ir::{AppIr, EntityIr, OperationTypeIr, ValueObjectIr};
use proc_macro2::TokenStream;
use quote::{format_ident, quote};

pub(super) fn source(ir: &AppIr) -> Result<String, CodegenError> {
    let request_context = super::context::contract();
    let values = ir
        .value_objects
        .iter()
        .map(value_object)
        .collect::<Result<Vec<_>, _>>()?;
    let hooks = ir
        .entities
        .iter()
        .map(hook_contract)
        .collect::<Result<Vec<_>, _>>()?;
    let policies = ir
        .entities
        .iter()
        .map(policy_contract)
        .collect::<Result<Vec<_>, _>>()?;
    let handler_traits = handler_traits(ir)?;
    let registry = registry(ir);
    render(quote! {
        use crate::{ApiError, api, entities};
        use async_trait::async_trait;
        use sea_orm::{
            ConnectionTrait, DatabaseConnection, DatabaseTransaction, DbBackend, DbErr,
            ExecResult, QueryResult, Statement,
        };
        use std::sync::Arc;

        #[derive(Clone, Debug, serde::Serialize)]
        pub struct Actor {
            pub id: uuid::Uuid,
            pub email: String,
            pub roles: Vec<String>,
        }

        impl Actor {
            pub fn has_role(&self, role: &str) -> bool {
                self.roles.iter().any(|candidate| candidate == role)
            }
        }

        #request_context

        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub enum HookOperation { Create, Update, Delete }

        #(#values)*
        #(#hooks)*
        #(#policies)*
        #handler_traits
        #registry
    })
}

fn value_object(value: &ValueObjectIr) -> Result<TokenStream, CodegenError> {
    let name = parse_ident(&value.rust_name)?;
    let fields = value
        .fields
        .iter()
        .map(|field| {
            let name = parse_ident(&field.rust_name)?;
            let base = rust_type(&field.ty);
            let ty = if field.required {
                base
            } else {
                quote! { Option<#base> }
            };
            Ok(quote! { pub #name: #ty })
        })
        .collect::<Result<Vec<_>, CodegenError>>()?;
    Ok(quote! {
        #[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
        pub struct #name { #(#fields,)* }
    })
}

fn hook_contract(entity: &EntityIr) -> Result<TokenStream, CodegenError> {
    let trait_name = format_ident!("{}Hooks", entity.rust_name);
    let default_name = format_ident!("Default{}Hooks", entity.rust_name);
    let module = parse_ident(&module_name(entity))?;
    Ok(quote! {
        #[async_trait]
        pub trait #trait_name: Send + Sync {
            async fn before_validate_create(&self, _ctx: &RequestContext, _input: &mut api::#module::CreateInput) -> Result<(), ApiError> { Ok(()) }
            async fn before_validate_update(&self, _ctx: &RequestContext, _input: &mut api::#module::UpdateInput) -> Result<(), ApiError> { Ok(()) }
            async fn before_create(&self, _ctx: &RequestContext, _input: &mut api::#module::CreateInput) -> Result<(), ApiError> { Ok(()) }
            async fn after_create(&self, _ctx: &RequestContext, _model: &entities::#module::Model) -> Result<(), ApiError> { Ok(()) }
            async fn before_update(&self, _ctx: &RequestContext, _before: &entities::#module::Model, _input: &mut api::#module::UpdateInput) -> Result<(), ApiError> { Ok(()) }
            async fn after_update(&self, _ctx: &RequestContext, _before: &entities::#module::Model, _after: &entities::#module::Model) -> Result<(), ApiError> { Ok(()) }
            async fn before_delete(&self, _ctx: &RequestContext, _model: &entities::#module::Model) -> Result<(), ApiError> { Ok(()) }
            async fn after_delete(&self, _ctx: &RequestContext, _model: &entities::#module::Model) -> Result<(), ApiError> { Ok(()) }
            async fn after_commit(&self, _ctx: &RequestContext, _operation: HookOperation, _model: &entities::#module::Model) -> Result<(), ApiError> { Ok(()) }
        }

        struct #default_name;
        #[async_trait]
        impl #trait_name for #default_name {}
    })
}

fn policy_contract(entity: &EntityIr) -> Result<TokenStream, CodegenError> {
    let trait_name = format_ident!("{}Policy", entity.rust_name);
    let default_name = format_ident!("Default{}Policy", entity.rust_name);
    let module = parse_ident(&module_name(entity))?;
    Ok(quote! {
        #[async_trait]
        pub trait #trait_name: Send + Sync {
            async fn can_read(&self, _ctx: &RequestContext, _model: &entities::#module::Model) -> Result<bool, ApiError> { Ok(true) }
            async fn can_create(&self, _ctx: &RequestContext, _input: &api::#module::CreateInput) -> Result<bool, ApiError> { Ok(true) }
            async fn can_update(&self, _ctx: &RequestContext, _before: &entities::#module::Model, _input: &api::#module::UpdateInput, _after: &entities::#module::Model) -> Result<bool, ApiError> { Ok(true) }
            async fn can_delete(&self, _ctx: &RequestContext, _model: &entities::#module::Model) -> Result<bool, ApiError> { Ok(true) }
        }

        struct #default_name;
        #[async_trait]
        impl #trait_name for #default_name {}
    })
}

fn handler_traits(ir: &AppIr) -> Result<TokenStream, CodegenError> {
    let mut traits = Vec::new();
    let mut names = Vec::new();
    for command in &ir.commands {
        let name = format_ident!("{}Handler", command.rust_name);
        let input = operation_type(ir, &command.input)?;
        let output = operation_type(ir, &command.output)?;
        traits.push(quote! {
            #[async_trait]
            pub trait #name: Send + Sync {
                async fn execute(&self, ctx: &RequestContext, input: #input) -> Result<#output, ApiError>;
            }
        });
        names.push(name);
    }
    for query in &ir.queries {
        let name = format_ident!("{}Handler", query.rust_name);
        let output = operation_type(ir, &query.output)?;
        let method = if let Some(input) = &query.input {
            let input = operation_type(ir, input)?;
            quote! { async fn execute(&self, ctx: &RequestContext, input: #input) -> Result<#output, ApiError>; }
        } else {
            quote! { async fn execute(&self, ctx: &RequestContext) -> Result<#output, ApiError>; }
        };
        traits.push(quote! {
            #[async_trait]
            pub trait #name: Send + Sync { #method }
        });
        names.push(name);
    }
    Ok(quote! {
        #(#traits)*
        pub trait RequiredHandlers: Send + Sync #( + #names )* {}
        impl<T> RequiredHandlers for T where T: Send + Sync #( + #names )* {}
    })
}

fn registry(ir: &AppIr) -> TokenStream {
    let hook_fields = ir
        .entities
        .iter()
        .map(|entity| {
            let field = format_ident!("{}_hooks", module_name(entity));
            let ty = format_ident!("{}Hooks", entity.rust_name);
            quote! { #field: Arc<dyn #ty> }
        })
        .collect::<Vec<_>>();
    let policy_fields = ir
        .entities
        .iter()
        .map(|entity| {
            let field = format_ident!("{}_policy", module_name(entity));
            let ty = format_ident!("{}Policy", entity.rust_name);
            quote! { #field: Arc<dyn #ty> }
        })
        .collect::<Vec<_>>();
    let defaults = ir
        .entities
        .iter()
        .flat_map(|entity| {
            let hook_field = format_ident!("{}_hooks", module_name(entity));
            let policy_field = format_ident!("{}_policy", module_name(entity));
            let hook_default = format_ident!("Default{}Hooks", entity.rust_name);
            let policy_default = format_ident!("Default{}Policy", entity.rust_name);
            [
                quote! { #hook_field: Arc::new(#hook_default) },
                quote! { #policy_field: Arc::new(#policy_default) },
            ]
        })
        .collect::<Vec<_>>();
    let setters = optional_setters(ir);
    let getters = optional_getters(ir);
    let moves = optional_moves(ir);
    let JobRegistry {
        field: job_field,
        default: job_default,
        setter: job_setter,
        access: job_handler_access,
    } = job_registry(ir);
    let required = !ir.commands.is_empty() || !ir.queries.is_empty();
    let (initial_state, initial_value, default_handler) = initial_handlers(required);
    let register = required.then(|| {
        quote! {
            impl AppExtensionsBuilder<Missing> {
                pub fn handlers<H>(self, handlers: H) -> AppExtensionsBuilder<Present<H>>
                where H: RequiredHandlers + 'static {
                    AppExtensionsBuilder { handlers: Present(Arc::new(handlers)), #moves }
                }
            }
        }
    });
    quote! {
        pub struct Missing;
        pub struct Present<T>(Arc<T>);
        #default_handler

        #[derive(Clone)]
        pub struct AppExtensions {
            handlers: Arc<dyn RequiredHandlers>,
            #job_field
            #(#hook_fields,)*
            #(#policy_fields,)*
        }

        pub struct AppExtensionsBuilder<State> {
            handlers: State,
            #job_field
            #(#hook_fields,)*
            #(#policy_fields,)*
        }

        impl AppExtensions {
            pub fn builder() -> AppExtensionsBuilder<#initial_state> {
                AppExtensionsBuilder {
                    handlers: #initial_value, #job_default #(#defaults,)*
                }
            }
            pub fn handlers(&self) -> &dyn RequiredHandlers { self.handlers.as_ref() }
            #job_handler_access
            #getters
        }

        impl<State> AppExtensionsBuilder<State> { #job_setter #setters }
        #register

        impl<H> AppExtensionsBuilder<Present<H>>
        where H: RequiredHandlers + 'static {
            pub fn build(self) -> AppExtensions {
                AppExtensions { handlers: self.handlers.0, #moves }
            }
        }
    }
}

fn initial_handlers(required: bool) -> (TokenStream, TokenStream, TokenStream) {
    if required {
        (quote! { Missing }, quote! { Missing }, TokenStream::new())
    } else {
        (
            quote! { Present<DefaultHandlers> },
            quote! { Present(Arc::new(DefaultHandlers)) },
            quote! { pub struct DefaultHandlers; },
        )
    }
}

struct JobRegistry {
    field: TokenStream,
    default: TokenStream,
    setter: TokenStream,
    access: TokenStream,
}

fn job_registry(ir: &AppIr) -> JobRegistry {
    if !ir.jobs.enabled {
        return JobRegistry {
            field: TokenStream::new(),
            default: TokenStream::new(),
            setter: TokenStream::new(),
            access: TokenStream::new(),
        };
    }
    JobRegistry {
        field: quote! { job_handler: Option<Arc<dyn crate::JobHandler>>, },
        default: quote! { job_handler: None, },
        setter: quote! {
            pub fn job_handler<H: crate::JobHandler + 'static>(mut self, handler: H) -> Self {
                self.job_handler = Some(Arc::new(handler));
                self
            }
        },
        access: quote! {
            pub(crate) fn job_handler(&self) -> Option<Arc<dyn crate::JobHandler>> {
                self.job_handler.clone()
            }
        },
    }
}

fn optional_setters(ir: &AppIr) -> TokenStream {
    let methods = ir.entities.iter().flat_map(|entity| {
        let hook_method = format_ident!("{}_hooks", module_name(entity));
        let policy_method = format_ident!("{}_policy", module_name(entity));
        let hook_ty = format_ident!("{}Hooks", entity.rust_name);
        let policy_ty = format_ident!("{}Policy", entity.rust_name);
        [
            quote! { pub fn #hook_method<H: #hook_ty + 'static>(mut self, value: H) -> Self { self.#hook_method = Arc::new(value); self } },
            quote! { pub fn #policy_method<P: #policy_ty + 'static>(mut self, value: P) -> Self { self.#policy_method = Arc::new(value); self } },
        ]
    });
    quote! { #(#methods)* }
}

fn optional_getters(ir: &AppIr) -> TokenStream {
    let fields = ir.entities.iter().flat_map(|entity| {
        let hook = format_ident!("{}_hooks", module_name(entity));
        let policy = format_ident!("{}_policy", module_name(entity));
        let hook_ty = format_ident!("{}Hooks", entity.rust_name);
        let policy_ty = format_ident!("{}Policy", entity.rust_name);
        [
            quote! { pub(crate) fn #hook(&self) -> &dyn #hook_ty { self.#hook.as_ref() } },
            quote! { pub(crate) fn #policy(&self) -> &dyn #policy_ty { self.#policy.as_ref() } },
        ]
    });
    quote! { #(#fields)* }
}

fn optional_moves(ir: &AppIr) -> TokenStream {
    let fields = ir.entities.iter().flat_map(|entity| {
        let hook = format_ident!("{}_hooks", module_name(entity));
        let policy = format_ident!("{}_policy", module_name(entity));
        [
            quote! { #hook: self.#hook },
            quote! { #policy: self.#policy },
        ]
    });
    let job = ir
        .jobs
        .enabled
        .then(|| quote! { job_handler: self.job_handler, });
    quote! { #job #(#fields,)* }
}

pub(super) fn operation_type(
    ir: &AppIr,
    operation_type: &OperationTypeIr,
) -> Result<TokenStream, CodegenError> {
    match operation_type {
        OperationTypeIr::Entity { entity } => {
            let entity = find_entity(ir, &entity.0)?;
            let module = parse_ident(&module_name(entity))?;
            Ok(quote! { entities::#module::Model })
        }
        OperationTypeIr::ValueObject { value_object } => {
            let value = ir
                .value_objects
                .iter()
                .find(|value| value.id == *value_object)
                .ok_or_else(|| {
                    CodegenError::new(format!("missing value object `{value_object}`"))
                })?;
            let name = parse_ident(&value.rust_name)?;
            Ok(quote! { #name })
        }
    }
}
