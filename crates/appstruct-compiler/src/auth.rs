use crate::surface::{SurfaceAuth, SurfaceEntity};
use appstruct_ir::{AuthIr, Diagnostic, EntityId, SourceSpan};
use std::collections::BTreeMap;

pub(crate) fn lower_auth(
    auth: &SurfaceAuth,
    entities: &[SurfaceEntity],
    fallback: &SourceSpan,
    diagnostics: &mut Vec<Diagnostic>,
) -> AuthIr {
    validate_role_declarations(auth, fallback, diagnostics);
    if !auth.enabled {
        if auth.user_entity.is_some()
            || auth.registration_enabled
            || auth.password_reset_enabled
            || auth.oauth_enabled
            || !auth.roles.is_empty()
            || auth.default_role.is_some()
        {
            diagnostics.push(Diagnostic::error(
                "AS3020",
                "auth and RBAC settings require `modules.auth.enabled: true`",
                fallback.clone(),
            ));
        }
        return disabled_auth();
    }

    let user_entity = auth.user_entity.as_ref().map(|value| {
        validate_user_entity(value, entities, diagnostics);
        EntityId(format!("app::{}", value.value))
    });
    if user_entity.is_none() {
        diagnostics.push(Diagnostic::error(
            "AS3021",
            "enabled auth requires `modules.auth.user_entity`",
            fallback.clone(),
        ));
    }
    if auth.roles.is_empty() {
        diagnostics.push(Diagnostic::error(
            "AS3022",
            "enabled auth requires at least one RBAC role",
            fallback.clone(),
        ));
    }
    let default_role = auth.default_role.as_ref().map(|role| role.value.clone());
    if default_role.is_none() {
        diagnostics.push(Diagnostic::error(
            "AS3023",
            "enabled auth requires `modules.rbac.default_role`",
            fallback.clone(),
        ));
    } else if !auth
        .roles
        .iter()
        .any(|role| Some(&role.value) == default_role.as_ref())
    {
        diagnostics.push(Diagnostic::error(
            "AS3024",
            "default RBAC role is not declared in `roles`",
            auth.default_role.as_ref().unwrap().span.clone(),
        ));
    }

    let mut roles = auth
        .roles
        .iter()
        .map(|role| role.value.clone())
        .collect::<Vec<_>>();
    roles.sort();
    AuthIr {
        enabled: true,
        user_entity,
        registration_enabled: auth.registration_enabled,
        password_reset_enabled: auth.password_reset_enabled,
        oauth_enabled: auth.oauth_enabled,
        roles,
        default_role,
    }
}

fn disabled_auth() -> AuthIr {
    AuthIr {
        enabled: false,
        user_entity: None,
        registration_enabled: false,
        password_reset_enabled: false,
        oauth_enabled: false,
        roles: Vec::new(),
        default_role: None,
    }
}

fn validate_role_declarations(
    auth: &SurfaceAuth,
    fallback: &SourceSpan,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let mut declarations = BTreeMap::new();
    for role in &auth.roles {
        if !valid_role(&role.value) {
            diagnostics.push(Diagnostic::error(
                "AS3025",
                format!("invalid RBAC role `{}`", role.value),
                role.span.clone(),
            ));
        }
        if let Some(first) = declarations.insert(role.value.clone(), role.span.clone()) {
            diagnostics.push(
                Diagnostic::error(
                    "AS3026",
                    format!("RBAC role `{}` is declared more than once", role.value),
                    role.span.clone(),
                )
                .with_secondary(first, "first declared here"),
            );
        }
    }
    if auth.enabled && auth.roles.is_empty() && auth.default_role.is_some() {
        diagnostics.push(Diagnostic::error(
            "AS3022",
            "RBAC roles cannot be empty",
            fallback.clone(),
        ));
    }
}

fn validate_user_entity(
    name: &crate::surface::Located<String>,
    entities: &[SurfaceEntity],
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Some(entity) = entities
        .iter()
        .find(|entity| entity.name.value == name.value)
    else {
        diagnostics.push(Diagnostic::error(
            "AS3027",
            format!("unknown auth user entity `{}`", name.value),
            name.span.clone(),
        ));
        return;
    };
    let valid_id = entity.fields.iter().any(|field| {
        field.name.value == "id" && field.type_name.value == "uuid" && field.flags.primary_key()
    });
    let valid_email = entity.fields.iter().any(|field| {
        field.name.value == "email"
            && field.type_name.value == "string"
            && field.flags.required()
            && field.flags.unique()
    });
    if !valid_id || !valid_email {
        diagnostics.push(
            Diagnostic::error(
                "AS3028",
                "auth user entity requires a UUID primary key `id` and required unique string `email`",
                entity.span.clone(),
            )
            .with_help("add compatible `id` and `email` fields to the configured user entity"),
        );
    }
    let unsupported = entity.fields.iter().find(|field| {
        !matches!(field.name.value.as_str(), "id" | "email")
            && field.flags.required()
            && field.generated.is_none()
            && field.default.is_none()
    });
    if let Some(field) = unsupported {
        diagnostics.push(Diagnostic::error(
            "AS3029",
            format!(
                "registration cannot populate required user field `{}`",
                field.name.value
            ),
            field.span.clone(),
        ));
    }
}

fn valid_role(value: &str) -> bool {
    let mut chars = value.chars();
    chars.next().is_some_and(|first| first.is_ascii_lowercase())
        && chars.all(|character| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || character == '_'
        })
}
