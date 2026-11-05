use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

use sqlx::postgres::PgPoolOptions;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn first_party_shell_works_over_a_real_loopback_tcp_socket() {
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .connect_lazy("postgres://unused:unused@127.0.0.1:1/unused")
        .expect("syntactically valid lazy postgres URL");
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .expect("bind loopback listener");
    let address = listener.local_addr().expect("listener address");

    let server = tokio::spawn(async move {
        axum::serve(listener, niccolo_evidence::api::router(pool))
            .await
            .expect("serve loopback router");
    });

    let response = tokio::task::spawn_blocking(move || {
        let mut stream = TcpStream::connect_timeout(&address, Duration::from_secs(2))
            .expect("connect to loopback listener");
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .expect("set read timeout");
        stream
            .write_all(b"GET / HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n")
            .expect("write HTTP request");
        let mut bytes = Vec::new();
        stream.read_to_end(&mut bytes).expect("read HTTP response");
        String::from_utf8(bytes).expect("UTF-8 HTTP response")
    })
    .await
    .expect("blocking client task");

    server.abort();

    assert!(response.starts_with("HTTP/1.1 200 OK"), "{response}");
    assert!(response.contains("content-type: text/html"), "{response}");
    assert!(response.contains("cache-control: no-store"), "{response}");
    assert!(response.contains("content-security-policy:"), "{response}");
    assert!(response.contains("I am about to say or do something"));
    assert!(response.contains("I am trying to understand something"));
    assert!(response.contains("Help me figure out what matters"));
    assert!(response.contains("does not upload or claim to analyze them"));
}
