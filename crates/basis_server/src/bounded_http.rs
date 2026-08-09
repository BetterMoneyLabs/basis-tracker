//! Bounded outbound HTTP client for Ergo node calls.

use reqwest::{RequestBuilder, StatusCode};
use serde::de::DeserializeOwned;
use std::sync::{Arc, LazyLock};
use std::time::Duration;
use tokio::sync::Semaphore;

pub const NODE_HTTP_TIMEOUT: Duration = Duration::from_secs(15);
pub const NODE_HTTP_MAX_BODY_BYTES: usize = 2 * 1024 * 1024;
pub const NODE_HTTP_MAX_IN_FLIGHT: usize = 16;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum BoundedHttpError {
    #[error("failed to initialize bounded HTTP client: {0}")]
    ClientInitialization(String),
    #[error("outbound node request limit reached")]
    Overloaded,
    #[error("outbound node request timed out")]
    Timeout,
    #[error("outbound node request failed: {0}")]
    Request(String),
    #[error("outbound node response body exceeds {limit} bytes")]
    BodyTooLarge { limit: usize },
    #[error("failed to read outbound node response: {0}")]
    Body(String),
    #[error("outbound node response is not valid JSON: {0}")]
    Json(String),
}

fn map_reqwest_error(error: reqwest::Error) -> BoundedHttpError {
    if error.is_timeout() {
        BoundedHttpError::Timeout
    } else {
        BoundedHttpError::Request(error.to_string())
    }
}

#[derive(Debug)]
pub struct BoundedResponse {
    pub status: StatusCode,
    body: Vec<u8>,
}

impl BoundedResponse {
    pub fn status(&self) -> StatusCode {
        self.status
    }

    pub fn json<T: DeserializeOwned>(&self) -> Result<T, BoundedHttpError> {
        serde_json::from_slice(&self.body)
            .map_err(|error| BoundedHttpError::Json(error.to_string()))
    }

    pub fn text_lossy(&self) -> std::borrow::Cow<'_, str> {
        String::from_utf8_lossy(&self.body)
    }
}

#[derive(Clone)]
pub struct BoundedHttpClient {
    client: reqwest::Client,
    permits: Arc<Semaphore>,
    timeout: Duration,
    max_body_bytes: usize,
}

impl BoundedHttpClient {
    fn new(
        timeout: Duration,
        max_body_bytes: usize,
        max_in_flight: usize,
    ) -> Result<Self, BoundedHttpError> {
        if timeout.is_zero() || max_body_bytes == 0 || max_in_flight == 0 {
            return Err(BoundedHttpError::ClientInitialization(
                "timeout, body cap, and concurrency limit must be non-zero".to_string(),
            ));
        }
        let client = reqwest::Client::builder()
            .connect_timeout(timeout.min(Duration::from_secs(3)))
            .timeout(timeout)
            .pool_max_idle_per_host(max_in_flight)
            .build()
            .map_err(|error| BoundedHttpError::ClientInitialization(error.to_string()))?;
        Ok(Self {
            client,
            permits: Arc::new(Semaphore::new(max_in_flight)),
            timeout,
            max_body_bytes,
        })
    }

    pub fn get(&self, url: &str) -> RequestBuilder {
        self.client.get(url)
    }

    pub fn post(&self, url: &str) -> RequestBuilder {
        self.client.post(url)
    }

    pub async fn execute(
        &self,
        request: RequestBuilder,
    ) -> Result<BoundedResponse, BoundedHttpError> {
        let _permit = self
            .permits
            .try_acquire()
            .map_err(|_| BoundedHttpError::Overloaded)?;
        let max_body_bytes = self.max_body_bytes;

        tokio::time::timeout(self.timeout, async move {
            let mut response = request.send().await.map_err(map_reqwest_error)?;
            if response
                .content_length()
                .is_some_and(|length| length > max_body_bytes as u64)
            {
                return Err(BoundedHttpError::BodyTooLarge {
                    limit: max_body_bytes,
                });
            }

            let status = response.status();
            let mut body = Vec::with_capacity(
                response
                    .content_length()
                    .unwrap_or(0)
                    .min(max_body_bytes as u64) as usize,
            );
            while let Some(chunk) = response.chunk().await.map_err(|error| {
                if error.is_timeout() {
                    BoundedHttpError::Timeout
                } else {
                    BoundedHttpError::Body(error.to_string())
                }
            })? {
                let new_len = body
                    .len()
                    .checked_add(chunk.len())
                    .filter(|length| *length <= max_body_bytes)
                    .ok_or(BoundedHttpError::BodyTooLarge {
                        limit: max_body_bytes,
                    })?;
                body.reserve(new_len.saturating_sub(body.capacity()));
                body.extend_from_slice(&chunk);
            }
            Ok(BoundedResponse { status, body })
        })
        .await
        .map_err(|_| BoundedHttpError::Timeout)?
    }
}

static NODE_HTTP_CLIENT: LazyLock<Result<BoundedHttpClient, BoundedHttpError>> =
    LazyLock::new(|| {
        BoundedHttpClient::new(
            NODE_HTTP_TIMEOUT,
            NODE_HTTP_MAX_BODY_BYTES,
            NODE_HTTP_MAX_IN_FLIGHT,
        )
    });

pub fn node_http() -> Result<&'static BoundedHttpClient, BoundedHttpError> {
    NODE_HTTP_CLIENT
        .as_ref()
        .map_err(|error| BoundedHttpError::ClientInitialization(error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
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
        let _occupied = client.permits.try_acquire().unwrap();

        assert_eq!(
            client
                .execute(client.get("http://127.0.0.1:1"))
                .await
                .unwrap_err(),
            BoundedHttpError::Overloaded
        );
    }
}
