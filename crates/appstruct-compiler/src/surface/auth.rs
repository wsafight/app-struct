use super::SurfaceAuth;
use super::value::{
    ensure_known_keys, expect_bool, expect_mapping, expect_sequence, expect_string,
};
use crate::yaml::MappingEntry;
use appstruct_ir::Diagnostic;

pub(super) fn decode(modules_entry: Option<&MappingEntry>) -> Result<SurfaceAuth, Diagnostic> {
    let Some(modules_entry) = modules_entry else {
        return Ok(SurfaceAuth::default());
    };
    let modules = expect_mapping(&modules_entry.value, "`modules`")?;
    ensure_known_keys(
        modules,
        &[
            "auth", "rbac", "tenant", "audit", "mail", "jobs", "webhooks", "realtime", "file",
            "report", "activity",
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
        ],
        "`modules.auth`",
    )?;
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
        ..SurfaceAuth::default()
    })
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
