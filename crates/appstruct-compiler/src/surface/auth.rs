use super::SurfaceAuth;
use super::value::{
    ensure_known_keys, expect_bool, expect_mapping, expect_sequence, expect_string, required,
};
use crate::yaml::{MappingEntry, Node};
use appstruct_ir::Diagnostic;
use std::collections::BTreeMap;

type DecodedProviders = (Vec<super::Located<String>>, Vec<super::Located<String>>);

pub(super) fn decode(modules_entry: Option<&MappingEntry>) -> Result<SurfaceAuth, Diagnostic> {
    let Some(modules_entry) = modules_entry else {
        return Ok(SurfaceAuth::default());
    };
    let modules = expect_mapping(&modules_entry.value, "`modules`")?;
    ensure_known_keys(
        modules,
        &[
            "auth", "billing", "rbac", "tenant", "audit", "mail", "jobs", "webhooks", "realtime",
            "file", "report", "activity",
        ],
        "`modules`",
    )?;
    let mut output = decode_auth(modules.get("auth"))?;
    decode_rbac(modules.get("rbac"), &mut output)?;
    Ok(output)
}

fn decode_auth(entry: Option<&MappingEntry>) -> Result<SurfaceAuth, Diagnostic> {
    let Some(entry) = entry else {
        return Ok(SurfaceAuth::default());
    };
    let auth = expect_mapping(&entry.value, "`modules.auth`")?;
    ensure_known_keys(
        auth,
        &[
            "enabled",
            "user_entity",
            "registration",
            "password_reset",
            "oauth",
            "providers",
        ],
        "`modules.auth`",
    )?;
    let (oauth_providers, oauth_signup_disabled) = auth
        .get("providers")
        .map(|value| decode_providers(&value.value))
        .transpose()?
        .unwrap_or_default();
    Ok(SurfaceAuth {
        enabled: optional_bool(auth.get("enabled"), "`modules.auth.enabled`")?,
        user_entity: auth
            .get("user_entity")
            .map(|value| expect_string(&value.value, "`modules.auth.user_entity`"))
            .transpose()?,
        registration_enabled: optional_bool(
            auth.get("registration"),
            "`modules.auth.registration`",
        )?,
        password_reset_enabled: optional_bool(
            auth.get("password_reset"),
            "`modules.auth.password_reset`",
        )?,
        oauth_enabled: optional_bool(auth.get("oauth"), "`modules.auth.oauth`")?,
        oauth_providers,
        oauth_signup_disabled,
        ..SurfaceAuth::default()
    })
}

fn decode_providers(node: &Node) -> Result<DecodedProviders, Diagnostic> {
    let mut declarations = BTreeMap::new();
    let mut providers = Vec::new();
    let mut signup_disabled = Vec::new();
    for provider in expect_sequence(node, "`modules.auth.providers`")? {
        let (id, enabled, allow_signup) = if provider.scalar().is_some() {
            (expect_string(provider, "OAuth provider")?, true, true)
        } else {
            let mapping = expect_mapping(provider, "OAuth provider")?;
            ensure_known_keys(
                mapping,
                &["id", "type", "enabled", "allow_signup", "capabilities"],
                "OAuth provider",
            )?;
            let id = required(mapping, "id", &provider.span)
                .and_then(|entry| expect_string(&entry.value, "OAuth provider `id`"))?;
            let provider_type = required(mapping, "type", &provider.span)
                .and_then(|entry| expect_string(&entry.value, "OAuth provider `type`"))?;
            validate_provider_type(&id, &provider_type)?;
            let enabled = mapping
                .get("enabled")
                .map(|entry| expect_bool(&entry.value, "OAuth provider `enabled`"))
                .transpose()?
                .unwrap_or(true);
            let allow_signup = mapping
                .get("allow_signup")
                .map(|entry| expect_bool(&entry.value, "OAuth provider `allow_signup`"))
                .transpose()?
                .unwrap_or(true);
            if let Some(capabilities) = mapping.get("capabilities") {
                let capabilities =
                    expect_mapping(&capabilities.value, "OAuth provider `capabilities`")?;
                ensure_known_keys(
                    capabilities,
                    &["login", "signup", "account_linking"],
                    "OAuth provider `capabilities`",
                )?;
                for capability in ["login", "signup", "account_linking"] {
                    if let Some(entry) = capabilities.get(capability) {
                        expect_bool(
                            &entry.value,
                            &format!("OAuth provider capability `{capability}`"),
                        )?;
                    }
                }
            }
            (id, enabled, allow_signup)
        };
        if let Some(first) = declarations.insert(id.value.clone(), id.span.clone()) {
            return Err(Diagnostic::error(
                "AS3038",
                format!("OAuth provider `{}` is declared more than once", id.value),
                id.span,
            )
            .with_secondary(first, "first declared here"));
        }
        if enabled {
            if !allow_signup {
                signup_disabled.push(id.clone());
            }
            providers.push(id);
        }
    }
    Ok((providers, signup_disabled))
}

fn validate_provider_type(
    id: &super::Located<String>,
    provider_type: &super::Located<String>,
) -> Result<(), Diagnostic> {
    let expected = match id.value.as_str() {
        "oidc" | "google" => "oidc",
        "github" => "oauth",
        provider => {
            return Err(Diagnostic::error(
                "AS3027",
                format!("unsupported OAuth provider `{provider}`; use oidc, google, or github"),
                id.span.clone(),
            ));
        }
    };
    if provider_type.value != expected {
        return Err(Diagnostic::error(
            "AS3039",
            format!(
                "OAuth provider `{}` requires type `{expected}`, not `{}`",
                id.value, provider_type.value
            ),
            provider_type.span.clone(),
        ));
    }
    Ok(())
}

fn decode_rbac(entry: Option<&MappingEntry>, output: &mut SurfaceAuth) -> Result<(), Diagnostic> {
    let Some(entry) = entry else { return Ok(()) };
    let rbac = expect_mapping(&entry.value, "`modules.rbac`")?;
    ensure_known_keys(rbac, &["roles", "default_role"], "`modules.rbac`")?;
    output.roles = rbac
        .get("roles")
        .map(|value| {
            expect_sequence(&value.value, "`modules.rbac.roles`")?
                .iter()
                .map(|role| expect_string(role, "RBAC role"))
                .collect()
        })
        .transpose()?
        .unwrap_or_default();
    output.default_role = rbac
        .get("default_role")
        .map(|value| expect_string(&value.value, "`modules.rbac.default_role`"))
        .transpose()?;
    Ok(())
}

fn optional_bool(entry: Option<&MappingEntry>, context: &str) -> Result<bool, Diagnostic> {
    entry
        .map(|value| expect_bool(&value.value, context))
        .transpose()
        .map(Option::unwrap_or_default)
}
