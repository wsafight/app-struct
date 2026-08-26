use super::module_name;
use appstruct_ir::AppIr;
use proc_macro2::TokenStream;
use quote::{format_ident, quote};

pub(super) fn source(ir: &AppIr) -> TokenStream {
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
