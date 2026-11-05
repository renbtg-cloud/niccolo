#![cfg(feature = "pg-integration")]
use niccolo_evidence::{api, authz};
use sqlx::postgres::PgPoolOptions;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};
use uuid::Uuid;

async fn raw_http(port: u16, method: &str, path: &str, token: Option<&str>, body: &str) -> String {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
    let auth = token
        .map(|t| format!("Authorization: Bearer {t}\r\n"))
        .unwrap_or_default();
    let content = if body.is_empty() {
        String::new()
    } else {
        "Content-Type: application/json\r\n".to_owned()
    };
    let req = format!(
        "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1\r\n{auth}{content}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(req.as_bytes()).await.unwrap();
    let mut out = Vec::new();
    stream.read_to_end(&mut out).await.unwrap();
    String::from_utf8(out).unwrap()
}
#[tokio::test]
async fn real_loopback_web_and_persistent_situation_smoke() {
    let admin = PgPoolOptions::new()
        .connect(&std::env::var("NICCOLO_PG_ADMIN_URL").unwrap())
        .await
        .unwrap();
    let secure = PgPoolOptions::new()
        .max_connections(16)
        .connect(&std::env::var("NICCOLO_PG_SECURE_URL").unwrap())
        .await
        .unwrap();
    let w = Uuid::new_v4();
    sqlx::query("INSERT INTO niccolo.workspaces(workspace_id,label) VALUES($1,'web-e2e')")
        .bind(w)
        .execute(&admin)
        .await
        .unwrap();
    let token = authz::provision_token(&admin, Uuid::new_v4(), w, true, true, 1)
        .await
        .unwrap();
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .unwrap();
    let port = listener.local_addr().unwrap().port();
    let server =
        tokio::spawn(async move { axum::serve(listener, api::router(secure)).await.unwrap() });
    let root = raw_http(port, "GET", "/", None, "").await;
    assert!(root.contains("200 OK"));
    assert!(root.contains("Persistent Situation"));
    let js = raw_http(port, "GET", "/api.js", None, "").await;
    assert!(js.contains("200 OK"));
    assert!(js.contains("addPersistentContribution"));
    let s = Uuid::new_v4();
    let base = format!("/v1/workspaces/{w}/situations/{s}");
    let created = raw_http(
        port,
        "POST",
        &base,
        Some(&token),
        r#"{"focus":"Persistent focus"}"#,
    )
    .await;
    assert!(created.contains("201 Created"));
    assert!(created.contains("Persistent focus"));
    let c = Uuid::new_v4();
    let added = raw_http(
        port,
        "POST",
        &format!("{base}/contributions/{c}"),
        Some(&token),
        r#"{"kind":"hypothetical","text":"WHAT IF the message arrived later?"}"#,
    )
    .await;
    assert!(added.contains("201 Created"));
    assert!(added.contains("HYPOTHETICAL"));
    let current = raw_http(port, "GET", &base, Some(&token), "").await;
    assert!(current.contains("200 OK"));
    assert!(current.contains("WHAT IF the message arrived later?"));
    server.abort();
}
