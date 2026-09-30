use super::{PullAccess, render::AccessMode};

pub(super) fn resolve_access(
    access: PullAccess,
    role: Option<&str>,
    owner: Option<&str>,
) -> Result<AccessMode, String> {
    match access {
        PullAccess::None if role.is_none() && owner.is_none() => Ok(AccessMode::None),
        PullAccess::Public if role.is_none() && owner.is_none() => Ok(AccessMode::Public),
        PullAccess::Authenticated if role.is_none() && owner.is_none() => {
            Ok(AccessMode::Authenticated)
        }
        PullAccess::Role => role
            .filter(|value| valid_access_name(value))
            .map(|value| AccessMode::Role(value.to_owned()))
            .ok_or_else(|| "--access role requires a valid --role <name>".to_owned()),
        PullAccess::Owner => owner
            .filter(|value| valid_access_name(value))
            .map(|value| AccessMode::Owner(value.to_owned()))
            .ok_or_else(|| "--access owner requires a valid --owner <field>".to_owned()),
        _ => Err(
            "--role is valid only with --access role and --owner only with --access owner"
                .to_owned(),
        ),
    }
}

pub(super) fn valid_access_name(value: &str) -> bool {
    let mut characters = value.chars();
    characters
        .next()
        .is_some_and(|first| first.is_ascii_lowercase())
        && characters.all(|character| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || character == '_'
        })
}
