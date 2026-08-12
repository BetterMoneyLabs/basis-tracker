//! Bounded HTTP client for Ergo node requests.
//!
//! All outbound node traffic should go through [`bounded_client`] so connect
//! and total request timeouts apply uniformly, and through
//! [`read_body_capped`] / [`read_json_capped`] so a malformed or hostile node
//! cannot make the tracker buffer an unbounded response body.

use std::time::Duration;

/// Connect timeout for node requests.
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(3);
/// Total request timeout (connect + transfer).
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);
/// Maximum accepted response body size (2 MiB).
pub const MAX_BODY_BYTES: usize = 2 * 1024 * 1024;

/// Build a `reqwest::Client` with bounded connect and total timeouts.
pub fn bounded_client() -> reqwest::Client {
    reqwest::Client::builder()
        .connect_timeout(CONNECT_TIMEOUT)
        .timeout(REQUEST_TIMEOUT)
        .build()
        .expect("failed to build bounded HTTP client")
}

/// Read a response body as text, rejecting bodies over [`MAX_BODY_BYTES`].
///
/// The `Content-Length` header is checked first; the chunked read is capped as
/// a backstop for responses without a (truthful) length header.
pub async fn read_body_capped(mut response: reqwest::Response) -> Result<String, String> {
    if let Some(len) = response.content_length() {
        if len as usize > MAX_BODY_BYTES {
            return Err(format!(
                "response body too large: {len} bytes (cap {MAX_BODY_BYTES})"
            ));
        }
    }
    let mut buf: Vec<u8> = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|e| format!("read response body: {e}"))?
    {
        if buf.len() + chunk.len() > MAX_BODY_BYTES {
            return Err(format!("response body exceeds {MAX_BODY_BYTES} byte cap"));
        }
        buf.extend_from_slice(&chunk);
    }
    String::from_utf8(buf).map_err(|e| format!("response body not UTF-8: {e}"))
}

/// Read a response body and parse it as JSON, rejecting oversized bodies.
pub async fn read_json_capped(response: reqwest::Response) -> Result<serde_json::Value, String> {
    let body = read_body_capped(response).await?;
    serde_json::from_str(&body).map_err(|e| format!("parse response JSON: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bounded_client_timeouts() {
        // Builder must succeed; timeout values are the crate constants.
        let _client = bounded_client();
        assert_eq!(CONNECT_TIMEOUT, Duration::from_secs(3));
        assert_eq!(REQUEST_TIMEOUT, Duration::from_secs(15));
        assert_eq!(MAX_BODY_BYTES, 2 * 1024 * 1024);
    }

    #[tokio::test]
    async fn test_read_json_capped_parses_small_body() {
        // Spin a tiny one-shot HTTP server returning a small JSON body.
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            use tokio::io::{AsyncReadExt, AsyncWriteExt};
            let mut buf = vec![0u8; 1024];
            let _ = socket.read(&mut buf).await;
            let body = "{\"ok\":true}";
            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = socket.write_all(response.as_bytes()).await;
        });

        let client = bounded_client();
        let resp = client.get(format!("http://{addr}/x")).send().await.unwrap();
        let value = read_json_capped(resp).await.unwrap();
        assert_eq!(value["ok"], serde_json::json!(true));
    }

    #[tokio::test]
    async fn test_read_body_capped_rejects_oversized_body() {
        // Server declares an oversized Content-Length.
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            use tokio::io::{AsyncReadExt, AsyncWriteExt};
            let mut buf = vec![0u8; 1024];
            let _ = socket.read(&mut buf).await;
            let declared = MAX_BODY_BYTES + 1;
            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: text/plain\r\ncontent-length: {declared}\r\nconnection: close\r\n\r\n"
            );
            let _ = socket.write_all(response.as_bytes()).await;
        });

        let client = bounded_client();
        let resp = client.get(format!("http://{addr}/x")).send().await.unwrap();
        let err = read_body_capped(resp).await.unwrap_err();
        assert!(
            err.contains("too large"),
            "expected size-cap error, got: {err}"
        );
    }

    #[tokio::test]
    async fn test_read_body_capped_rejects_chunked_oversized_body() {
        // Server sends no Content-Length and streams chunks that exceed the cap.
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            use tokio::io::{AsyncReadExt, AsyncWriteExt};
            let mut buf = vec![0u8; 1024];
            let _ = socket.read(&mut buf).await;
            let headers = "HTTP/1.1 200 OK\r\ncontent-type: text/plain\r\ntransfer-encoding: chunked\r\nconnection: close\r\n\r\n";
            let _ = socket.write_all(headers.as_bytes()).await;
            // Send chunks of zeros until we are just over the cap.
            let chunk_size = 64 * 1024;
            let mut sent = 0usize;
            while sent <= MAX_BODY_BYTES {
                let chunk = vec![0u8; chunk_size];
                let line = format!("{:x}\r\n", chunk.len());
                let _ = socket.write_all(line.as_bytes()).await;
                let _ = socket.write_all(&chunk).await;
                let _ = socket.write_all(b"\r\n").await;
                sent += chunk.len();
            }
            let _ = socket.write_all(b"0\r\n\r\n").await;
        });

        let client = bounded_client();
        let resp = client.get(format!("http://{addr}/x")).send().await.unwrap();
        let err = read_body_capped(resp).await.unwrap_err();
        assert!(
            err.contains("exceeds") || err.contains("too large"),
            "expected chunked size-cap error, got: {err}"
        );
    }

    #[tokio::test]
    async fn test_read_json_capped_rejects_invalid_json() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            use tokio::io::{AsyncReadExt, AsyncWriteExt};
            let mut buf = vec![0u8; 1024];
            let _ = socket.read(&mut buf).await;
            let body = "not json";
            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                body.len(), body
            );
            let _ = socket.write_all(response.as_bytes()).await;
        });

        let client = bounded_client();
        let resp = client.get(format!("http://{addr}/x")).send().await.unwrap();
        let err = read_json_capped(resp).await.unwrap_err();
        assert!(
            err.contains("parse response JSON"),
            "expected JSON parse error, got: {err}"
        );
    }
}
