use appstruct_generated_backend::{AppExtensions, Application};
use sea_orm::Database;
use std::{env, net::SocketAddr};
use tokio::net::TcpListener;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "appstruct_generated_backend=info,tower_http=info".into()),
        )
        .init();

    let database_url = env::var("DATABASE_URL")?;
    let bind = env::var("APPSTRUCT_BIND").unwrap_or_else(|_| "127.0.0.1:3000".to_owned());
    let address: SocketAddr = bind.parse()?;
    let database = Database::connect(database_url).await?;
    let extensions = AppExtensions::builder().build();
    let listener = TcpListener::bind(address).await?;
    let application = Application::from_env(database, extensions).await?;
    tracing::info!(%address, "AppStruct API listening");
    application.serve(listener).await?;
    Ok(())
}
