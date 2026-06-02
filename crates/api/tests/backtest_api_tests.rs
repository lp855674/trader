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
}

#[tokio::test]
async fn post_replay_returns_created_and_persists_events() {
    std::env::set_current_dir(workspace_root()).unwrap();
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let app = router_with_state(AppState::new(db, "configs/backtest/ma_cross.toml".into()));

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/replays")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CREATED);
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    assert!(
        bytes
            .as_ref()
            .windows("\"bars\"".len())
            .any(|window| window == b"\"bars\"")
    );

    for uri in ["/api/v1/events", "/api/v1/runs/sample-ma-cross/events"] {
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
        assert!(
            bytes
                .as_ref()
                .windows("replay.completed".len())
                .any(|window| window == b"replay.completed")
        );
    }
}

#[tokio::test]
async fn post_paper_run_returns_accepted_run_start() {
    std::env::set_current_dir(workspace_root()).unwrap();
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let app = router_with_state(AppState::new(db, "configs/backtest/ma_cross.toml".into()));

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/paper-runs")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::ACCEPTED);
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    assert!(
        bytes
            .as_ref()
            .windows("\"status\":\"running\"".len())
            .any(|window| window == b"\"status\":\"running\"")
    );
}

#[tokio::test]
async fn post_paper_run_populates_query_routes() {
    std::env::set_current_dir(workspace_root()).unwrap();
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let app = router_with_state(AppState::new(db, "configs/backtest/ma_cross.toml".into()));

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/paper-runs")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::ACCEPTED);
    wait_for_status(app.clone(), "sample-ma-cross", "completed").await;

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
        .clone()
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

    for uri in ["/api/v1/runs", "/api/v1/runs/sample-ma-cross"] {
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
}

#[tokio::test]
async fn completed_paper_run_status_is_preserved_after_late_cancel() {
    std::env::set_current_dir(workspace_root()).unwrap();
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let app = router_with_state(AppState::new(db, "configs/backtest/ma_cross.toml".into()));

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/paper-runs")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::ACCEPTED);
    wait_for_status(app.clone(), "sample-ma-cross", "completed").await;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/runs/sample-ma-cross/status")
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
            .windows("completed".len())
            .any(|window| window == b"completed")
    );

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/runs/sample-ma-cross/cancel")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/runs/sample-ma-cross/status")
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
            .windows("completed".len())
            .any(|window| window == b"completed")
    );
}

#[tokio::test]
async fn failed_paper_run_records_failed_status_and_error() {
    std::env::set_current_dir(workspace_root()).unwrap();
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let app = router_with_state(AppState::new(
        db,
        "configs/backtest/missing-bars.toml".into(),
    ));

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/paper-runs")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/runs/sample-missing-bars/status")
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
            .windows("failed".len())
            .any(|window| window == b"failed")
    );
    assert!(
        bytes
            .as_ref()
            .windows("\"error\":null".len())
            .all(|window| window != b"\"error\":null")
    );
}

#[tokio::test]
async fn active_paper_run_can_be_cancelled() {
    std::env::set_current_dir(workspace_root()).unwrap();
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();
    let app = router_with_state(AppState::new(db, "configs/backtest/slow-paper.toml".into()));

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/paper-runs")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::ACCEPTED);

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/runs/sample-slow-paper/cancel")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    assert_status(app.clone(), "sample-slow-paper", "cancelled").await;
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|path| path.parent())
        .unwrap()
        .to_path_buf()
}

async fn assert_status(app: axum::Router, run_id: &str, expected_status: &str) {
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/api/v1/runs/{run_id}/status"))
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
            .windows(expected_status.len())
            .any(|window| window == expected_status.as_bytes())
    );
}

async fn wait_for_status(app: axum::Router, run_id: &str, expected_status: &str) {
    for _ in 0..50 {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!("/api/v1/runs/{run_id}/status"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        if response.status() == StatusCode::OK {
            let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
            if bytes
                .as_ref()
                .windows(expected_status.len())
                .any(|window| window == expected_status.as_bytes())
            {
                return;
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    panic!("run {run_id} did not reach {expected_status}");
}
