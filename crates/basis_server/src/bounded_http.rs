//! Re-export the one process-wide bounded Ergo node HTTP client.

pub use basis_store::ergo_scanner::node_http;

#[cfg(test)]
use basis_store::ergo_scanner::{BoundedHttpClient, BoundedHttpError};

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    async fn raw_server(response: Vec<u8>, delay: Duration) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = [0u8; 1024];
            let _ = socket.read(&mut request).await;
            tokio::time::sleep(delay).await;
            let _ = socket.write_all(&response).await;
        });
        format!("http://{address}")
    }

    async fn signaled_raw_server(
        response: Vec<u8>,
        delay: Duration,
    ) -> (String, tokio::sync::oneshot::Receiver<()>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let (accepted_tx, accepted_rx) = tokio::sync::oneshot::channel();
        tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let _ = accepted_tx.send(());
            let mut request = [0u8; 1024];
            let _ = socket.read(&mut request).await;
            tokio::time::sleep(delay).await;
            let _ = socket.write_all(&response).await;
        });
        (format!("http://{address}"), accepted_rx)
    }

    #[tokio::test]
    async fn chunked_body_is_stopped_at_the_configured_cap() {
        let payload = vec![b'x'; 65];
        let mut response =
            b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n".to_vec();
        response.extend_from_slice(format!("{:X}\r\n", payload.len()).as_bytes());
        response.extend_from_slice(&payload);
        response.extend_from_slice(b"\r\n0\r\n\r\n");
        let url = raw_server(response, Duration::ZERO).await;
        let client = BoundedHttpClient::new(Duration::from_secs(1), 64, 1).unwrap();

        assert_eq!(
            client.execute(client.get(&url)).await.unwrap_err(),
            BoundedHttpError::BodyTooLarge { limit: 64 }
        );
    }

    #[tokio::test]
    async fn declared_content_length_over_cap_is_rejected_before_body_read() {
        let response =
            b"HTTP/1.1 200 OK\r\nContent-Length: 65\r\nConnection: close\r\n\r\n".to_vec();
        let url = raw_server(response, Duration::ZERO).await;
        let client = BoundedHttpClient::new(Duration::from_secs(1), 64, 1).unwrap();

        assert_eq!(
            client.execute(client.get(&url)).await.unwrap_err(),
            BoundedHttpError::BodyTooLarge { limit: 64 }
        );
    }

    #[tokio::test]
    async fn stalled_request_hits_the_total_deadline() {
        let response = b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\n{}".to_vec();
        let url = raw_server(response, Duration::from_millis(200)).await;
        let client = BoundedHttpClient::new(Duration::from_millis(20), 64, 1).unwrap();

        assert_eq!(
            client.execute(client.get(&url)).await.unwrap_err(),
            BoundedHttpError::Timeout
        );
    }

    #[tokio::test]
    async fn concurrent_request_limit_rejects_without_queueing() {
        let client = BoundedHttpClient::new(Duration::from_secs(1), 64, 1).unwrap();
        let response = b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\n{}".to_vec();
        let (url, accepted) = signaled_raw_server(response, Duration::from_millis(100)).await;
        let first_client = client.clone();
        let first_url = url.clone();
        let first =
            tokio::spawn(async move { first_client.execute(first_client.get(&first_url)).await });
        accepted.await.unwrap();

        assert_eq!(
            client
                .execute(client.get("http://127.0.0.1:1"))
                .await
                .unwrap_err(),
            BoundedHttpError::Overloaded
        );
        assert!(first.await.unwrap().is_ok());
    }
}
