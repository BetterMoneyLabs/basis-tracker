//! HTTP client for Ergo node communication using std::net::TcpStream

use std::time::Duration;

use serde_json::Value;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum HttpClientError {
    #[error("HTTP error: {0}")]
    HttpError(String),
    #[error("JSON parsing error: {0}")]
    JsonError(String),
    #[error("Network error: {0}")]
    NetworkError(String),
    #[error("Node API error: {0}")]
    NodeApiError(String),
}

/// HTTP client for Ergo node communication
pub struct SimpleHttpClient {
    base_url: String,
    api_key: String,
    timeout_secs: u64,
}

impl SimpleHttpClient {
    pub fn new(base_url: String, api_key: String, timeout_secs: u64) -> Self {
        Self {
            base_url,
            api_key,
            timeout_secs,
        }
    }

    /// Make a GET request to the Ergo node
    pub async fn get(&self, endpoint: &str) -> Result<Value, HttpClientError> {
        let url = format!("{}{}", self.base_url, endpoint);
        
        // Parse URL components
        let url_parts: Vec<&str> = url.splitn(2, "://").collect();
        if url_parts.len() != 2 {
            return Err(HttpClientError::HttpError("Invalid URL format".to_string()));
        }
        
        let protocol = url_parts[0];
        let rest = url_parts[1];
        
        let host_path: Vec<&str> = rest.splitn(2, '/').collect();
        let host = host_path[0];
        let path = if host_path.len() > 1 { host_path[1] } else { "" };
        
        // For HTTP protocol, connect to port 80
        let port = if protocol == "https" { 443 } else { 80 };
        
        // Create TCP connection
        let mut stream = std::net::TcpStream::connect(format!("{}:{}", host, port))
            .map_err(|e| HttpClientError::NetworkError(e.to_string()))?;
        
        // Set timeout
        stream.set_read_timeout(Some(Duration::from_secs(self.timeout_secs)))
            .map_err(|e| HttpClientError::NetworkError(e.to_string()))?;
        
        // Create HTTP request
        let mut request = format!(
            "GET /{} HTTP/1.1\r\nHost: {}\r\nUser-Agent: BasisTracker/1.0\r\nAccept: application/json\r\n",
            path, host
        );
        
        // Add API key header if provided
        if !self.api_key.is_empty() {
            request.push_str(&format!("api_key: {}\r\n", self.api_key));
        }
        
        request.push_str("Connection: close\r\n\r\n");
        
        // Send request
        std::io::Write::write_all(&mut stream, request.as_bytes())
            .map_err(|e| HttpClientError::NetworkError(e.to_string()))?;
        
        // Read response
        let mut response = String::new();
        std::io::Read::read_to_string(&mut stream, &mut response)
            .map_err(|e| HttpClientError::NetworkError(e.to_string()))?;
        
        // Parse HTTP response (simple parsing)
        let response_parts: Vec<&str> = response.splitn(2, "\r\n\r\n").collect();
        if response_parts.len() != 2 {
            return Err(HttpClientError::HttpError("Invalid HTTP response".to_string()));
        }
        
        let body = response_parts[1].trim();
        
        // Parse JSON response
        let json: Value = serde_json::from_str(body)
            .map_err(|e| HttpClientError::JsonError(e.to_string()))?;
        
        Ok(json)
    }

