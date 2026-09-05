use super::super::{access, module_name, parse_ident};
use crate::CodegenError;
use appstruct_ir::{AppIr, EntityIr};
use proc_macro2::{Ident, Literal, TokenStream};
use quote::{format_ident, quote};

pub(super) struct WorkflowContext<'context> {
    pub module: &'context Ident,
    pub hooks: &'context Ident,
    pub policy: &'context Ident,
    pub parse_id: &'context TokenStream,
    pub primary: &'context Ident,
}

pub(super) struct WorkflowSupport {
    pub routes: TokenStream,
    pub tokens: TokenStream,
}

#[allow(clippy::too_many_lines)]
pub(super) fn support(
    ir: &AppIr,
    entity: &EntityIr,
    context: &WorkflowContext<'_>,
) -> Result<WorkflowSupport, CodegenError> {
    let Some(workflow) = &entity.workflow else {
        return Ok(WorkflowSupport {
            routes: TokenStream::new(),
            tokens: TokenStream::new(),
        });
    };
    let field = entity
        .workflow_field()
        .expect("IR validation guarantees the workflow field exists");
    let field_name = parse_ident(&field.rust_name)?;
    let WorkflowContext {
        module,
        hooks,
        policy,
        parse_id,
        primary,
    } = context;
    let read_scope = access::member_scope(entity, module, &entity.access.read)?;
    let input_variants = workflow
        .transitions
        .iter()
        .map(|transition| {
            let variant = transition_variant(&transition.name);
            transition.input.as_ref().map_or_else(
                || Ok(quote! { #variant }),
                |input| {
                    let value = ir
                        .value_objects
                        .iter()
                        .find(|value| value.id == *input)
                        .expect("IR validation guarantees workflow input exists");
                    let value = parse_ident(&value.rust_name)?;
                    Ok(quote! { #variant(crate::extensions::#value) })
                },
            )
        })
        .collect::<Result<Vec<_>, CodegenError>>()?;
    let parse_input_arms = workflow
        .transitions
        .iter()
        .map(|transition| {
            let name = transition.name.as_str();
            let variant = transition_variant(name);
            transition.input.as_ref().map_or_else(
                || {
                    Ok(quote! {
                        #name => {
                            if !value.is_null()
                                && !value.as_object().is_some_and(serde_json::Map::is_empty)
                            {
                                return Err(ApiError::InvalidWorkflowInput(
                                    "this transition does not accept input".to_owned(),
                                ));
                            }
                            Ok(WorkflowTransitionInput::#variant)
                        }
                    })
                },
                |input| {
                    let value_object = ir
                        .value_objects
                        .iter()
                        .find(|value| value.id == *input)
                        .expect("IR validation guarantees workflow input exists");
                    let value_type = parse_ident(&value_object.rust_name)?;
                    Ok(quote! {
                        #name => serde_json::from_value::<crate::extensions::#value_type>(value)
                            .map(WorkflowTransitionInput::#variant)
                            .map_err(|error| ApiError::InvalidWorkflowInput(error.to_string())),
                    })
                },
            )
        })
        .collect::<Result<Vec<_>, CodegenError>>()?;
    let target_arms = workflow.transitions.iter().map(|transition| {
        let name = transition.name.as_str();
        let target = transition.to.as_str();
        let sources = transition.from.iter().map(String::as_str);
        quote! {
            #name if [#(#sources),*].contains(&before.#field_name.as_str()) => (#name, #target),
            #name => return Err(ApiError::InvalidWorkflowState),
        }
    });
    let allowed_arms = workflow
        .transitions
        .iter()
        .map(|transition| {
            let name = transition.name.as_str();
            let allowed = access::transition_allowed(entity, &transition.access)?;
            Ok(quote! { #name => #allowed })
        })
        .collect::<Result<Vec<_>, CodegenError>>()?;
    let capability_checks = workflow
        .transitions
        .iter()
        .map(|transition| {
            let name = transition.name.as_str();
            let target = transition.to.as_str();
            let input = transition.input.as_ref().map_or_else(
                || quote! { None },
                |input| {
                    let name = ir
                        .value_objects
                        .iter()
                        .find(|value| value.id == *input)
                        .expect("IR validation guarantees workflow input exists")
                        .rust_name
                        .as_str();
                    quote! { Some(#name.to_owned()) }
                },
            );
            let sources = transition.from.iter().map(String::as_str);
            let allowed = access::transition_allowed(entity, &transition.access)?;
            Ok(quote! {
                if [#(#sources),*].contains(&before.#field_name.as_str()) {
                    let candidate = workflow_candidate(before, #target)?;
                    if (#allowed)
                        && state.extensions.#policy()
                            .can_view_transition(&context, #name, before, &candidate).await?
                    {
                        allowed_transitions.push(WorkflowTransitionCapability {
                            name: #name.to_owned(),
                            to: #target.to_owned(),
                            input: #input,
                        });
                    }
                }
            })
        })
        .collect::<Result<Vec<_>, CodegenError>>()?;
    let entity_id = entity.id.0.as_str();
    let event_prefix = Literal::string(&module_name(entity));
    let audit = entity.audit_enabled.then(|| {
        quote! {
            let input_bytes = serde_json::to_vec(&input).map_err(|_| ApiError::Internal)?;
            let input_digest = <sha2::Sha256 as sha2::Digest>::digest(&input_bytes);
            let audit_metadata = serde_json::json!({
                "transition": transition,
                "input_sha256": format!("sha256:{input_digest:x}"),
                "from_revision": before.revision,
                "to_revision": after.revision,
            });
            crate::audit::record_with_metadata(
                &transaction, &context, #entity_id, after.#primary.to_string(),
                &format!("workflow.{transition}"), Some(&before), Some(&after),
                &audit_metadata,
            ).await?;
        }
    });
    let activity = ir.activity.resource_for_entity(&entity.id).map(|resource| {
        let resource = resource.resource.as_str();
        quote! {
            crate::activity::record_system_event(
                &transaction, &context, #resource, after.#primary.to_string(),
                &format!("workflow.{transition}"),
            ).await?;
        }
    });
    let webhook = ir.webhooks.enabled.then(|| {
        quote! {
            let event = format!(concat!(#event_prefix, ".workflow.{}"), transition);
            let idempotency_key = format!(
                "workflow:{}:{}:{}", #entity_id, after.#primary, after.revision,
            );
            let payload = serde_json::json!({
                "transition": transition,
                "input": &input,
                "before": &before,
                "after": &after,
            });
            crate::webhooks::publish(
                &transaction, &event, &payload, Some(&idempotency_key), context.tenant(),
            ).await.map_err(|error| {
                tracing::error!(?error, transition, "workflow event could not be persisted");
                ApiError::Internal
            })?;
        }
    });
    let tokens = quote! {
        #[derive(Clone, Debug, Serialize)]
        pub enum WorkflowTransitionInput { #(#input_variants,)* }

        #[derive(Clone, Debug, Serialize)]
        pub struct WorkflowTransitionCapability {
            pub name: String,
            pub to: String,
            pub input: Option<String>,
        }

        #[derive(Clone, Debug, Serialize)]
        pub struct WorkflowCapabilities {
            pub state: String,
            pub revision: i64,
            pub allowed_transitions: Vec<WorkflowTransitionCapability>,
        }

        async fn workflow_capabilities(
            State(state): State<AppState>,
            Path(id): Path<String>,
            headers: HeaderMap,
        ) -> Result<([(header::HeaderName, String); 1], Json<WorkflowCapabilities>), ApiError> {
            let context = state.context(&headers).await?;
            let id = #parse_id;
            #read_scope
            let model = #module::Entity::find_by_id(id)
                .filter(access_condition)
                .one(&state.database).await?.ok_or(ApiError::NotFound)?;
            if !state.extensions.#policy().can_read(&context, &model).await? {
                return Err(ApiError::NotFound);
            }
            let before = &model;
            let mut allowed_transitions = Vec::new();
            #(#capability_checks)*
            Ok((
                etag_header(&model),
                Json(WorkflowCapabilities {
                    state: model.#field_name.clone(),
                    revision: model.revision,
                    allowed_transitions,
                }),
            ))
        }

        async fn execute_workflow_transition(
            State(state): State<AppState>,
            Path((id, action)): Path<(String, String)>,
            headers: HeaderMap,
            input: Result<Json<serde_json::Value>, axum::extract::rejection::JsonRejection>,
        ) -> Result<([(header::HeaderName, String); 1], Json<serde_json::Value>), ApiError> {
            let expected = expected_revision(&headers)?;
            let context = state.mutation_context(&headers).await?;
            let actor = context.actor().cloned();
            let tenant = context.tenant();
            let raw_input = input
                .map_err(|error| ApiError::InvalidWorkflowInput(error.body_text()))?.0;
            let id = #parse_id;
            let transaction = state.database.begin().await?;
            let after = {
                let context = RequestContext::transaction_with_file(
                    &transaction, &state.mail, &state.file, &state.realtime, actor.clone(), tenant,
                );
                #read_scope
                let before = #module::Entity::find_by_id(id)
                    .filter(access_condition)
                    .lock_exclusive()
                    .one(&transaction).await?.ok_or(ApiError::NotFound)?;
                if !state.extensions.#policy().can_read(&context, &before).await? {
                    return Err(ApiError::NotFound);
                }
                if before.revision != expected {
                    return Err(ApiError::ConcurrentModification);
                }
                let (transition, target) = match action.as_str() {
                    #(#target_arms)*
                    _ => return Err(ApiError::UnknownWorkflowTransition),
                };
                let candidate = workflow_candidate(&before, target)?;
                let transition_allowed = match transition {
                    #(#allowed_arms,)*
                    _ => false,
                };
                if !transition_allowed {
                    return Err(access_denied(&context));
                }
                let mut input = parse_workflow_transition_input(transition, raw_input)?;
                state.extensions.#hooks()
                    .before_transition(&context, transition, &before, &mut input).await?;
                if !state.extensions.#policy()
                    .can_transition(&context, transition, &before, &input, &candidate).await?
                {
                    return Err(ApiError::Forbidden);
                }
                let mut active = before.clone().into_active_model();
                active.#field_name = Set(target.to_owned());
                active.revision = Set(before.revision.checked_add(1)
                    .ok_or_else(|| sea_orm::DbErr::Custom("revision overflow".to_owned()))?);
                let after = active.update(&transaction).await?;
                state.extensions.#hooks()
                    .after_transition(&context, transition, &before, &input, &after).await?;
                #audit
                #activity
                #webhook
                after
            };
            transaction.commit().await?;
            let event = format!(concat!(#event_prefix, ".workflow.{}"), action);
            publish_realtime_event(&state, &context, &event, &after);
            run_after_commit(
                &state, crate::HookOperation::Transition, &after, actor, tenant,
            ).await;
            Ok((etag_header(&after), Json(redact_model(&context, after)?)))
        }

        fn parse_workflow_transition_input(
            transition: &str,
            value: serde_json::Value,
        ) -> Result<WorkflowTransitionInput, ApiError> {
            match transition {
                #(#parse_input_arms)*
                _ => Err(ApiError::UnknownWorkflowTransition),
            }
        }

        fn workflow_candidate(
            before: &#module::Model,
            target: &str,
        ) -> Result<#module::Model, ApiError> {
            let mut active = before.clone().into_active_model();
            active.#field_name = Set(target.to_owned());
            active.revision = Set(before.revision.checked_add(1)
                .ok_or_else(|| sea_orm::DbErr::Custom("revision overflow".to_owned()))?);
            Ok(active.try_into_model()?)
        }
    };
    Ok(WorkflowSupport {
        routes: quote! {
            .route("/{id}/_transitions", get(workflow_capabilities))
            .route("/{id}/_transitions/{action}", axum::routing::post(execute_workflow_transition))
        },
        tokens,
    })
}

fn transition_variant(name: &str) -> Ident {
    let pascal = name
        .split('_')
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut characters = part.chars();
            characters.next().map_or_else(String::new, |first| {
                first.to_uppercase().chain(characters).collect()
            })
        })
        .collect::<String>();
    format_ident!("{pascal}")
}
