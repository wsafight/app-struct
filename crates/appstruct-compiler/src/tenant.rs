use crate::surface::{SurfaceEntity, SurfaceRoot};
use appstruct_ir::{AuthIr, Diagnostic, TenantIr};
use std::collections::BTreeMap;

pub(crate) fn lower_tenant(
    root: &SurfaceRoot,
    entities: &[SurfaceEntity],
    auth: &AuthIr,
    diagnostics: &mut Vec<Diagnostic>,
) -> TenantIr {
    let span = root
        .tenant
        .span
        .as_ref()
        .unwrap_or(&root.app_name.span)
        .clone();
    if root.tenant.enabled && !auth.enabled {
        diagnostics.push(Diagnostic::error(
            "AS3034",
            "enabled tenant module requires `modules.auth.enabled: true`",
            span,
        ));
    }
    if !root.tenant.enabled {
        for entity in entities.iter().filter(|entity| entity.tenant_scoped) {
            diagnostics.push(Diagnostic::error(
                "AS3035",
                "tenant-scoped entity requires `modules.tenant.enabled: true`",
                entity.span.clone(),
            ));
        }
    }
    validate_relation_directions(entities, diagnostics);
    TenantIr {
        enabled: root.tenant.enabled,
    }
}

fn validate_relation_directions(entities: &[SurfaceEntity], diagnostics: &mut Vec<Diagnostic>) {
    let tenant_scope = entities
        .iter()
        .map(|entity| (entity.name.value.as_str(), entity.tenant_scoped))
        .collect::<BTreeMap<_, _>>();
    for source in entities.iter().filter(|entity| !entity.tenant_scoped) {
        for field in &source.fields {
            if field.type_name.value != "relation" {
                continue;
            }
            let Some(target) = &field.target else {
                continue;
            };
            let target_name = target.value.strip_prefix("app::").unwrap_or(&target.value);
            if tenant_scope.get(target_name) == Some(&true) {
                diagnostics.push(
                    Diagnostic::error(
                        "AS3037",
                        "a global entity cannot reference a tenant-scoped entity",
                        target.span.clone(),
                    )
                    .with_help(
                        "make the source tenant-scoped or move the relation to the tenant entity",
                    ),
                );
            }
        }
    }
}
