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
    InvalidTenant,
    InvalidPrecondition,
    PreconditionRequired,
    ConcurrentModification,
    InvalidCredentialsInput,
    InvalidCsrf,
    InvalidResetToken,
    InvalidInvitationToken,
    InvalidEmailVerificationToken,
    InvalidOAuthState,
    OAuthConfiguration,
    OAuthProvider,
    Unauthorized,
    TooManyRequests,
    Forbidden,
    NotFound,
    Conflict(String),
    Validation(Vec<FieldViolation>),
    Database(DbErr),
    Internal,
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

impl ApiError {
    pub(crate) fn into_bulk_failure(self, id: &str) -> appstruct_runtime::BulkFailure {
        let (code, message) = match self {
            Self::InvalidId => ("invalid_id", "The resource identifier is invalid".to_owned()),
            Self::InvalidQuery(message) => ("invalid_query", message),
            Self::InvalidTenant => ("invalid_tenant", "A valid tenant is required".to_owned()),
            Self::InvalidPrecondition => (
                "invalid_precondition",
                "The record precondition is invalid".to_owned(),
            ),
            Self::PreconditionRequired => (
                "precondition_required",
                "An expected revision is required".to_owned(),
            ),
            Self::ConcurrentModification => (
                "concurrent_modification",
                "The record changed after it was loaded".to_owned(),
            ),
            Self::InvalidCredentialsInput => ("invalid_input", "The input is invalid".to_owned()),
            Self::InvalidCsrf => ("forbidden", "The request could not be verified".to_owned()),
            Self::InvalidResetToken
            | Self::InvalidInvitationToken
            | Self::InvalidEmailVerificationToken
            | Self::InvalidOAuthState => ("invalid_input", "The input is invalid".to_owned()),
            Self::OAuthConfiguration | Self::OAuthProvider | Self::Internal => {
                tracing::error!(id, "bulk item failed with an internal service error");
                ("internal_error", "The record could not be processed".to_owned())
            }
            Self::Unauthorized => ("unauthorized", "Authentication is required".to_owned()),
            Self::TooManyRequests => ("rate_limited", "Too many attempts".to_owned()),
            Self::Forbidden => ("forbidden", "The operation is not allowed".to_owned()),
            Self::NotFound => ("not_found", "The record was not found".to_owned()),
            Self::Conflict(message) => ("conflict", message),
            Self::Validation(fields) => {
                let message = fields
                    .into_iter()
                    .map(|field| format!("{}: {}", field.field, field.message))
                    .collect::<Vec<_>>()
                    .join("; ");
                ("validation_failed", message)
            }
            Self::Database(error) => {
                let detail = error.to_string();
                if detail.contains("duplicate key") || detail.contains("violates unique constraint") {
                    ("conflict", "The record conflicts with existing data".to_owned())
                } else {
                    tracing::error!(id, error = %detail, "bulk database operation failed");
                    ("database_error", "The record could not be persisted".to_owned())
                }
            }
        };
        appstruct_runtime::bulk_failure(id, code, message)
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
            Self::InvalidTenant => (
                StatusCode::BAD_REQUEST,
                "INVALID_TENANT",
                "A valid X-AppStruct-Tenant header is required".to_owned(),
                vec![],
            ),
            Self::InvalidPrecondition => (
                StatusCode::BAD_REQUEST,
                "INVALID_PRECONDITION",
                "The If-Match header is invalid".to_owned(),
                vec![],
            ),
            Self::PreconditionRequired => (
                StatusCode::PRECONDITION_REQUIRED,
                "PRECONDITION_REQUIRED",
                "The latest ETag must be supplied in If-Match".to_owned(),
                vec![],
            ),
            Self::ConcurrentModification => (
                StatusCode::PRECONDITION_FAILED,
                "CONCURRENT_MODIFICATION",
                "The resource changed after it was loaded".to_owned(),
                vec![],
            ),
            Self::InvalidCredentialsInput => (
                StatusCode::BAD_REQUEST,
                "INVALID_CREDENTIALS_INPUT",
                "The email or password does not meet the required format".to_owned(),
                vec![],
            ),
            Self::InvalidCsrf => (
                StatusCode::FORBIDDEN,
                "INVALID_CSRF",
                "The request could not be verified".to_owned(),
                vec![],
            ),
            Self::InvalidResetToken => (
                StatusCode::BAD_REQUEST,
                "INVALID_RESET_TOKEN",
                "The password reset link is invalid or expired".to_owned(),
                vec![],
            ),
            Self::InvalidInvitationToken => (
                StatusCode::BAD_REQUEST,
                "INVALID_INVITATION_TOKEN",
                "The organization invitation link is invalid or expired".to_owned(),
                vec![],
            ),
            Self::InvalidEmailVerificationToken => (
                StatusCode::BAD_REQUEST,
                "INVALID_EMAIL_VERIFICATION_TOKEN",
                "The email verification link is invalid or expired".to_owned(),
                vec![],
            ),
            Self::InvalidOAuthState => (
                StatusCode::BAD_REQUEST,
                "INVALID_OAUTH_STATE",
                "The OAuth login state is invalid or expired".to_owned(),
                vec![],
            ),
            Self::OAuthConfiguration => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "OAUTH_CONFIGURATION_ERROR",
                "OAuth is not configured correctly".to_owned(),
                vec![],
            ),
            Self::OAuthProvider => (
                StatusCode::BAD_GATEWAY,
                "OAUTH_PROVIDER_ERROR",
                "The OAuth provider could not authenticate the account".to_owned(),
                vec![],
            ),
            Self::Unauthorized => (
                StatusCode::UNAUTHORIZED,
                "UNAUTHENTICATED",
                "Authentication is required".to_owned(),
                vec![],
            ),
            Self::TooManyRequests => (
                StatusCode::TOO_MANY_REQUESTS,
                "RATE_LIMITED",
                "Too many authentication attempts".to_owned(),
                vec![],
            ),
            Self::NotFound => (
                StatusCode::NOT_FOUND,
                "NOT_FOUND",
                "The requested resource was not found".to_owned(),
                vec![],
            ),
            Self::Conflict(message) => (
                StatusCode::CONFLICT,
                "CONFLICT",
                message,
                vec![],
            ),
            Self::Forbidden => (
                StatusCode::FORBIDDEN,
                "FORBIDDEN",
                "The operation is not allowed".to_owned(),
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
            Self::Internal => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "INTERNAL_ERROR",
                "The request could not be completed".to_owned(),
                vec![],
            ),
        };
        (status, Json(ErrorEnvelope { error: ErrorBody { code, message, fields } })).into_response()
    }
}
