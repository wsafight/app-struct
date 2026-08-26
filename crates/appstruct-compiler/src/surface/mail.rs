use super::value::{ensure_known_keys, expect_bool, expect_mapping, expect_string, required};
use super::{Located, SurfaceMail, SurfaceMailTemplate};
use crate::yaml::MappingEntry;
use appstruct_ir::Diagnostic;

pub(super) fn decode(entry: Option<&MappingEntry>) -> Result<SurfaceMail, Diagnostic> {
    let Some(modules_entry) = entry else {
        return Ok(SurfaceMail::default());
    };
    let modules = expect_mapping(&modules_entry.value, "`modules`")?;
    let Some(entry) = modules.get("mail") else {
        return Ok(SurfaceMail::default());
    };
    let mail = expect_mapping(&entry.value, "`modules.mail`")?;
    ensure_known_keys(
        mail,
        &["enabled", "provider", "from", "templates"],
        "`modules.mail`",
    )?;
    let enabled = mail
        .get("enabled")
        .map(|value| expect_bool(&value.value, "`modules.mail.enabled`"))
        .transpose()?
        .unwrap_or(true);
    let provider = mail
        .get("provider")
        .map(|value| expect_string(&value.value, "`modules.mail.provider`"))
        .transpose()?;
    let from = mail
        .get("from")
        .map(|value| expect_string(&value.value, "`modules.mail.from`"))
        .transpose()?;
    let templates = mail
        .get("templates")
        .map(decode_templates)
        .transpose()?
        .unwrap_or_default();
    Ok(SurfaceMail {
        enabled,
        provider,
        from,
        templates,
        span: Some(entry.value.span.clone()),
    })
}

fn decode_templates(entry: &MappingEntry) -> Result<Vec<SurfaceMailTemplate>, Diagnostic> {
    let templates = expect_mapping(&entry.value, "`modules.mail.templates`")?;
    templates
        .iter()
        .map(|(name, entry)| {
            let template = expect_mapping(&entry.value, "mail template")?;
            ensure_known_keys(template, &["subject", "text", "html"], "mail template")?;
            let subject = required(template, "subject", &entry.value.span)?;
            let text = required(template, "text", &entry.value.span)?;
            Ok(SurfaceMailTemplate {
                name: Located {
                    value: name.clone(),
                    span: entry.key_span.clone(),
                },
                subject: expect_string(&subject.value, "mail template subject")?,
                text: expect_string(&text.value, "mail template text")?,
                html: template
                    .get("html")
                    .map(|value| expect_string(&value.value, "mail template HTML"))
                    .transpose()?,
            })
        })
        .collect()
}
