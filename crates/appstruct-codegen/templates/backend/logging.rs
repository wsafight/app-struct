use std::{env, error::Error, fmt};
use tracing_subscriber::EnvFilter;

const DEFAULT_FILTER: &str = "appstruct_generated_backend=info,tower_http=info";

#[derive(Debug)]
pub struct LoggingError(String);

impl fmt::Display for LoggingError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Error for LoggingError {}

pub fn init_tracing() -> Result<(), LoggingError> {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| DEFAULT_FILTER.into());
    match env::var("APPSTRUCT_LOG_FORMAT").as_deref() {
        Ok("json") => tracing_subscriber::fmt()
            .with_env_filter(filter)
            .json()
            .try_init()
            .map_err(|error| LoggingError(error.to_string()))?,
        Ok("text") | Err(env::VarError::NotPresent) => tracing_subscriber::fmt()
            .with_env_filter(filter)
            .try_init()
            .map_err(|error| LoggingError(error.to_string()))?,
        Err(error) => return Err(LoggingError(error.to_string())),
        Ok(value) => {
            return Err(LoggingError(format!(
                "APPSTRUCT_LOG_FORMAT must be `text` or `json`, got `{value}`"
            )));
        }
    }
    Ok(())
}
