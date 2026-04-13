use std::sync::Arc;

use axum::body::Body;
use axum::extract::State;
use axum::http::{HeaderMap, Request, StatusCode, header};
use axum::middleware::Next;
use axum::response::IntoResponse;
use axum::response::Response;

use crate::AppState;

fn extract_api_key(headers: &HeaderMap) -> Option<String> {
    if let Some(value) = headers.get(header::AUTHORIZATION) {
        if let Ok(s) = value.to_str() {
            let s = s.trim();
            if let Some(rest) = s.strip_prefix("Bearer ") {
                let key = rest.trim();
                if !key.is_empty() {
                    return Some(key.to_string());
                }
            }
        }
    }

    if let Some(value) = headers.get("x-api-key") {
        if let Ok(s) = value.to_str() {
            let key = s.trim();
            if !key.is_empty() {
                return Some(key.to_string());
            }
        }
    }

    None
}

pub async fn require_api_key(
    State(state): State<Arc<AppState>>,
    req: Request<Body>,
    next: Next,
) -> Response {
    let Some(expected) = state.api_key.as_deref() else {
        return next.run(req).await;
    };

    let actual = extract_api_key(req.headers());
    if actual.as_deref() == Some(expected) {
        return next.run(req).await;
    }

    let body = axum::Json(serde_json::json!({
        "error_code": "unauthorized",
        "message": "missing or invalid api key",
    }));
    (StatusCode::UNAUTHORIZED, body).into_response()
}

#[cfg(test)]
mod tests {
    use super::extract_api_key;
    use axum::http::HeaderMap;

    #[test]
    fn extracts_bearer() {
        let mut headers = HeaderMap::new();
        headers.insert("authorization", "Bearer k1".parse().expect("hdr"));
        assert_eq!(extract_api_key(&headers).as_deref(), Some("k1"));
    }

    #[test]
    fn extracts_x_api_key() {
        let mut headers = HeaderMap::new();
        headers.insert("x-api-key", "k2".parse().expect("hdr"));
        assert_eq!(extract_api_key(&headers).as_deref(), Some("k2"));
    }
}
