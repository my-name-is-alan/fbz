use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Serialize;
use tracing::error;

#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AppError {
    Unauthorized { message: String },
    Forbidden { message: String },
    NotFound { message: String },
    Conflict { message: String },
    RangeNotSatisfiable { message: String },
    TooManyRequests { message: String },
    UnprocessableEntity { message: String },
    Internal { message: String },
}

#[derive(Serialize)]
struct ErrorEnvelope<'a> {
    error: ErrorBody<'a>,
}

#[derive(Serialize)]
struct ErrorBody<'a> {
    code: &'a str,
    message: &'a str,
}

#[allow(dead_code)]
impl AppError {
    pub fn unauthorized(message: impl Into<String>) -> Self {
        Self::Unauthorized {
            message: message.into(),
        }
    }

    pub fn forbidden(message: impl Into<String>) -> Self {
        Self::Forbidden {
            message: message.into(),
        }
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self::NotFound {
            message: message.into(),
        }
    }

    pub fn conflict(message: impl Into<String>) -> Self {
        Self::Conflict {
            message: message.into(),
        }
    }

    pub fn range_not_satisfiable(message: impl Into<String>) -> Self {
        Self::RangeNotSatisfiable {
            message: message.into(),
        }
    }

    pub fn too_many_requests(message: impl Into<String>) -> Self {
        Self::TooManyRequests {
            message: message.into(),
        }
    }

    pub fn unprocessable(message: impl Into<String>) -> Self {
        Self::UnprocessableEntity {
            message: message.into(),
        }
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::Internal {
            message: message.into(),
        }
    }

    pub fn status_code(&self) -> StatusCode {
        match self {
            Self::Unauthorized { .. } => StatusCode::UNAUTHORIZED,
            Self::Forbidden { .. } => StatusCode::FORBIDDEN,
            Self::NotFound { .. } => StatusCode::NOT_FOUND,
            Self::Conflict { .. } => StatusCode::CONFLICT,
            Self::RangeNotSatisfiable { .. } => StatusCode::RANGE_NOT_SATISFIABLE,
            Self::TooManyRequests { .. } => StatusCode::TOO_MANY_REQUESTS,
            Self::UnprocessableEntity { .. } => StatusCode::UNPROCESSABLE_ENTITY,
            Self::Internal { .. } => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    pub fn code(&self) -> &'static str {
        match self {
            Self::Unauthorized { .. } => "unauthorized",
            Self::Forbidden { .. } => "forbidden",
            Self::NotFound { .. } => "not_found",
            Self::Conflict { .. } => "conflict",
            Self::RangeNotSatisfiable { .. } => "range_not_satisfiable",
            Self::TooManyRequests { .. } => "too_many_requests",
            Self::UnprocessableEntity { .. } => "unprocessable_entity",
            Self::Internal { .. } => "internal_server_error",
        }
    }

    pub fn message(&self) -> &str {
        match self {
            Self::Unauthorized { message }
            | Self::Forbidden { message }
            | Self::NotFound { message }
            | Self::Conflict { message }
            | Self::RangeNotSatisfiable { message }
            | Self::TooManyRequests { message }
            | Self::UnprocessableEntity { message }
            | Self::Internal { message } => message,
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        if matches!(self, Self::Internal { .. }) {
            error!(
                code = self.code(),
                message = self.message(),
                "request failed"
            );
        }

        let status = self.status_code();
        let body = Json(ErrorEnvelope {
            error: ErrorBody {
                code: self.code(),
                message: self.message(),
            },
        });

        (status, body).into_response()
    }
}

#[cfg(test)]
mod tests {
    use axum::response::IntoResponse;

    use super::*;

    #[test]
    fn errors_map_to_expected_status_codes() {
        let cases = [
            (
                AppError::unauthorized("missing token"),
                StatusCode::UNAUTHORIZED,
            ),
            (AppError::forbidden("denied"), StatusCode::FORBIDDEN),
            (AppError::not_found("missing"), StatusCode::NOT_FOUND),
            (AppError::conflict("exists"), StatusCode::CONFLICT),
            (
                AppError::range_not_satisfiable("outside resource length"),
                StatusCode::RANGE_NOT_SATISFIABLE,
            ),
            (
                AppError::too_many_requests("rate limited"),
                StatusCode::TOO_MANY_REQUESTS,
            ),
            (
                AppError::unprocessable("invalid input"),
                StatusCode::UNPROCESSABLE_ENTITY,
            ),
            (
                AppError::internal("boom"),
                StatusCode::INTERNAL_SERVER_ERROR,
            ),
        ];

        for (error, expected) in cases {
            assert_eq!(error.status_code(), expected);
            assert_eq!(error.into_response().status(), expected);
        }
    }

    #[test]
    fn error_codes_are_stable() {
        assert_eq!(
            AppError::unauthorized("missing token").code(),
            "unauthorized"
        );
        assert_eq!(AppError::forbidden("denied").code(), "forbidden");
        assert_eq!(AppError::not_found("missing").code(), "not_found");
        assert_eq!(AppError::conflict("exists").code(), "conflict");
        assert_eq!(
            AppError::range_not_satisfiable("outside resource length").code(),
            "range_not_satisfiable"
        );
        assert_eq!(
            AppError::too_many_requests("rate limited").code(),
            "too_many_requests"
        );
        assert_eq!(
            AppError::unprocessable("invalid input").code(),
            "unprocessable_entity"
        );
        assert_eq!(AppError::internal("boom").code(), "internal_server_error");
    }
}
