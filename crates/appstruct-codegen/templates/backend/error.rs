use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use sea_orm::DbErr;
use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
pub struct FieldViolation {
    pub field: String,
    pub message: String,
}

#[derive(Debug)]
pub enum ApiError {
    InvalidId,
    InvalidQuery(String),
    NotFound,
    Validation(Vec<FieldViolation>),
    Database(DbErr),
}

#[derive(Serialize)]
struct ErrorEnvelope {
    error: ErrorBody,
}

#[derive(Serialize)]
struct ErrorBody {
    code: &'static str,
    message: String,
    fields: Vec<FieldViolation>,
}

impl From<DbErr> for ApiError {
    fn from(error: DbErr) -> Self {
        Self::Database(error)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, code, message, fields) = match self {
            Self::InvalidId => (
                StatusCode::BAD_REQUEST,
                "INVALID_ID",
                "The resource identifier is invalid".to_owned(),
                vec![],
            ),
            Self::InvalidQuery(message) => (
                StatusCode::BAD_REQUEST,
                "INVALID_QUERY",
                message,
                vec![],
            ),
            Self::NotFound => (
                StatusCode::NOT_FOUND,
                "NOT_FOUND",
                "The requested resource was not found".to_owned(),
                vec![],
            ),
            Self::Validation(fields) => (
                StatusCode::UNPROCESSABLE_ENTITY,
                "VALIDATION_FAILED",
                "One or more fields are invalid".to_owned(),
                fields,
            ),
            Self::Database(error) => {
                let message = error.to_string();
                let conflict = message.contains("duplicate key")
                    || message.contains("violates unique constraint");
                if conflict {
                    (
                        StatusCode::CONFLICT,
                        "CONFLICT",
                        "The write conflicts with existing data".to_owned(),
                        vec![],
                    )
                } else {
                    tracing::error!(error = %message, "database operation failed");
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "DATABASE_ERROR",
                        "The database operation failed".to_owned(),
                        vec![],
                    )
                }
            }
        };
        (status, Json(ErrorEnvelope { error: ErrorBody { code, message, fields } })).into_response()
    }
}
