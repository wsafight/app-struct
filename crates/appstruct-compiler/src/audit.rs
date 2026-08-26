use crate::surface::{SurfaceAudit, SurfaceAuth, SurfaceEntity};
use appstruct_ir::{AuditIr, Diagnostic, SourceSpan};

pub(crate) fn lower_audit(
    audit: &SurfaceAudit,
    auth: &SurfaceAuth,
    entities: &[SurfaceEntity],
    fallback: &SourceSpan,
    diagnostics: &mut Vec<Diagnostic>,
) -> AuditIr {
    let span = audit.span.as_ref().unwrap_or(fallback);
    if audit.enabled && !auth.enabled {
        diagnostics.push(Diagnostic::error(
            "AS3037",
            "enabled audit module requires `modules.auth.enabled: true`",
            span.clone(),
        ));
    }
    if audit.enabled && audit.reader_roles.is_empty() {
        diagnostics.push(Diagnostic::error(
            "AS3038",
            "enabled audit module requires at least one `reader_roles` entry",
            span.clone(),
        ));
    }
    for role in &audit.reader_roles {
        if !auth
            .roles
            .iter()
            .any(|declared| declared.value == role.value)
        {
            diagnostics.push(Diagnostic::error(
                "AS3039",
                format!("audit reader role `{}` is not declared by RBAC", role.value),
                role.span.clone(),
            ));
        }
    }
    if !audit.enabled {
        for entity in entities.iter().filter(|entity| entity.audit_enabled) {
            diagnostics.push(Diagnostic::error(
                "AS3040",
                "audited entity requires `modules.audit.enabled: true`",
                entity.span.clone(),
            ));
        }
    }
    let mut reader_roles = audit
        .reader_roles
        .iter()
        .map(|role| role.value.clone())
        .collect::<Vec<_>>();
    reader_roles.sort();
    reader_roles.dedup();
    AuditIr {
        enabled: audit.enabled,
        reader_roles,
    }
}
