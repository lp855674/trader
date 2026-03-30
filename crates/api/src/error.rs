use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;

#[derive(Debug)]
pub struct ApiError {
    pub status: StatusCode,
    pub code: &'static str,
    pub message: String,
}

impl ApiError {
    pub fn internal(err: db::DbError) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            code: "internal",
            message: err.to_string(),
        }
    }

    pub fn bad_request(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            code: "bad_request",
            message: message.into(),
        }
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            code: "not_found",
            message: message.into(),
        }
    }

    pub fn pipeline(err: pipeline::PipelineError) -> Self {
        match err {
            pipeline::PipelineError::Exec(exec::ExecError::NotConfigured) => Self {
                status: StatusCode::BAD_REQUEST,
                code: exec::ExecError::ERROR_CODE_NOT_CONFIGURED,
                message: "execution adapter not configured (paper/live profile missing)".to_string(),
            },
            pipeline::PipelineError::Exec(exec::ExecError::Longbridge(msg)) => Self {
                status: StatusCode::BAD_GATEWAY,
                code: "broker_error",
                message: msg,
            },
            pipeline::PipelineError::RiskDenied(msg) => Self {
                status: StatusCode::FORBIDDEN,
                code: "risk_denied",
                message: msg,
            },
            other => Self {
                status: StatusCode::INTERNAL_SERVER_ERROR,
                code: "pipeline_error",
                message: other.to_string(),
            },
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        let body = Json(serde_json::json!({
            "error_code": self.code,
            "message": self.message,
        }));
        (self.status, body).into_response()
    }
}
