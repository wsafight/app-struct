use crate::surface::{SurfaceAccessRule, SurfaceDomain, SurfaceEntity};
use appstruct_ir::Diagnostic;

pub(crate) fn warnings(domain: &SurfaceDomain) -> Vec<Diagnostic> {
    domain
        .entities
        .iter()
        .filter_map(public_mutation_warning)
        .collect()
}

fn public_mutation_warning(entity: &SurfaceEntity) -> Option<Diagnostic> {
    let access = entity.access.as_ref()?;
    let operations = [
        ("create", access.create.as_ref()),
        ("update", access.update.as_ref()),
        ("delete", access.delete.as_ref()),
    ]
    .into_iter()
    .filter_map(|(name, rule)| {
        rule.is_some_and(|rule| allows_anonymous(&rule.value))
            .then_some(name)
    })
    .collect::<Vec<_>>();
    if operations.is_empty() {
        return None;
    }
    Some(
        Diagnostic::warning(
            "AS3070",
            format!(
                "entity `{}` allows anonymous {} operations",
                entity.name.value,
                operations.join(", ")
            ),
            access.span.clone(),
        )
        .with_help("require authentication or an explicit role for production write operations"),
    )
}

fn allows_anonymous(rule: &SurfaceAccessRule) -> bool {
    match rule {
        SurfaceAccessRule::Public => true,
        SurfaceAccessRule::Any(rules) => rules.iter().any(|rule| allows_anonymous(&rule.value)),
        SurfaceAccessRule::All(rules) => rules.iter().all(|rule| allows_anonymous(&rule.value)),
        SurfaceAccessRule::Authenticated
        | SurfaceAccessRule::Role(_)
        | SurfaceAccessRule::Owner(_) => false,
    }
}
