use crate::surface::SurfaceActivity;
use appstruct_ir::{
    ActivityIr, ActivityResourceIr, AuditIr, AuthIr, Diagnostic, EntityIr, FileIr, SourceSpan,
};
use std::collections::{BTreeMap, BTreeSet};

const DEFAULT_MAX_COMMENT_BYTES: u32 = 4_000;

#[allow(clippy::too_many_lines)]
pub(crate) fn lower_activity(
    activity: &SurfaceActivity,
    auth: &AuthIr,
    audit: &AuditIr,
    file: &FileIr,
    entities: &[EntityIr],
    fallback: &SourceSpan,
    diagnostics: &mut Vec<Diagnostic>,
) -> ActivityIr {
    if !activity.enabled {
        return ActivityIr::default();
    }
    let span = activity.span.as_ref().unwrap_or(fallback);
    if !auth.enabled {
        diagnostics.push(Diagnostic::error(
            "AS3100",
            "enabled activity module requires auth",
            span.clone(),
        ));
    }
    if !audit.enabled {
        diagnostics.push(Diagnostic::error(
            "AS3100",
            "enabled activity module requires audit",
            span.clone(),
        ));
    }
    if activity.attachments && !file.enabled {
        diagnostics.push(Diagnostic::error(
            "AS3101",
            "activity attachments require file storage",
            span.clone(),
        ));
    }
    if activity.resources.is_empty() {
        diagnostics.push(Diagnostic::error(
            "AS3102",
            "enabled activity module requires at least one resource",
            span.clone(),
        ));
    }

    let entities_by_name = entities
        .iter()
        .map(|entity| (entity.rust_name.as_str(), entity))
        .collect::<BTreeMap<_, _>>();
    let mut seen_entities = BTreeSet::new();
    let mut seen_resources = BTreeSet::new();
    let mut resources = Vec::new();
    for declared in &activity.resources {
        let Some(entity) = entities_by_name.get(declared.value.as_str()) else {
            diagnostics.push(Diagnostic::error(
                "AS3103",
                format!("unknown activity resource entity `{}`", declared.value),
                declared.span.clone(),
            ));
            continue;
        };
        if !seen_entities.insert(entity.id.clone()) {
            diagnostics.push(Diagnostic::error(
                "AS3103",
                format!("duplicate activity resource entity `{}`", declared.value),
                declared.span.clone(),
            ));
            continue;
        }
        if !seen_resources.insert(entity.table_name.as_str()) {
            diagnostics.push(Diagnostic::error(
                "AS3103",
                format!("duplicate activity resource key `{}`", entity.table_name),
                declared.span.clone(),
            ));
            continue;
        }
        resources.push(ActivityResourceIr {
            entity: entity.id.clone(),
            resource: entity.table_name.clone(),
        });
    }
    resources.sort_by(|left, right| left.entity.cmp(&right.entity));

    let known_roles = auth
        .roles
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    let mut seen_roles = BTreeSet::new();
    for role in &activity.admin_roles {
        if !known_roles.contains(role.value.as_str()) {
            diagnostics.push(Diagnostic::error(
                "AS3104",
                format!("unknown activity admin role `{}`", role.value),
                role.span.clone(),
            ));
        }
        if !seen_roles.insert(role.value.as_str()) {
            diagnostics.push(Diagnostic::error(
                "AS3104",
                format!("duplicate activity admin role `{}`", role.value),
                role.span.clone(),
            ));
        }
    }

    let max_comment_bytes =
        activity
            .max_comment_bytes
            .as_ref()
            .map_or(DEFAULT_MAX_COMMENT_BYTES, |value| {
                if let Ok(value @ 1..=65_536) = u32::try_from(value.value) {
                    value
                } else {
                    diagnostics.push(Diagnostic::error(
                        "AS3105",
                        "activity max_comment_bytes must be between 1 and 65536",
                        value.span.clone(),
                    ));
                    u32::try_from(value.value.clamp(1, 65_536)).unwrap_or(65_536)
                }
            });

    ActivityIr {
        enabled: true,
        max_comment_bytes,
        attachments: activity.attachments,
        admin_roles: activity
            .admin_roles
            .iter()
            .map(|role| role.value.clone())
            .collect(),
        resources,
    }
}
