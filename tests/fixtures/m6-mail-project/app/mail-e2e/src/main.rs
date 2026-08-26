use appstruct_generated_backend::{MailError, MailState};
use sea_orm::Database;
use std::{collections::BTreeMap, env};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let database = Database::connect(env::var("DATABASE_URL")?).await?;
    let mode = env::args().nth(1).unwrap_or_else(|| "send".to_owned());
    if mode == "production-check" {
        match MailState::from_env(database) {
            Err(MailError::Configuration(message)) if message.contains("forbidden") => return Ok(()),
            _ => return Err("capture provider was not rejected in production".into()),
        }
    }

    let mail = MailState::from_env(database)?;
    let tenant_id = env::var("TENANT_ID")?.parse::<uuid::Uuid>()?;
    let mut variables = BTreeMap::from([
        ("project_name".to_owned(), "Mercury".to_owned()),
        ("recipient_name".to_owned(), "Ada".to_owned()),
    ]);
    let delivery = mail
        .send_template(
            "project-created",
            "ada@example.com",
            &variables,
            Some(tenant_id),
        )
        .await?;
    assert_eq!(delivery.provider, "capture");

    variables.remove("project_name");
    assert!(matches!(
        mail.send_template("project-created", "ada@example.com", &variables, None)
            .await,
        Err(MailError::Template(_))
    ));
    assert!(matches!(
        mail.send_template("welcome", "not-an-email", &variables, None)
            .await,
        Err(MailError::InvalidAddress)
    ));
    Ok(())
}