    /// Make a POST request to the Ergo node
    pub async fn post(&self, endpoint: &str, body: Value) -> Result<Value, HttpClientError> {
        let url = format!("{}{}", self.base_url, endpoint);
        
        // Parse URL components
        let url_parts: Vec<&str> = url.splitn(2, "://").collect();
        if url_parts.len() != 2 {
            return Err(HttpClientError::HttpError("Invalid URL format".to_string()));
        }
        
        let protocol = url_parts[0];
        let rest = url_parts[1];
        
        let host_path: Vec<&str> = rest.splitn(2, '/').collect();
        let host = host_path[0];
        let path = if host_path.len() > 1 { host_path[1] } else { "" };
        
        // For HTTP protocol, connect to port 80
        let port = if protocol == "https" { 443 } else { 80 };
        
        // Create TCP connection
        let mut stream = std::net::TcpStream::connect(format!("{}:{}", host, port))
            .map_err(|e| HttpClientError::NetworkError(e.to_string()))?;
        
        // Set timeout
        stream.set_read_timeout(Some(Duration::from_secs(self.timeout_secs)))
            .map_err(|e| HttpClientError::NetworkError(e.to_string()))?;
        
        // Convert body to JSON string
        let body_str = serde_json::to_string(&body)
            .map_err(|e| HttpClientError::JsonError(e.to_string()))?;
        
        // Create HTTP request
        let mut request = format!(
            "POST /{} HTTP/1.1\r\nHost: {}\r\nUser-Agent: BasisTracker/1.0\r\nContent-Type: application/json\r\nContent-Length: {}\r\n",
            path, host, body_str.len()
        );
        
        // Add API key header if provided
        if !self.api_key.is_empty() {
            request.push_str(&format!("api_key: {}\r\n", self.api_key));
        }
        
        request.push_str("Connection: close\r\n\r\n");
        request.push_str(&body_str);
        
        // Send request
        std::io::Write::write_all(&mut stream, request.as_bytes())
            .map_err(|e| HttpClientError::NetworkError(e.to_string()))?;
        
        // Read response
        let mut response = String::new();
        std::io::Read::read_to_string(&mut stream, &mut response)
            .map_err(|e| HttpClientError::NetworkError(e.to_string()))?;
        
        // Parse HTTP response (simple parsing)
        let response_parts: Vec<&str> = response.splitn(2, "\r\n\r\n").collect();
        if response_parts.len() != 2 {
            return Err(HttpClientError::HttpError("Invalid HTTP response".to_string()));
        }
        
        let body = response_parts[1].trim();
        
        // Parse JSON response
        let json: Value = serde_json::from_str(body)
            .map_err(|e| HttpClientError::JsonError(e.to_string()))?;
        
        Ok(json)
    }

    /// Get node info
    pub async fn get_node_info(&self) -> Result<Value, HttpClientError> {
        self.get("/info").await
    }

    /// Get block headers at specific height
    pub async fn get_blocks_at_height(&self, height: u64) -> Result<Value, HttpClientError> {
        self.get(&format!("/blocks/at/{}", height)).await
    }

    /// Get block by ID
    pub async fn get_block_by_id(&self, block_id: &str) -> Result<Value, HttpClientError> {
        self.get(&format!("/blocks/{}", block_id)).await
    }

    /// Get block transactions by block ID
    pub async fn get_block_transactions(&self, block_id: &str) -> Result<Value, HttpClientError> {
        self.get(&format!("/blocks/{}/transactions", block_id)).await
    }

    /// Get unspent boxes by ErgoTree template
    pub async fn get_unspent_boxes_by_ergo_tree(&self, ergo_tree: &str) -> Result<Value, HttpClientError> {
        let body = serde_json::json!({
            "ergoTree": ergo_tree
        });
        self.post("/blockchain/box/unspent/byErgoTree", body).await
    }

    /// Get box by ID
    pub async fn get_box_by_id(&self, box_id: &str) -> Result<Value, HttpClientError> {
        self.get(&format!("/blockchain/box/byId/{}", box_id)).await
    }

    /// Get transaction by ID
    pub async fn get_transaction_by_id(&self, tx_id: &str) -> Result<Value, HttpClientError> {
        self.get(&format!("/blockchain/transaction/byId/{}", tx_id)).await
    }

    /// Get unconfirmed transactions
    pub async fn get_unconfirmed_transactions(&self) -> Result<Value, HttpClientError> {
        self.get("/blockchain/transactions/unconfirmed").await
    }

    /// Get UTXO set size
    pub async fn get_utxo_size(&self) -> Result<Value, HttpClientError> {
        self.get("/blockchain/utxo/size").await
    }

    /// Get current blockchain height
    pub async fn get_current_height(&self) -> Result<u64, HttpClientError> {
        let info = self.get_node_info().await?;
        
        if let Some(full_height) = info["fullHeight"].as_u64() {
            Ok(full_height)
        } else {
            Err(HttpClientError::NodeApiError(
                "Failed to parse fullHeight from node info".to_string()
            ))
        }
    }

    /// Get block headers for a range of heights
    pub async fn get_block_headers_range(&self, from: u64, to: u64) -> Result<Vec<Value>, HttpClientError> {
        let mut headers = Vec::new();
        
        for height in from..=to {
            if let Ok(blocks) = self.get_blocks_at_height(height).await {
                if let Some(blocks_array) = blocks.as_array() {
                    for block in blocks_array {
                        headers.push(block.clone());
                    }
                }
            }
        }
        
        Ok(headers)
    }

    /// Search for boxes with specific criteria
    pub async fn search_boxes(&self, criteria: Value) -> Result<Value, HttpClientError> {
        self.post("/blockchain/box/search", criteria).await
    }
}