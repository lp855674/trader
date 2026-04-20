use std::time::Duration;

use axum::Router;
use axum::extract::ws::{Message, WebSocketUpgrade};
use axum::response::IntoResponse;
use axum::routing::get;
use tokio::net::TcpListener;

async fn ws_handler(ws: WebSocketUpgrade) -> impl IntoResponse {
    ws.on_upgrade(|mut socket| async move {
        let hello = serde_json::json!({
            "kind": "hello",
            "schema_version": 1u32,
        });
        socket
            .send(Message::Text(serde_json::to_string(&hello).expect("hello json")))
            .await
            .expect("send hello");

        tokio::time::sleep(Duration::from_millis(100)).await;

        let event = serde_json::json!({
            "event_id": "evt-1",
            "kind": "error",
            "error_code": "smoke",
            "message": "alive",
        });
        socket
            .send(Message::Text(serde_json::to_string(&event).expect("event json")))
            .await
            .expect("send event");

        tokio::time::sleep(Duration::from_millis(300)).await;
    })
}

#[tokio::test]
async fn stream_client_reads_hello_and_followup_event() {
    let app = Router::new().route("/v1/stream", get(ws_handler));
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("addr");
    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.expect("serve");
    });

    let client = terminal_client::QuantdStreamClient::new(format!("http://{addr}"), None);
    let mut stream = client.connect().await.expect("connect");

    let hello = terminal_client::QuantdStreamClient::next_message(&mut stream)
        .await
        .expect("hello ok")
        .expect("hello exists");
    assert_eq!(
        hello,
        terminal_core::models::StreamMessage::Hello { schema_version: 1 }
    );

    let event = terminal_client::QuantdStreamClient::next_message(&mut stream)
        .await
        .expect("event ok")
        .expect("event exists");
    assert_eq!(
        event,
        terminal_core::models::StreamMessage::Error {
            error_code: "smoke".to_string(),
            message: "alive".to_string(),
        }
    );

    server.abort();
}
