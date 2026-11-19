use axum::{
    Json,
    extract::{FromRequest, rejection::JsonRejection},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde_json::{Value, json};

pub type AppResult<T> = Result<T, AppError>;

/// Every error leaves the API as `{"error": {"code", "message", "details"?}}`.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("{0}")]
    BadRequest(String),
    #[error("authentication required")]
    Unauthorized,
    #[error("{0}")]
    Forbidden(String),
    #[error("{0}")]
    NotFound(String),
    #[error("{message}")]
    Conflict { code: &'static str, message: String },
    #[error("{message}")]
    Gone {
        code: &'static str,
        message: String,
        details: Value,
    },
    #[error("the card version does not pass the publication gate")]
    GateFailed(Vec<String>),
    #[error(transparent)]
    Database(#[from] sqlx::Error),
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}

impl AppError {
    pub fn not_found(what: &str) -> Self {
        Self::NotFound(format!("{what} not found"))
    }
}

impl From<JsonRejection> for AppError {
    fn from(rejection: JsonRejection) -> Self {
        Self::BadRequest(rejection.body_text())
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let message = self.to_string();
        let (status, code, message, details) = match self {
            Self::BadRequest(m) => (StatusCode::BAD_REQUEST, "bad_request", m, None),
            Self::Unauthorized => (StatusCode::UNAUTHORIZED, "unauthorized", message, None),
            Self::Forbidden(m) => (StatusCode::FORBIDDEN, "forbidden", m, None),
            Self::NotFound(m) => (StatusCode::NOT_FOUND, "not_found", m, None),
            Self::Conflict { code, message } => (StatusCode::CONFLICT, code, message, None),
            Self::Gone { code, message, details } => (StatusCode::GONE, code, message, Some(details)),
            Self::GateFailed(reasons) => (
                StatusCode::UNPROCESSABLE_ENTITY,
                "publication_gate_failed",
                message,
                Some(json!(reasons)),
            ),
            Self::Database(e) => return database_error(e),
            Self::Internal(e) => {
                tracing::error!(error = ?e, "internal error");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal",
                    "internal server error".into(),
                    None,
                )
            }
        };
        envelope(status, code, &message, details)
    }
}

/// Turn constraint violations into client errors; everything else is a 500.
fn database_error(e: sqlx::Error) -> Response {
    if let sqlx::Error::Database(db) = &e {
        let constraint = db.constraint().unwrap_or("unknown").to_string();
        let mapped = match db.code().as_deref() {
            Some("23505") => Some((StatusCode::CONFLICT, "already_exists")),
            Some("23514") => Some((StatusCode::BAD_REQUEST, "constraint_violation")),
            Some("23503") => Some((StatusCode::BAD_REQUEST, "invalid_reference")),
            Some("23502") => Some((StatusCode::BAD_REQUEST, "missing_field")),
            Some("22P02" | "22007" | "22008") => Some((StatusCode::BAD_REQUEST, "invalid_value")),
            _ => None,
        };
        if let Some((status, code)) = mapped {
            let message = format!("request violates database rule `{constraint}`");
            return envelope(status, code, &message, Some(json!({ "constraint": constraint })));
        }
    }
    tracing::error!(error = ?e, "database error");
    envelope(
        StatusCode::INTERNAL_SERVER_ERROR,
        "internal",
        "internal server error",
        None,
    )
}

fn envelope(status: StatusCode, code: &str, message: &str, details: Option<Value>) -> Response {
    let mut error = json!({ "code": code, "message": message });
    if let Some(details) = details {
        error["details"] = details;
    }
    (status, Json(json!({ "error": error }))).into_response()
}

/// JSON body extractor whose rejections use the standard error envelope.
#[derive(FromRequest)]
#[from_request(via(axum::Json), rejection(AppError))]
pub struct ApiJson<T>(pub T);

/// Normalize framework rejections (path/query/body limits and routing errors).
/// Keep status and headers such as Allow; existing application JSON errors pass through.
pub async fn normalize_errors(request: axum::extract::Request, next: axum::middleware::Next) -> Response {
    let response = next.run(request).await;
    let status = response.status();
    if !(status.is_client_error() || status.is_server_error())
        || response
            .headers()
            .get(axum::http::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .is_some_and(|v| v.starts_with("application/json"))
    {
        return response;
    }
    let code = match status {
        StatusCode::BAD_REQUEST => "bad_request",
        StatusCode::NOT_FOUND => "not_found",
        StatusCode::METHOD_NOT_ALLOWED => "method_not_allowed",
        StatusCode::PAYLOAD_TOO_LARGE => "payload_too_large",
        StatusCode::UNSUPPORTED_MEDIA_TYPE => "unsupported_media_type",
        _ => "http_error",
    };
    let (mut parts, _) = response.into_parts();
    let json_response = envelope(
        status,
        code,
        status.canonical_reason().unwrap_or("request failed"),
        None,
    );
    parts.headers.remove(axum::http::header::CONTENT_LENGTH);
    parts.headers.insert(
        axum::http::header::CONTENT_TYPE,
        axum::http::HeaderValue::from_static("application/json"),
    );
    Response::from_parts(parts, json_response.into_body())
}
