use crate::ApiError;
use async_trait::async_trait;
use lettre::message::Mailbox;
use lettre::transport::smtp::authentication::Credentials;
use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};
use sea_orm::{ConnectionTrait, DatabaseConnection, DbBackend, Statement};
use std::env;

#[async_trait]
pub trait AuthMailSender: Send + Sync {
    async fn send_password_reset(
        &self,
        database: &DatabaseConnection,
        recipient: &str,
        reset_url: &str,
    ) -> Result<(), ApiError>;
}

pub struct DevMailSender;

#[async_trait]
impl AuthMailSender for DevMailSender {
    async fn send_password_reset(
        &self,
        database: &DatabaseConnection,
        recipient: &str,
        reset_url: &str,
    ) -> Result<(), ApiError> {
        database
            .execute_raw(Statement::from_sql_and_values(
                DbBackend::Postgres,
                "INSERT INTO \"_appstruct_auth_mail_capture\" (id, recipient, subject, body, created_at) VALUES ($1, $2, $3, $4, CURRENT_TIMESTAMP)",
                [
                    uuid::Uuid::now_v7().into(),
                    recipient.to_owned().into(),
                    "Reset your password".to_owned().into(),
                    reset_url.to_owned().into(),
                ],
            ))
            .await?;
        Ok(())
    }
}

pub(super) struct DisabledMailSender;

#[async_trait]
impl AuthMailSender for DisabledMailSender {
    async fn send_password_reset(
        &self,
        _database: &DatabaseConnection,
        _recipient: &str,
        _reset_url: &str,
    ) -> Result<(), ApiError> {
        Err(ApiError::NotFound)
    }
}

pub struct SmtpMailSender {
    transport: AsyncSmtpTransport<Tokio1Executor>,
    from: Mailbox,
}

impl SmtpMailSender {
    pub fn from_env() -> Result<Self, String> {
        let host = required("APPSTRUCT_SMTP_HOST")?;
        let username = required("APPSTRUCT_SMTP_USERNAME")?;
        let password = required("APPSTRUCT_SMTP_PASSWORD")?;
        let from = required("APPSTRUCT_SMTP_FROM")?
            .parse()
            .map_err(|error| format!("invalid APPSTRUCT_SMTP_FROM: {error}"))?;
        let mut builder = AsyncSmtpTransport::<Tokio1Executor>::relay(&host)
            .map_err(|error| format!("invalid SMTP relay: {error}"))?
            .credentials(Credentials::new(username, password));
        if let Ok(port) = env::var("APPSTRUCT_SMTP_PORT") {
            builder = builder.port(
                port.parse()
                    .map_err(|error| format!("invalid APPSTRUCT_SMTP_PORT: {error}"))?,
            );
        }
        Ok(Self {
            transport: builder.build(),
            from,
        })
    }
}

#[async_trait]
impl AuthMailSender for SmtpMailSender {
    async fn send_password_reset(
        &self,
        _database: &DatabaseConnection,
        recipient: &str,
        reset_url: &str,
    ) -> Result<(), ApiError> {
        let message = Message::builder()
            .from(self.from.clone())
            .to(recipient.parse().map_err(|_| ApiError::Internal)?)
            .subject("Reset your password")
            .body(format!("Open this link to reset your password:\n\n{reset_url}\n"))
            .map_err(|_| ApiError::Internal)?;
        self.transport
            .send(message)
            .await
            .map_err(|error| {
                tracing::error!(%error, "auth SMTP delivery failed");
                ApiError::Internal
            })?;
        Ok(())
    }
}

fn required(name: &str) -> Result<String, String> {
    env::var(name).map_err(|_| format!("{name} is required for SMTP auth mail"))
}
