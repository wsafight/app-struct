use super::parse_ident;
use crate::CodegenError;
use appstruct_ir::{AccessRuleIr, EntityIr, FieldIr};
use proc_macro2::TokenStream;
use quote::quote;

pub(super) fn scope(
    entity: &EntityIr,
    module: &syn::Ident,
    rule: &AccessRuleIr,
) -> Result<TokenStream, CodegenError> {
    let condition = condition(entity, module, rule)?;
    let tenant_scope = tenant_scope(entity, module);
    Ok(quote! {
        let access_scope = #condition;
        let access_condition = match access_scope {
            Some(condition) => condition,
            None => return Err(access_denied(&context)),
        };
        select = select.filter(access_condition);
        #tenant_scope
    })
}

pub(super) fn member_scope(
    entity: &EntityIr,
    module: &syn::Ident,
    rule: &AccessRuleIr,
) -> Result<TokenStream, CodegenError> {
    let condition = condition(entity, module, rule)?;
    let tenant_condition = tenant_condition(entity, module);
    Ok(quote! {
        let access_condition = #condition
            .ok_or_else(|| access_denied(&context))?
            #tenant_condition;
    })
}

fn tenant_scope(entity: &EntityIr, module: &syn::Ident) -> TokenStream {
    if entity.tenant_scoped {
        quote! {
            let tenant_id = context.require_tenant()?;
            select = select.filter(#module::Column::TenantId.eq(tenant_id));
        }
    } else {
        TokenStream::new()
    }
}

fn tenant_condition(entity: &EntityIr, module: &syn::Ident) -> TokenStream {
    if entity.tenant_scoped {
        quote! { .add(#module::Column::TenantId.eq(context.require_tenant()?)) }
    } else {
        TokenStream::new()
    }
}

pub(super) fn create_allowed(
    entity: &EntityIr,
    rule: &AccessRuleIr,
) -> Result<TokenStream, CodegenError> {
    allowed(entity, rule, &quote! { input }, None)
}

pub(super) fn row_allowed(
    entity: &EntityIr,
    rule: &AccessRuleIr,
) -> Result<TokenStream, CodegenError> {
    allowed(entity, rule, &quote! { model }, None)
}

pub(super) fn update_allowed(
    entity: &EntityIr,
    rule: &AccessRuleIr,
) -> Result<TokenStream, CodegenError> {
    allowed(
        entity,
        rule,
        &quote! { before },
        Some(&quote! { candidate }),
    )
}

pub(super) fn operation_allowed(rule: &AccessRuleIr) -> TokenStream {
    match rule {
        AccessRuleIr::Public => quote! { true },
        AccessRuleIr::Authenticated => quote! { context.actor().is_some() },
        AccessRuleIr::Role { role } => {
            quote! { context.actor().is_some_and(|actor| actor.has_role(#role)) }
        }
        AccessRuleIr::Any { rules } => {
            let children = rules.iter().map(operation_allowed);
            quote! { false #(|| #children)* }
        }
        AccessRuleIr::All { rules } => {
            let children = rules.iter().map(operation_allowed);
            quote! { true #(&& #children)* }
        }
        AccessRuleIr::Owner { .. } => unreachable!("compiler rejects owner operation rules"),
    }
}

fn condition(
    entity: &EntityIr,
    module: &syn::Ident,
    rule: &AccessRuleIr,
) -> Result<TokenStream, CodegenError> {
    Ok(match rule {
        AccessRuleIr::Public => quote! { Some(Condition::all()) },
        AccessRuleIr::Authenticated => {
            quote! { context.actor().map(|_| Condition::all()) }
        }
        AccessRuleIr::Role { role } => quote! {
            context.actor().filter(|actor| actor.has_role(#role)).map(|_| Condition::all())
        },
        AccessRuleIr::Owner { field } => {
            let field = owner_field(entity, &field.0)?;
            let column = column_ident(field)?;
            quote! {
                context.actor().map(|actor| Condition::all().add(#module::Column::#column.eq(actor.id)))
            }
        }
        AccessRuleIr::Any { rules } => {
            let children = rules
                .iter()
                .map(|rule| condition(entity, module, rule))
                .collect::<Result<Vec<_>, _>>()?;
            quote! {{
                let children = vec![#(#children),*].into_iter().flatten().collect::<Vec<_>>();
                if children.is_empty() {
                    None
                } else {
                    Some(children.into_iter().fold(Condition::any(), |scope, child| scope.add(child)))
                }
            }}
        }
        AccessRuleIr::All { rules } => {
            let children = rules
                .iter()
                .map(|rule| condition(entity, module, rule))
                .collect::<Result<Vec<_>, _>>()?;
            quote! {{
                let children = vec![#(#children),*];
                if children.iter().any(Option::is_none) {
                    None
                } else {
                    Some(children.into_iter().flatten().fold(Condition::all(), |scope, child| scope.add(child)))
                }
            }}
        }
    })
}

fn allowed(
    entity: &EntityIr,
    rule: &AccessRuleIr,
    before: &TokenStream,
    after: Option<&TokenStream>,
) -> Result<TokenStream, CodegenError> {
    Ok(match rule {
        AccessRuleIr::Public => quote! { true },
        AccessRuleIr::Authenticated => quote! { context.actor().is_some() },
        AccessRuleIr::Role { role } => {
            quote! { context.actor().is_some_and(|actor| actor.has_role(#role)) }
        }
        AccessRuleIr::Owner { field } => {
            let field = owner_field(entity, &field.0)?;
            let name = parse_ident(&field.rust_name)?;
            let before_match = owner_match(field, before, &name);
            if let Some(after) = after {
                let after_match = owner_match(field, after, &name);
                quote! { context.actor().is_some_and(|actor| #before_match && #after_match) }
            } else {
                quote! { context.actor().is_some_and(|actor| #before_match) }
            }
        }
        AccessRuleIr::Any { rules } => {
            let children = rules
                .iter()
                .map(|rule| allowed(entity, rule, before, after))
                .collect::<Result<Vec<_>, _>>()?;
            quote! { false #(|| #children)* }
        }
        AccessRuleIr::All { rules } => {
            let children = rules
                .iter()
                .map(|rule| allowed(entity, rule, before, after))
                .collect::<Result<Vec<_>, _>>()?;
            quote! { true #(&& #children)* }
        }
    })
}

fn owner_match(field: &FieldIr, value: &TokenStream, name: &syn::Ident) -> TokenStream {
    if field.nullable {
        quote! { #value.#name == Some(actor.id) }
    } else {
        quote! { #value.#name == actor.id }
    }
}

fn owner_field<'entity>(
    entity: &'entity EntityIr,
    id: &str,
) -> Result<&'entity FieldIr, CodegenError> {
    entity
        .fields
        .iter()
        .find(|field| field.id.0 == id)
        .ok_or_else(|| CodegenError::new(format!("missing owner field `{id}`")))
}

fn column_ident(field: &FieldIr) -> Result<syn::Ident, CodegenError> {
    let name = field
        .rust_name
        .split('_')
        .map(|part| {
            let mut chars = part.chars();
            chars.next().map_or_else(String::new, |first| {
                first.to_uppercase().chain(chars).collect::<String>()
            })
        })
        .collect::<String>();
    parse_ident(&name)
}
