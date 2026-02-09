use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};
use tracing::error;

#[derive(Debug)]
pub enum AppError {
    Internal(anyhow::Error),
    NotFound(String),
    BadRequest(String),
}

impl IntoResponse for AppError {
    /// Converts the application error into an HTTP response.
    ///
    /// * `Internal` errors are logged and return 500.
    /// * `NotFound` returns 404.
    /// * `BadRequest` returns 400.
    fn into_response(self) -> Response {
        let (status, error_message) = match self {
            AppError::Internal(inner) => {
                error!("Internal error: {:#}", inner); // Log full context
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Internal Server Error".to_string(),
                )
            }
            AppError::NotFound(msg) => (StatusCode::NOT_FOUND, msg),
            AppError::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg),
        };

        (status, error_message).into_response()
    }
}

// Enable `?` for standard errors -> AppError::Internal
impl<E> From<E> for AppError
where
    E: Into<anyhow::Error>,
{
    fn from(err: E) -> Self {
        Self::Internal(err.into())
    }
}
