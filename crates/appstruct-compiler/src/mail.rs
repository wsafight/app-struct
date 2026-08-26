use crate::surface::{SurfaceMail, SurfaceMailTemplate};
use appstruct_ir::{Diagnostic, MailIr, MailProviderIr, MailTemplateIr, SourceSpan};

pub(crate) fn lower_mail(
    mail: &SurfaceMail,
    fallback: &SourceSpan,
    diagnostics: &mut Vec<Diagnostic>,
) -> MailIr {
    if !mail.enabled {
        return MailIr {
            enabled: false,
            provider: MailProviderIr::Capture,
            from: String::new(),
            templates: Vec::new(),
        };
    }
    let span = mail.span.as_ref().unwrap_or(fallback);
    let provider = lower_provider(mail, span, diagnostics);
    let from = mail.from.as_ref().map_or_else(String::new, |value| {
        if !value.value.contains('@') {
            diagnostics.push(Diagnostic::error(
                "AS3042",
                "`modules.mail.from` must contain a valid sender mailbox",
                value.span.clone(),
            ));
        }
        value.value.clone()
    });
    if mail.from.is_none() {
        diagnostics.push(Diagnostic::error(
            "AS3042",
            "enabled mail module requires `modules.mail.from`",
            span.clone(),
        ));
    }
    if mail.templates.is_empty() {
        diagnostics.push(Diagnostic::error(
            "AS3043",
            "enabled mail module requires at least one template",
            span.clone(),
        ));
    }
    let templates = mail
        .templates
        .iter()
        .map(|template| lower_template(template, diagnostics))
        .collect();
    MailIr {
        enabled: true,
        provider,
        from,
        templates,
    }
}

fn lower_provider(
    mail: &SurfaceMail,
    span: &SourceSpan,
    diagnostics: &mut Vec<Diagnostic>,
) -> MailProviderIr {
    let Some(provider) = &mail.provider else {
        diagnostics.push(Diagnostic::error(
            "AS3041",
            "enabled mail module requires `modules.mail.provider`",
            span.clone(),
        ));
        return MailProviderIr::Capture;
    };
    match provider.value.as_str() {
        "capture" => MailProviderIr::Capture,
        "smtp" => MailProviderIr::Smtp,
        "resend" => MailProviderIr::Resend,
        _ => {
            diagnostics.push(Diagnostic::error(
                "AS3041",
                "mail provider must be `capture`, `smtp`, or `resend`",
                provider.span.clone(),
            ));
            MailProviderIr::Capture
        }
    }
}

fn lower_template(
    template: &SurfaceMailTemplate,
    diagnostics: &mut Vec<Diagnostic>,
) -> MailTemplateIr {
    if !valid_name(&template.name.value) {
        diagnostics.push(Diagnostic::error(
            "AS3044",
            "mail template name must use lowercase letters, digits, `_`, or `-`",
            template.name.span.clone(),
        ));
    }
    validate_source("subject", &template.subject, diagnostics);
    validate_source("text", &template.text, diagnostics);
    if let Some(html) = &template.html {
        validate_source("html", html, diagnostics);
    }
    MailTemplateIr {
        name: template.name.value.clone(),
        subject: template.subject.value.clone(),
        text: template.text.value.clone(),
        html: template.html.as_ref().map(|value| value.value.clone()),
    }
}

fn validate_source(
    field: &str,
    source: &crate::surface::Located<String>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if source.value.trim().is_empty() {
        diagnostics.push(Diagnostic::error(
            "AS3045",
            format!("mail template `{field}` must not be empty"),
            source.span.clone(),
        ));
        return;
    }
    let mut environment = minijinja::Environment::new();
    if let Err(error) = environment.add_template("mail", &source.value) {
        diagnostics.push(Diagnostic::error(
            "AS3046",
            format!("invalid MiniJinja in mail template `{field}`: {error}"),
            source.span.clone(),
        ));
    }
}

fn valid_name(value: &str) -> bool {
    !value.is_empty()
        && value.chars().all(|character| {
            character.is_ascii_lowercase()
                || character.is_ascii_digit()
                || matches!(character, '_' | '-')
        })
}
