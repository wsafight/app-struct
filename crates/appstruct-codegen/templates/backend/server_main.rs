use appstruct_generated_backend::{Application, connect_database, init_tracing};
use std::{env, net::SocketAddr};
use tokio::net::TcpListener;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    init_tracing()?;

    let database_url = env::var("DATABASE_URL")?;
    let bind = env::var("APPSTRUCT_BIND").unwrap_or_else(|_| "127.0.0.1:3000".to_owned());
    let address: SocketAddr = bind.parse()?;
    let database = connect_database(database_url).await?;
    let listener = TcpListener::bind(address).await?;
    let application =
        Application::from_env(database, appstruct_app_backend::extensions()).await?;
    tracing::info!(%address, "AppStruct API listening");
    application.serve(listener).await?;
    Ok(())
}
