use super::SurfaceActivity;
use super::value::{
    ensure_known_keys, expect_bool, expect_mapping, expect_sequence, expect_string, expect_u64,
};
use crate::yaml::MappingEntry;
use appstruct_ir::Diagnostic;

pub(super) fn decode(entry: Option<&MappingEntry>) -> Result<SurfaceActivity, Diagnostic> {
    let Some(modules_entry) = entry else {
        return Ok(SurfaceActivity::default());
    };
    let modules = expect_mapping(&modules_entry.value, "`modules`")?;
    let Some(entry) = modules.get("activity") else {
        return Ok(SurfaceActivity::default());
    };
    let activity = expect_mapping(&entry.value, "`modules.activity`")?;
    ensure_known_keys(
        activity,
        &[
            "enabled",
            "max_comment_bytes",
            "attachments",
            "admin_roles",
            "resources",
        ],
        "`modules.activity`",
    )?;
    let enabled = activity
        .get("enabled")
        .map(|value| expect_bool(&value.value, "`modules.activity.enabled`"))
        .transpose()?
        .unwrap_or(true);
    Ok(SurfaceActivity {
        enabled,
        max_comment_bytes: activity
            .get("max_comment_bytes")
            .map(|value| expect_u64(&value.value, "activity max comment bytes"))
            .transpose()?,
        attachments: activity
            .get("attachments")
            .map(|value| expect_bool(&value.value, "`modules.activity.attachments`"))
            .transpose()?
            .unwrap_or_default(),
        admin_roles: string_list(activity.get("admin_roles"), "activity admin role")?,
        resources: string_list(activity.get("resources"), "activity resource entity")?,
        span: Some(entry.value.span.clone()),
    })
}

fn string_list(
    entry: Option<&MappingEntry>,
    context: &str,
) -> Result<Vec<super::Located<String>>, Diagnostic> {
    entry
        .map(|value| {
            expect_sequence(&value.value, context)?
                .iter()
                .map(|item| expect_string(item, context))
                .collect()
        })
        .transpose()
        .map(Option::unwrap_or_default)
}
