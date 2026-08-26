use super::render;
use crate::{Artifact, ArtifactKind, CodegenError};
use appstruct_ir::{AppIr, MailProviderIr};
use quote::quote;

pub(super) fn plan(ir: &AppIr) -> Result<Vec<Artifact>, CodegenError> {
    let source = if ir.mail.enabled {
        enabled_source(ir)?
    } else {
        disabled_source()?
    };
    Ok(vec![Artifact::text(
        "backend/src/mail.rs",
        source,
        ArtifactKind::RustSource,
    )])
}

fn enabled_source(ir: &AppIr) -> Result<String, CodegenError> {
    let sender = &ir.mail.from;
    let provider = provider_initializer(ir.mail.provider);
    let contract = contract_source(sender, &provider);
    let templates = template_source(ir);
    let adapter = match ir.mail.provider {
        MailProviderIr::Capture => capture_source(),
        MailProviderIr::Smtp => smtp_source(),
        MailProviderIr::Resend => resend_source(),
    };
    let required = (!matches!(ir.mail.provider, MailProviderIr::Capture)).then(|| {
        quote! {
            fn required(name: &str) -> Result<String, MailError> {
                env::var(name)
                    .map_err(|_| MailError::Configuration(format!("{name} is required")))
            }
        }
    });
    render(quote! {
        use async_trait::async_trait;
        use lettre::message::Mailbox;
        use sea_orm::{DatabaseConnection, DbErr};
        use serde::Serialize;
        use std::{collections::BTreeMap, env, fmt, sync::Arc};
        #contract
        #templates
        #adapter
        #required
    })
}

fn provider_initializer(provider: MailProviderIr) -> proc_macro2::TokenStream {
    match provider {
        MailProviderIr::Capture => quote! {{
            if env::var("APPSTRUCT_ENV").as_deref() == Ok("production") {
                return Err(MailError::Configuration(
                    "capture mail provider is forbidden in production".to_owned()
                ));
            }
            Arc::new(CaptureProvider { database }) as Arc<dyn MailProvider>
        }},
        MailProviderIr::Smtp => {
            quote! { Arc::new(SmtpProvider::from_env()?) as Arc<dyn MailProvider> }
        }
        MailProviderIr::Resend => {
            quote! { Arc::new(ResendProvider::from_env()?) as Arc<dyn MailProvider> }
        }
    }
}

