use api::{AppState, router_with_state};
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use std::path::PathBuf;
use storage::Db;
use tower::ServiceExt;

#[tokio::test]
async fn post_backtest_returns_created() {
    std::env::set_current_dir(workspace_root()).unwrap();
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let app = router_with_state(AppState::new(db, "configs/backtest/ma_cross.toml".into()));

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/backtests")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CREATED);
}

#[tokio::test]
async fn post_backtest_populates_query_routes() {
    std::env::set_current_dir(workspace_root()).unwrap();
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let app = router_with_state(AppState::new(db, "configs/backtest/ma_cross.toml".into()));

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/backtests")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);

    for uri in [
        "/api/v1/fills",
        "/api/v1/account-balances",
        "/api/v1/portfolio/snapshots",
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(uri)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        assert_ne!(bytes.as_ref(), b"[]");
    }

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/metrics")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    assert!(
        bytes
            .as_ref()
            .windows("total_return".len())
            .any(|window| window == b"total_return")
    );
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|path| path.parent())
        .unwrap()
        .to_path_buf()
}