fn template_source(ir: &AppIr) -> proc_macro2::TokenStream {
    let templates = ir.mail.templates.iter().map(|template| {
        let name = &template.name;
        let subject = &template.subject;
        let text = &template.text;
        let html = template
            .html
            .as_ref()
            .map_or_else(|| quote! { None }, |html| quote! { Some(#html) });
        quote! {
            #name => Some(MailTemplate { name: #name, subject: #subject, text: #text, html: #html })
        }
    });
    quote! {
        #[derive(Clone, Copy)]
        struct MailTemplate {
            name: &'static str,
            subject: &'static str,
            text: &'static str,
            html: Option<&'static str>,
        }
        fn template(name: &str) -> Option<MailTemplate> {
            match name { #(#templates,)* _ => None }
        }
        fn render_message(
            template: MailTemplate,
            sender: &str,
            recipient: &str,
            variables: &BTreeMap<String, String>,
            tenant_id: Option<uuid::Uuid>,
        ) -> Result<MailMessage, MailError> {
            let mut environment = minijinja::Environment::new();
            environment.set_undefined_behavior(minijinja::UndefinedBehavior::Strict);
            environment.add_template("subject", template.subject)
                .map_err(|error| MailError::Template(error.to_string()))?;
            environment.add_template("text", template.text)
                .map_err(|error| MailError::Template(error.to_string()))?;
            if let Some(html) = template.html {
                environment.add_template("html", html)
                    .map_err(|error| MailError::Template(error.to_string()))?;
            }
            let render = |name| environment.get_template(name)
                .and_then(|value| value.render(variables))
                .map_err(|error| MailError::Template(error.to_string()));
            Ok(MailMessage {
                template: template.name.to_owned(), sender: sender.to_owned(),
                recipient: recipient.to_owned(), subject: render("subject")?,
                text: render("text")?, html: template.html.map(|_| render("html")).transpose()?,
                tenant_id,
            })
        }
    }
}

fn contract_source(sender: &str, provider: &proc_macro2::TokenStream) -> proc_macro2::TokenStream {
    quote! {
        #[derive(Clone, Debug, Serialize)]
        pub struct MailMessage {
            pub template: String,
            pub sender: String,
            pub recipient: String,
            pub subject: String,
            pub text: String,
            pub html: Option<String>,
            pub tenant_id: Option<uuid::Uuid>,
        }

        #[derive(Clone, Debug, PartialEq, Eq)]
        pub struct MailDelivery { pub id: String, pub provider: String }

        #[derive(Debug)]
        pub enum MailError {
            Disabled,
            UnknownTemplate(String),
            InvalidAddress,
            Template(String),
            Configuration(String),
            Delivery(String),
            Database(DbErr),
        }

        impl fmt::Display for MailError {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                match self {
                    Self::Disabled => formatter.write_str("mail module is disabled"),
                    Self::UnknownTemplate(name) => write!(formatter, "unknown mail template `{name}`"),
                    Self::InvalidAddress => formatter.write_str("mail sender or recipient is invalid"),
                    Self::Template(error) => write!(formatter, "mail template rendering failed: {error}"),
                    Self::Configuration(error) => write!(formatter, "mail configuration is invalid: {error}"),
                    Self::Delivery(error) => write!(formatter, "mail delivery failed: {error}"),
                    Self::Database(error) => write!(formatter, "mail capture failed: {error}"),
                }
            }
        }

        impl std::error::Error for MailError {}
        impl From<DbErr> for MailError {
            fn from(error: DbErr) -> Self { Self::Database(error) }
        }

        #[async_trait]
        pub trait MailProvider: Send + Sync {
            fn name(&self) -> &'static str;
            async fn deliver(&self, message: &MailMessage) -> Result<MailDelivery, MailError>;
        }

        #[derive(Clone)]
        pub struct MailState { provider: Arc<dyn MailProvider> }

        impl MailState {
            pub fn from_env(database: DatabaseConnection) -> Result<Self, MailError> {
                let provider = #provider;
                Ok(Self { provider })
            }

            pub fn with_provider(provider: Arc<dyn MailProvider>) -> Self { Self { provider } }

            pub async fn send_template(
                &self,
                name: &str,
                recipient: &str,
                variables: &BTreeMap<String, String>,
                tenant_id: Option<uuid::Uuid>,
            ) -> Result<MailDelivery, MailError> {
                let template = template(name)
                    .ok_or_else(|| MailError::UnknownTemplate(name.to_owned()))?;
                let sender = #sender;
                let _: Mailbox = sender.parse().map_err(|_| MailError::InvalidAddress)?;
                let _: Mailbox = recipient.parse().map_err(|_| MailError::InvalidAddress)?;
                let message = render_message(template, sender, recipient, variables, tenant_id)?;
                self.provider.deliver(&message).await
            }
        }
    }
}

fn capture_source() -> proc_macro2::TokenStream {
    quote! {
        use sea_orm::{ConnectionTrait, DbBackend, Statement};
        struct CaptureProvider { database: DatabaseConnection }

        #[async_trait]
        impl MailProvider for CaptureProvider {
            fn name(&self) -> &'static str { "capture" }
            async fn deliver(&self, message: &MailMessage) -> Result<MailDelivery, MailError> {
                let id = uuid::Uuid::now_v7();
                self.database.execute_raw(Statement::from_sql_and_values(
                    DbBackend::Postgres,
                    "INSERT INTO \"_appstruct_mail_deliveries\" (id, provider, template, sender, recipient, subject, text_body, html_body, tenant_id, created_at) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, CURRENT_TIMESTAMP)",
                    [
                        id.into(), self.name().to_owned().into(), message.template.clone().into(),
                        message.sender.clone().into(), message.recipient.clone().into(),
                        message.subject.clone().into(), message.text.clone().into(),
                        message.html.clone().into(), message.tenant_id.into(),
                    ],
                )).await?;
                Ok(MailDelivery { id: id.to_string(), provider: self.name().to_owned() })
            }
        }
    }
}

fn smtp_source() -> proc_macro2::TokenStream {
    quote! {
        use lettre::message::{MultiPart, SinglePart, header::ContentType};
        use lettre::transport::smtp::authentication::Credentials;
        use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};
        struct SmtpProvider { transport: AsyncSmtpTransport<Tokio1Executor> }

        impl SmtpProvider {
            fn from_env() -> Result<Self, MailError> {
                let host = required("APPSTRUCT_SMTP_HOST")?;
                let credentials = Credentials::new(
                    required("APPSTRUCT_SMTP_USERNAME")?, required("APPSTRUCT_SMTP_PASSWORD")?,
                );
                let mut builder = AsyncSmtpTransport::<Tokio1Executor>::relay(&host)
                    .map_err(|error| MailError::Configuration(error.to_string()))?
                    .credentials(credentials);
                if let Ok(port) = env::var("APPSTRUCT_SMTP_PORT") {
                    builder = builder.port(port.parse().map_err(|error| {
                        MailError::Configuration(format!("invalid APPSTRUCT_SMTP_PORT: {error}"))
                    })?);
                }
                Ok(Self { transport: builder.build() })
            }
        }

        #[async_trait]
        impl MailProvider for SmtpProvider {
            fn name(&self) -> &'static str { "smtp" }
            async fn deliver(&self, message: &MailMessage) -> Result<MailDelivery, MailError> {
                let builder = Message::builder()
                    .from(message.sender.parse().map_err(|_| MailError::InvalidAddress)?)
                    .to(message.recipient.parse().map_err(|_| MailError::InvalidAddress)?)
                    .subject(&message.subject);
                let email = if let Some(html) = &message.html {
                    builder.multipart(
                        MultiPart::alternative()
                            .singlepart(SinglePart::builder()
                                .header(ContentType::TEXT_PLAIN).body(message.text.clone()))
                            .singlepart(SinglePart::builder()
                                .header(ContentType::TEXT_HTML).body(html.clone()))
                    )
                } else {
                    builder.body(message.text.clone())
                }.map_err(|error| MailError::Delivery(error.to_string()))?;
                self.transport.send(email).await
                    .map_err(|error| MailError::Delivery(error.to_string()))?;
                Ok(MailDelivery {
                    id: uuid::Uuid::now_v7().to_string(), provider: self.name().to_owned(),
                })
            }
        }
    }
}

fn resend_source() -> proc_macro2::TokenStream {
    quote! {
        struct ResendProvider { client: reqwest::Client, api_key: String }

        impl ResendProvider {
            fn from_env() -> Result<Self, MailError> {
                Ok(Self {
                    client: reqwest::Client::new(), api_key: required("APPSTRUCT_RESEND_API_KEY")?,
                })
            }
        }

        #[async_trait]
        impl MailProvider for ResendProvider {
            fn name(&self) -> &'static str { "resend" }
            async fn deliver(&self, message: &MailMessage) -> Result<MailDelivery, MailError> {
                let payload = serde_json::json!({
                    "from": &message.sender, "to": [&message.recipient],
                    "subject": &message.subject, "text": &message.text, "html": &message.html,
                });
                let response = self.client.post("https://api.resend.com/emails")
                    .bearer_auth(&self.api_key).json(&payload).send().await
                    .map_err(|error| MailError::Delivery(error.to_string()))?;
                if !response.status().is_success() {
                    return Err(MailError::Delivery(format!("resend returned {}", response.status())));
                }
                let body: serde_json::Value = response.json().await
                    .map_err(|error| MailError::Delivery(error.to_string()))?;
                let id = body.get("id").and_then(serde_json::Value::as_str)
                    .ok_or_else(|| MailError::Delivery("resend response omitted id".to_owned()))?;
                Ok(MailDelivery { id: id.to_owned(), provider: self.name().to_owned() })
            }
        }
    }
}

fn disabled_source() -> Result<String, CodegenError> {
    render(quote! {
        use async_trait::async_trait;
        use sea_orm::DatabaseConnection;
        use std::{collections::BTreeMap, fmt, sync::Arc};

        #[derive(Clone, Debug, serde::Serialize)]
        pub struct MailMessage;
        #[derive(Clone, Debug, PartialEq, Eq)]
        pub struct MailDelivery { pub id: String, pub provider: String }
        #[derive(Debug)]
        pub enum MailError { Disabled }
        impl fmt::Display for MailError {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("mail module is disabled")
            }
        }
        impl std::error::Error for MailError {}
        #[async_trait]
        pub trait MailProvider: Send + Sync {
            async fn deliver(&self, message: &MailMessage) -> Result<MailDelivery, MailError>;
        }
        #[derive(Clone, Default)]
        pub struct MailState;
        impl MailState {
            pub fn from_env(_database: DatabaseConnection) -> Result<Self, MailError> { Ok(Self) }
            pub fn with_provider(_provider: Arc<dyn MailProvider>) -> Self { Self }
            pub async fn send_template(
                &self, _name: &str, _recipient: &str,
                _variables: &BTreeMap<String, String>, _tenant_id: Option<uuid::Uuid>,
            ) -> Result<MailDelivery, MailError> { Err(MailError::Disabled) }
        }
    })
}
