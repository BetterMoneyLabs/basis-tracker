//! Ergo blockchain scanner for monitoring Basis reserve contracts
//! This module provides blockchain integration using /blockchain endpoints (no node scans).

use std::{
    collections::HashSet,
    path::Path,
    sync::{Arc, LazyLock},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::sync::{Mutex, Semaphore};

use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::{debug, error, info, warn};

use reqwest::{Client, RequestBuilder, StatusCode};
use serde::de::DeserializeOwned;

pub(crate) const SCAN_PAGE_SIZE: usize = 100;
pub(crate) const MAX_SCAN_PAGES: usize = 1_024;
pub(crate) const MAX_SCAN_BOXES: usize = 100_000;
#[cfg(test)]
pub(crate) const MAX_RESPONSE_BODY_BYTES: usize = NODE_HTTP_MAX_BODY_BYTES;
pub(crate) const MAX_CONCURRENT_SCANNER_REQUESTS: usize = 4;
const MAX_ERROR_BODY_CHARS: usize = 1_024;

/// Total deadline applied to every outbound Ergo node request in this process.
pub const NODE_HTTP_TIMEOUT: Duration = Duration::from_secs(15);
/// Maximum response body accepted from the Ergo node.
pub const NODE_HTTP_MAX_BODY_BYTES: usize = 2 * 1024 * 1024;
/// Maximum number of concurrent Ergo node requests across scanners and server.
pub const NODE_HTTP_MAX_IN_FLIGHT: usize = 16;

/// Failure returned by the process-wide bounded Ergo node HTTP client.
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

/// Fully buffered response whose body has already passed the configured cap.
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

    pub(crate) fn into_parts(self) -> (StatusCode, Vec<u8>) {
        (self.status, self.body)
    }
}

/// HTTP executor that applies one admission budget, total deadline, and body cap.
#[derive(Clone)]
pub struct BoundedHttpClient {
    client: reqwest::Client,
    permits: Arc<Semaphore>,
    timeout: Duration,
    max_body_bytes: usize,
}

impl BoundedHttpClient {
    pub fn new(
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
        self.request(reqwest::Method::GET, url)
    }

    pub fn post(&self, url: &str) -> RequestBuilder {
        self.request(reqwest::Method::POST, url)
    }

    pub fn request(&self, method: reqwest::Method, url: &str) -> RequestBuilder {
        self.client.request(method, url)
    }

    /// Execute a request through this client's shared policy.
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

/// Return the one process-wide client used by server and scanner node calls.
pub fn node_http() -> Result<&'static BoundedHttpClient, BoundedHttpError> {
    NODE_HTTP_CLIENT
        .as_ref()
        .map_err(|error| BoundedHttpError::ClientInitialization(error.to_string()))
}

pub(crate) fn summarize_error_body(body: &[u8]) -> String {
    String::from_utf8_lossy(body)
        .chars()
        .take(MAX_ERROR_BODY_CHARS)
        .collect()
}

/// Response from `POST /blockchain/box/unspent/byAddress` is a JSON array of IndexedErgoBox.
pub(crate) type ByAddressResponse = Vec<IndexedErgoBox>;

/// Box representation returned by Ergo `/blockchain/*` endpoints (IndexedErgoBox).
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct IndexedErgoBox {
    #[serde(rename = "boxId")]
    pub(crate) box_id: String,
    value: u64,
    #[serde(rename = "ergoTree")]
    ergo_tree: String,
    #[serde(rename = "creationHeight")]
    creation_height: u64,
    #[serde(rename = "transactionId")]
    transaction_id: String,
    #[serde(rename = "additionalRegisters")]
    additional_registers: std::collections::HashMap<String, String>,
    #[serde(default)]
    assets: Vec<IndexedBoxAsset>,
    #[serde(flatten)]
    _extra: std::collections::HashMap<String, serde_json::Value>,
}

/// Asset representation in an IndexedErgoBox.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct IndexedBoxAsset {
    #[serde(rename = "tokenId")]
    token_id: String,
    amount: u64,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
pub(crate) struct IndexedHeightResponse {
    #[serde(rename = "indexedHeight")]
    pub(crate) indexed_height: u64,
    #[serde(rename = "fullHeight")]
    pub(crate) full_height: u64,
}

impl From<IndexedErgoBox> for ScanBox {
    fn from(box_: IndexedErgoBox) -> Self {
        Self {
            box_id: box_.box_id,
            value: box_.value,
            ergo_tree: box_.ergo_tree,
            creation_height: box_.creation_height,
            transaction_id: box_.transaction_id,
            additional_registers: box_.additional_registers,
            assets: box_
                .assets
                .into_iter()
                .map(|a| BoxAsset {
                    token_id: a.token_id,
                    amount: a.amount,
                })
                .collect(),
        }
    }
}

use crate::{
    persistence::{ReserveStorage, ScannerMetadataStorage},
    ExtendedReserveInfo, ReserveTracker,
};

#[derive(Error, Debug)]
pub enum ScannerError {
    #[error("Scanner error: {0}")]
    Generic(String),
    #[error("Store error: {0}")]
    StoreError(String),
    #[error("Node error: {0}")]
    NodeError(String),
    #[error("Network error: {0}")]
    NetworkError(String),
    #[error("Box error: {0}")]
    BoxError(String),
    #[error("Invalid transaction {0}")]
    InvalidTransaction(String),
    #[error("Reserve box validation failed at TX id: {0}")]
    InvalidReserveBox(String),
    #[error("HTTP error: {0}")]
    HttpError(String),
    #[error("JSON parse error: {0}")]
    JsonError(String),
    #[error("Scanner response exceeds {max_bytes} bytes")]
    ResponseTooLarge { max_bytes: usize },
    #[error("Scanner request concurrency gate is closed")]
    RequestGateClosed,
    #[error("Scanner request capacity is exhausted")]
    RequestCapacityExceeded,
    #[error("Indexed node is behind: indexed height {indexed_height}, full height {full_height}")]
    IndexLag {
        indexed_height: u64,
        full_height: u64,
    },
    #[error("Incoherent scanner snapshot: {0}")]
    IncoherentSnapshot(String),
    #[error("Scanner resource limit exceeded: {0}")]
    ScanLimitExceeded(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScanType {
    Reserves,
}

impl ScanType {
    pub fn to_str(&self) -> &'static str {
        match self {
            ScanType::Reserves => "reserves",
        }
    }
}

/// Configuration for scanner
#[derive(Clone, Serialize, Deserialize)]
pub struct NodeConfig {
    /// Starting block height for scanning
    pub start_height: Option<u64>,
    /// Basis reserve contract P2S address (optional)
    pub reserve_contract_p2s: Option<String>,
    /// Ergo node URL
    pub node_url: String,
    /// Scan registration name
    pub scan_name: Option<String>,
    /// API key for Ergo node authentication
    pub api_key: Option<String>,
}

impl std::fmt::Debug for NodeConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NodeConfig")
            .field("start_height", &self.start_height)
            .field("reserve_contract_p2s", &self.reserve_contract_p2s)
            .field("node_url", &self.node_url)
            .field("scan_name", &self.scan_name)
            .field("api_key", &self.api_key.as_ref().map(|_| "<redacted>"))
            .finish()
    }
}

/// Inner state for scanner that requires synchronization
#[derive(Clone)]
pub struct ServerStateInner {
    pub current_height: u64,
    pub last_scanned_height: u64,
    pub scan_active: bool,
}

/// Server state for scanner
/// Uses real blockchain integration with proper synchronization
#[derive(Clone)]
pub struct ServerState {
    pub config: NodeConfig,
    pub inner: Arc<Mutex<ServerStateInner>>,
    pub client: Client,
    pub(crate) request_permits: Arc<Semaphore>,
    pub reserve_tracker: ReserveTracker,
    pub metadata_storage: ScannerMetadataStorage,
    pub reserve_storage: ReserveStorage,
}

impl ServerState {
    async fn request_bytes(
        &self,
        request: reqwest::RequestBuilder,
        context: &str,
    ) -> Result<(StatusCode, Vec<u8>), ScannerError> {
        let _permit = self
            .request_permits
            .try_acquire()
            .map_err(|error| match error {
                tokio::sync::TryAcquireError::Closed => ScannerError::RequestGateClosed,
                tokio::sync::TryAcquireError::NoPermits => ScannerError::RequestCapacityExceeded,
            })?;
        let response = node_http()
            .map_err(|error| ScannerError::HttpError(format!("{}: {}", context, error)))?
            .execute(request)
            .await
            .map_err(|error| match error {
                BoundedHttpError::BodyTooLarge { limit } => {
                    ScannerError::ResponseTooLarge { max_bytes: limit }
                }
                BoundedHttpError::Overloaded => ScannerError::RequestCapacityExceeded,
                other => ScannerError::HttpError(format!("{}: {}", context, other)),
            })?;
        Ok(response.into_parts())
    }

    /// Create HTTP request builder with API key header if configured
    fn request_builder(
        &self,
        method: reqwest::Method,
        url: &str,
    ) -> Result<reqwest::RequestBuilder, ScannerError> {
        debug!("Request method: {}, URL: {}", method, url);

        let client = node_http().map_err(|error| ScannerError::HttpError(error.to_string()))?;
        let mut request = client.request(method, url);

        // Add API key header if configured
        if let Some(api_key) = &self.config.api_key {
            debug!("Using configured API key for request to: {}", url);
            request = request.header("api_key", api_key);
        } else {
            debug!("No API key configured for request to: {}", url);
            info!("No API key header added to HTTP request");
        }

        Ok(request)
    }

    /// Create a server state that uses real Ergo scanner
    pub fn new(config: NodeConfig, data_dir: impl AsRef<Path>) -> Result<Self, ScannerError> {
        let configured = config.reserve_contract_p2s.as_deref().ok_or_else(|| {
            ScannerError::Generic(
                "reserve scanner contract generation is required before storage can be opened"
                    .to_string(),
            )
        })?;
        let historical = crate::contract_compiler::get_basis_reserve_contract_p2s()
            .map_err(|error| ScannerError::Generic(error.to_string()))?;
        if configured != historical {
            return Err(ScannerError::Generic(
                "reserve scanner generation is unsupported; v2 requires the BNS2/BRS2 scanner and unknown identities are rejected"
                    .to_string(),
            ));
        }
        let start_height = config.start_height.unwrap_or(0);
        let client = Client::new();
        let data_dir = data_dir.as_ref();

        // Log which Ergo node is being used (INFO level)
        info!("Initializing Ergo scanner with node: {}", config.node_url);
        if config.api_key.is_some() {
            info!("Ergo node API key is configured");
        } else {
            warn!("No API key configured for Ergo node");
        }

        // Open scanner metadata storage - create directory if it doesn't exist
        let storage_path = data_dir.join("scanner_metadata");

        // Create directory if it doesn't exist
        if let Some(parent) = storage_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                ScannerError::StoreError(format!(
                    "Failed to create scanner metadata directory: {}",
                    e
                ))
            })?;
        }

        let metadata_storage = ScannerMetadataStorage::open(&storage_path).map_err(|e| {
            ScannerError::StoreError(format!("Failed to open scanner metadata storage: {:?}", e))
        })?;

        // Open reserve storage - create directory if it doesn't exist
        let reserve_storage_path = data_dir.join("reserves");

        // Create directory if it doesn't exist
        if let Some(parent) = reserve_storage_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                ScannerError::StoreError(format!(
                    "Failed to create reserve storage directory: {}",
                    e
                ))
            })?;
        }

        let reserve_storage = ReserveStorage::open(&reserve_storage_path).map_err(|e| {
            ScannerError::StoreError(format!("Failed to open reserve storage: {:?}", e))
        })?;

        // Create reserve tracker and load existing reserves from database
        let reserve_tracker = ReserveTracker::new();

        // Load existing reserves from database
        if let Ok(existing_reserves) = reserve_storage.get_all_reserves() {
            let reserves_count = existing_reserves.len();
            for reserve in existing_reserves {
                if let Err(e) = reserve_tracker.update_reserve(reserve) {
                    warn!("Failed to load reserve from database: {}", e);
                }
            }
            info!("Loaded {} reserves from database", reserves_count);
        }

        // Create synchronized inner state
        let inner = Arc::new(Mutex::new(ServerStateInner {
            current_height: 0,
            last_scanned_height: start_height,
            scan_active: false,
        }));

        Ok(Self {
            config,
            inner,
            client,
            request_permits: Arc::new(Semaphore::new(MAX_CONCURRENT_SCANNER_REQUESTS)),
            reserve_tracker,
            metadata_storage,
            reserve_storage,
        })
    }

    /// Fetch current blockchain height directly from the node, bypassing cache.
    /// Used by the scanner loop so it detects new blocks immediately instead of
    /// reusing a potentially stale cached value for up to 10 minutes.
    pub async fn fetch_current_height(&self) -> Result<u64, ScannerError> {
        let url = format!("{}/info", self.config.node_url);
        info!("Fetching current blockchain height from: {}", url);

        let (status, body) = self
            .request_bytes(
                self.request_builder(reqwest::Method::GET, &url)?,
                "Failed to fetch node height",
            )
            .await?;

        if !status.is_success() {
            return Err(ScannerError::NodeError(format!(
                "Node returned status: {}",
                status
            )));
        }

        let info: serde_json::Value = serde_json::from_slice(&body)
            .map_err(|e| ScannerError::JsonError(format!("Failed to parse node info: {}", e)))?;

        let height = info["fullHeight"].as_u64().ok_or_else(|| {
            ScannerError::NodeError("Failed to parse fullHeight from node info".to_string())
        })?;

        // Keep the cache fresh for other callers that don't need the latest height.
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        if let Err(e) = self.metadata_storage.store_blockchain_height(height, now) {
            warn!("Failed to cache blockchain height: {:?}", e);
        }

        Ok(height)
    }

    /// Get current blockchain height from cache or Ergo node.
    /// Uses cached value if less than 10 minutes old, otherwise fetches from node.
    pub async fn get_current_height(&self) -> Result<u64, ScannerError> {
        const CACHE_TTL_MS: u64 = 600_000; // 10 minutes in milliseconds

        // Check if we have a cached height
        match self.metadata_storage.get_blockchain_height() {
            Ok(Some((cached_height, cached_timestamp))) => {
                let now = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as u64;

                if now.saturating_sub(cached_timestamp) < CACHE_TTL_MS {
                    debug!("Using cached blockchain height: {}", cached_height);
                    return Ok(cached_height);
                }

                debug!("Cached blockchain height expired, fetching from node");
            }
            Ok(None) => {
                debug!("No cached blockchain height found, fetching from node");
            }
            Err(e) => {
                warn!("Failed to read cached blockchain height: {:?}", e);
            }
        }

        // Fetch from node
        let url = format!("{}/info", self.config.node_url);

        let (status, body) = self
            .request_bytes(
                self.request_builder(reqwest::Method::GET, &url)?,
                "Failed to fetch node height",
            )
            .await?;

        if !status.is_success() {
            return Err(ScannerError::NodeError(format!(
                "Node returned status: {}",
                status
            )));
        }

        let info: serde_json::Value = serde_json::from_slice(&body)
            .map_err(|e| ScannerError::JsonError(format!("Failed to parse node info: {}", e)))?;

        let height = info["fullHeight"].as_u64().ok_or_else(|| {
            ScannerError::NodeError("Failed to parse fullHeight from node info".to_string())
        })?;

        // Store in cache with current timestamp
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        if let Err(e) = self.metadata_storage.store_blockchain_height(height, now) {
            warn!("Failed to cache blockchain height: {:?}", e);
        }

        Ok(height)
    }

    async fn fetch_indexed_height(&self) -> Result<IndexedHeightResponse, ScannerError> {
        let url = format!("{}/blockchain/indexedHeight", self.config.node_url);
        let (status, body) = self
            .request_bytes(
                self.request_builder(reqwest::Method::GET, &url)?,
                "Failed to fetch indexed height",
            )
            .await?;
        if !status.is_success() {
            return Err(ScannerError::NodeError(format!(
                "Failed to get indexed height with status {}: {}",
                status,
                summarize_error_body(&body)
            )));
        }
        serde_json::from_slice(&body).map_err(|e| {
            ScannerError::JsonError(format!("Failed to parse indexed height response: {}", e))
        })
    }

    fn require_caught_up(height: IndexedHeightResponse) -> Result<(), ScannerError> {
        if height.indexed_height < height.full_height {
            return Err(ScannerError::IndexLag {
                indexed_height: height.indexed_height,
                full_height: height.full_height,
            });
        }
        Ok(())
    }

    async fn fetch_unspent_reserve_page(
        &self,
        reserve_contract_p2s: &str,
        offset: usize,
    ) -> Result<Vec<IndexedErgoBox>, ScannerError> {
        let url = format!("{}/blockchain/box/unspent/byAddress", self.config.node_url);
        let request = self
            .request_builder(reqwest::Method::POST, &url)?
            .query(&[
                ("offset", offset.to_string()),
                ("limit", SCAN_PAGE_SIZE.to_string()),
                ("sortDirection", "asc".to_string()),
                ("includeUnconfirmed", "false".to_string()),
                ("excludeMempoolSpent", "false".to_string()),
            ])
            .json(reserve_contract_p2s);
        let (status, body) = self
            .request_bytes(request, "Failed to fetch reserve boxes")
            .await?;

        if status == StatusCode::NOT_FOUND {
            return Ok(Vec::new());
        }
        if !status.is_success() {
            return Err(ScannerError::NodeError(format!(
                "Failed to get reserve boxes with status {}: {}",
                status,
                summarize_error_body(&body)
            )));
        }
        serde_json::from_slice(&body).map_err(|e| {
            ScannerError::JsonError(format!("Failed to parse reserve boxes response: {}", e))
        })
    }

    /// Get a complete, height-coherent set of unspent reserve boxes via
    /// `POST /blockchain/box/unspent/byAddress`.
    pub async fn get_unspent_reserve_boxes(&self) -> Result<Vec<ScanBox>, ScannerError> {
        let reserve_contract_p2s = self.config.reserve_contract_p2s.as_ref().ok_or_else(|| {
            ScannerError::Generic("Reserve contract P2S not configured".to_string())
        })?;

        let before = self.fetch_indexed_height().await?;
        Self::require_caught_up(before)?;

        let mut parsed: ByAddressResponse = Vec::new();
        let mut seen_box_ids = HashSet::new();
        let mut exhausted = false;

        for page_index in 0..MAX_SCAN_PAGES {
            let offset = page_index.checked_mul(SCAN_PAGE_SIZE).ok_or_else(|| {
                ScannerError::ScanLimitExceeded("page offset overflow".to_string())
            })?;
            let page = self
                .fetch_unspent_reserve_page(reserve_contract_p2s, offset)
                .await?;
            if page.len() > SCAN_PAGE_SIZE {
                return Err(ScannerError::IncoherentSnapshot(format!(
                    "node returned {} boxes for requested page size {} at offset {}",
                    page.len(),
                    SCAN_PAGE_SIZE,
                    offset
                )));
            }

            let page_len = page.len();
            for box_ in page {
                if !seen_box_ids.insert(box_.box_id.clone()) {
                    return Err(ScannerError::IncoherentSnapshot(format!(
                        "duplicate box id {} across paginated response",
                        box_.box_id
                    )));
                }
                if parsed.len() >= MAX_SCAN_BOXES {
                    return Err(ScannerError::ScanLimitExceeded(format!(
                        "more than {} reserve boxes",
                        MAX_SCAN_BOXES
                    )));
                }
                parsed.push(box_);
            }

            if page_len < SCAN_PAGE_SIZE {
                exhausted = true;
                break;
            }
        }

        if !exhausted {
            return Err(ScannerError::ScanLimitExceeded(format!(
                "scan did not exhaust within {} pages",
                MAX_SCAN_PAGES
            )));
        }

        let after = self.fetch_indexed_height().await?;
        Self::require_caught_up(after)?;
        if before != after {
            return Err(ScannerError::IncoherentSnapshot(format!(
                "indexed/full height changed from {}/{} to {}/{} during pagination",
                before.indexed_height, before.full_height, after.indexed_height, after.full_height
            )));
        }

        info!(
            "Found {} reserve boxes in a complete snapshot at indexed height {}",
            parsed.len(),
            after.indexed_height
        );
        Ok(parsed.into_iter().map(Into::into).collect())
    }

    /// Check if scanner is active
    pub async fn is_active(&self) -> bool {
        // Use async lock for async context
        let inner = self.inner.lock().await;
        inner.scan_active
    }

    /// Start scanning with real blockchain integration
    pub async fn start_scanning(&mut self) -> Result<(), ScannerError> {
        info!("Starting Ergo blockchain scanner for reserves");

        if self.config.reserve_contract_p2s.is_none() {
            warn!("No reserve contract P2S specified, scanner will have no boxes to fetch");
        }

        // Update inner state
        {
            let mut inner = self.inner.lock().await;
            inner.scan_active = true;
        }

        Ok(())
    }

    /// Get last scanned height
    pub async fn last_scanned_height(&self) -> u64 {
        let inner = self.inner.lock().await;
        inner.last_scanned_height
    }

    /// Get the reserve tracker
    pub fn reserve_tracker(&self) -> &ReserveTracker {
        &self.reserve_tracker
    }

    /// Get the reserve storage for direct database access
    pub fn reserve_storage(&self) -> &ReserveStorage {
        &self.reserve_storage
    }

    /// Get unspent boxes from the blockchain via the reserve contract address.
    pub async fn get_scan_boxes(&self) -> Result<Vec<ScanBox>, ScannerError> {
        self.get_unspent_reserve_boxes().await
    }

    /// Parse reserve box into ExtendedReserveInfo
    pub(crate) fn parse_reserve_box(
        &self,
        scan_box: &ScanBox,
    ) -> Result<ExtendedReserveInfo, ScannerError> {
        let box_id = scan_box.box_id.clone();
        let value = scan_box.value;
        let creation_height = scan_box.creation_height;

        let expected_tree = crate::contract_compiler::get_basis_reserve_ergo_tree_hex()
            .map_err(|error| ScannerError::Generic(error.to_string()))?;
        let actual_tree = hex::decode(&scan_box.ergo_tree).map_err(|_| {
            ScannerError::InvalidReserveBox(format!("Invalid ErgoTree encoding in box {}", box_id))
        })?;
        let expected_tree = hex::decode(expected_tree).map_err(|_| {
            ScannerError::Generic("embedded historical ErgoTree is invalid".to_string())
        })?;
        if actual_tree != expected_tree {
            return Err(ScannerError::InvalidReserveBox(format!(
                "Reserve contract generation mismatch in box {}",
                box_id
            )));
        }

        // Extract owner public key from R4 register
        let owner_pubkey_raw = scan_box
            .additional_registers
            .get("R4")
            .ok_or_else(|| {
                ScannerError::InvalidReserveBox(format!("Missing R4 register in box {}", box_id))
            })?
            .clone();

        // Strip the 0x07 prefix if present (GroupElement type identifier from Ergo registers)
        let owner_pubkey = if owner_pubkey_raw.starts_with("07") && owner_pubkey_raw.len() >= 66 {
            // Extract the actual 33-byte public key (66 hex chars) after the 0x07 prefix
            owner_pubkey_raw[2..].to_string()
        } else {
            // Use as-is if no prefix or wrong length
            owner_pubkey_raw
        };

        // Extract tracker NFT ID from R6 register (required according to spec)
        let tracker_nft_id_raw = scan_box
            .additional_registers
            .get("R6")
            .ok_or_else(|| {
                ScannerError::InvalidReserveBox(format!("Missing R6 register in box {}", box_id))
            })?
            .clone();

        // Create extended reserve info
        // Decode the hex-encoded public key to actual bytes
        let owner_pubkey_bytes = hex::decode(&owner_pubkey).map_err(|_| {
            ScannerError::InvalidReserveBox(format!(
                "Invalid hex in owner pubkey for box {}",
                box_id
            ))
        })?;

        // Decode the hex-encoded tracker NFT ID to actual bytes
        // R6 contains a Coll[Byte] value with Ergo serialization prefix: 0e20 (type + length)
        // We need to strip the first 2 bytes (4 hex chars) to get the actual data
        let tracker_nft_hex = if tracker_nft_id_raw.len() >= 4 {
            &tracker_nft_id_raw[4..]
        } else {
            tracker_nft_id_raw.as_str()
        };
        let tracker_nft_id_bytes = hex::decode(tracker_nft_hex).map_err(|_| {
            ScannerError::InvalidReserveBox(format!(
                "Invalid hex in tracker NFT ID for box {}",
                box_id
            ))
        })?;

        // Validate that the tracker NFT ID is exactly 32 bytes (the actual tracker NFT ID)
        if tracker_nft_id_bytes.len() != 32 {
            return Err(ScannerError::InvalidReserveBox(format!(
                "Invalid tracker NFT ID length in box {}: expected 32 bytes, got {}",
                box_id,
                tracker_nft_id_bytes.len()
            )));
        }

        // Extract refund initiation height from R7 register (optional; absent = 0)
        let refund_initiation_height =
            decode_ergo_long_register(scan_box.additional_registers.get("R7"));

        let reserve_info = ExtendedReserveInfo::new(
            box_id.as_bytes(),
            &owner_pubkey_bytes,
            value,
            Some(&tracker_nft_id_bytes),
            creation_height,
            refund_initiation_height,
        );

        Ok(reserve_info)
    }

    /// Process scan boxes and update reserve tracker
    pub async fn process_scan_boxes(&self) -> Result<(), ScannerError> {
        info!("Starting to process scan boxes...");
        let scan_boxes = self.get_scan_boxes().await?;
        info!("Retrieved {} scan boxes to process", scan_boxes.len());

        // Validate the entire coherent snapshot before the first mutation. A
        // malformed candidate must not turn an incomplete observation into a
        // destructive reconciliation.
        let mut parsed_reserves = Vec::with_capacity(scan_boxes.len());
        for scan_box in &scan_boxes {
            debug!(
                "Processing scan box: ID={}, value={}, registers={:?}",
                scan_box.box_id, scan_box.value, scan_box.additional_registers
            );
            let reserve_info = self.parse_reserve_box(scan_box)?;
            debug!(
                "Successfully parsed reserve box: box_id={}, owner={}, collateral={}",
                reserve_info.box_id,
                reserve_info.owner_pubkey,
                reserve_info.base_info.collateral_amount
            );
            parsed_reserves.push(reserve_info);
        }

        let current_box_ids: HashSet<String> = parsed_reserves
            .iter()
            .map(|reserve| reserve.box_id.clone())
            .collect();
        let all_reserves = self.reserve_tracker.get_all_reserves();
        info!(
            "Current tracker has {} reserves, {} are still active in scan",
            all_reserves.len(),
            current_box_ids.len()
        );

        // Apply upserts only after collection and validation completed. Persist
        // before publishing the value to the in-memory reader.
        for reserve_info in &parsed_reserves {
            self.reserve_storage
                .store_reserve(reserve_info)
                .map_err(|e| {
                    ScannerError::StoreError(format!(
                        "Failed to persist reserve {}: {:?}",
                        reserve_info.box_id, e
                    ))
                })?;
            self.reserve_tracker
                .update_reserve(reserve_info.clone())
                .map_err(|e| {
                    ScannerError::StoreError(format!(
                        "Failed to update reserve {} in memory: {}",
                        reserve_info.box_id, e
                    ))
                })?;
        }

        // A successfully exhausted empty set is meaningful and removes every
        // stale reserve. Fetch, height, duplicate, bound, and parse failures
        // returned above before this destructive phase.
        for reserve in all_reserves {
            if current_box_ids.contains(&reserve.box_id) {
                continue;
            }
            info!(
                "Removing spent reserve: {} (not found in complete snapshot)",
                reserve.box_id
            );
            self.reserve_storage
                .remove_reserve(&reserve.box_id)
                .map_err(|e| {
                    ScannerError::StoreError(format!(
                        "Failed to remove reserve {} from database: {:?}",
                        reserve.box_id, e
                    ))
                })?;
            self.reserve_tracker
                .remove_reserve(&reserve.box_id)
                .map_err(|e| {
                    ScannerError::StoreError(format!(
                        "Failed to remove reserve {} from memory: {}",
                        reserve.box_id, e
                    ))
                })?;
        }

        debug!(
            "Finished processing scan boxes: {} processed, {} in tracker after processing",
            scan_boxes.len(),
            self.reserve_tracker.get_all_reserves().len()
        );

        Ok(())
    }
}

/// Start the scanner in background
pub async fn start_scanner(state: ServerState) -> Result<(), ScannerError> {
    let state = Arc::new(state);
    tokio::spawn(reserve_scanner_loop(state.clone()));
    Ok(())
}

/// Create a scanner with default configuration
pub fn create_default_scanner(data_dir: impl AsRef<Path>) -> Result<ServerState, ScannerError> {
    let config = NodeConfig::default();
    ServerState::new(config, data_dir)
}

/// Ergo box representation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErgoBox {
    pub box_id: String,
    pub value: u64,
    pub ergo_tree: String,
    pub creation_height: u64,
    pub transaction_id: String,
    pub additional_registers: std::collections::HashMap<String, String>,
}

impl ErgoBox {
    /// Get a specific register value
    pub fn get_register(&self, register: &str) -> Option<&str> {
        self.additional_registers.get(register).map(|s| s.as_str())
    }

    /// Check if this box has a specific register
    pub fn has_register(&self, register: &str) -> bool {
        self.additional_registers.contains_key(register)
    }
}

/// Scan box representation from Ergo node
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanBox {
    pub box_id: String,
    pub value: u64,
    pub ergo_tree: String,
    pub creation_height: u64,
    pub transaction_id: String,
    pub additional_registers: std::collections::HashMap<String, String>,
    /// Assets in the box (tokens)
    #[serde(default)]
    pub assets: Vec<BoxAsset>,
}

/// Asset in an Ergo box
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoxAsset {
    /// Token ID
    pub token_id: String,
    /// Amount
    pub amount: u64,
}

/// Events related to reserve activity
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ReserveEvent {
    /// A new reserve was created
    ReserveCreated {
        box_id: String,
        owner_pubkey: String,
        collateral_amount: u64,
        height: u64,
    },
    /// An existing reserve was topped up
    ReserveToppedUp {
        box_id: String,
        additional_collateral: u64,
        height: u64,
    },
    /// A redemption occurred from a reserve
    ReserveRedeemed {
        box_id: String,
        redeemed_amount: u64,
        height: u64,
    },
    /// A reserve was spent/closed
    ReserveSpent { box_id: String, height: u64 },
}

/// Default node configuration
impl Default for NodeConfig {
    fn default() -> Self {
        Self {
            start_height: None,
            reserve_contract_p2s: None,
            node_url: "http://159.89.116.15:11088".to_string(), // Your Ergo node
            scan_name: Some("Basis Reserve Scanner".to_string()),
            api_key: None,
        }
    }
}

/// Reserve scanner loop (background task)
pub async fn reserve_scanner_loop(state: Arc<ServerState>) -> Result<(), ScannerError> {
    info!("Starting reserve scanner background loop");

    let mut consecutive_failures = 0;
    const MAX_CONSECUTIVE_FAILURES: u32 = 5;

    loop {
        // Update current height
        match state.fetch_current_height().await {
            Ok(height) => {
                // Log height update at INFO level
                let previous_height = {
                    let inner = state.inner.lock().await;
                    inner.current_height
                };

                if height != previous_height {
                    info!("Current Ergo blockchain height: {}", height);
                    // Update current height in state
                    {
                        let mut inner = state.inner.lock().await;
                        inner.current_height = height;
                    }
                }

                // Process scan boxes whenever the height has advanced
                if height > state.last_scanned_height().await {
                    match state.process_scan_boxes().await {
                        Ok(()) => {
                            consecutive_failures = 0;
                            // Update last scanned height on success
                            {
                                let mut inner = state.inner.lock().await;
                                inner.last_scanned_height = height;
                            }
                        }
                        Err(e) => {
                            error!("Failed to process scan boxes: {}", e);
                            consecutive_failures += 1;

                            if consecutive_failures >= MAX_CONSECUTIVE_FAILURES {
                                error!(
                                    "Too many consecutive failures ({}), waiting before retry",
                                    consecutive_failures
                                );
                                tokio::time::sleep(Duration::from_secs(60)).await;
                                // Wait longer after many failures
                            }
                        }
                    }
                }
            }
            Err(e) => {
                error!("Failed to get current height: {}", e);
                consecutive_failures += 1;
                if consecutive_failures >= MAX_CONSECUTIVE_FAILURES {
                    error!(
                        "Too many consecutive failures ({}), waiting before retry",
                        consecutive_failures
                    );
                    tokio::time::sleep(Duration::from_secs(60)).await;
                }
            }
        }

        // Wait before next scan (shorter wait if we're recovering)
        let wait_time = if consecutive_failures > 0 {
            Duration::from_secs(10) // Shorter wait during recovery
        } else {
            Duration::from_secs(30) // Normal wait
        };
        tokio::time::sleep(wait_time).await;
    }
}

/// Decode an Ergo `Long` constant from its serialized hex representation.
/// Format: type byte `05` followed by zigzag-VLQ encoded value.
/// Missing, malformed, or negative values are treated as `0`.
pub fn decode_ergo_long_register(value: Option<&String>) -> u64 {
    let hex = match value {
        Some(h) if h.starts_with("05") && h.len() > 2 => &h[2..],
        _ => return 0,
    };
    let bytes = match hex::decode(hex) {
        Ok(b) => b,
        Err(_) => return 0,
    };
    decode_vlq_long(&bytes)
        .ok()
        .and_then(|v| if v >= 0 { Some(v as u64) } else { None })
        .unwrap_or(0)
}

/// Decode a zigzag-VLQ encoded signed long.
fn decode_vlq_long(bytes: &[u8]) -> Result<i64, String> {
    let mut zigzag: u64 = 0;
    let mut shift = 0;
    for &byte in bytes {
        let value = (byte & 0x7f) as u64;
        zigzag |= value << shift;
        if byte & 0x80 == 0 {
            let n = zigzag as i64;
            return Ok((n >> 1) ^ -(n & 1));
        }
        shift += 7;
        if shift > 63 {
            return Err("VLQ overflow".to_string());
        }
    }
    Err("Incomplete VLQ".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    async fn oversized_declared_response_server() -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = [0u8; 1024];
            let _ = socket.read(&mut request).await;
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                NODE_HTTP_MAX_BODY_BYTES + 1
            );
            let _ = socket.write_all(response.as_bytes()).await;
        });
        format!("http://{address}")
    }

    #[tokio::test]
    async fn reserve_scanner_rejects_oversized_declared_node_body() {
        let temp_dir = tempfile::tempdir().unwrap();
        let config = NodeConfig {
            node_url: oversized_declared_response_server().await,
            ..NodeConfig::default()
        };
        let state = ServerState::new(config, temp_dir.path()).unwrap();

        let error = state.fetch_current_height().await.unwrap_err();

        assert!(error
            .to_string()
            .contains("outbound node response body exceeds 2097152 bytes"));
    }

    fn historical_tree() -> String {
        crate::contract_compiler::get_basis_reserve_ergo_tree_hex().unwrap()
    }

    fn historical_config() -> NodeConfig {
        NodeConfig {
            reserve_contract_p2s: Some(
                crate::contract_compiler::get_basis_reserve_contract_p2s().unwrap(),
            ),
            ..NodeConfig::default()
        }
    }

    #[test]
    fn node_config_debug_redacts_api_key() {
        let sentinel = "sentinel-node-api-key-do-not-log";
        let config = NodeConfig {
            api_key: Some(sentinel.to_string()),
            ..NodeConfig::default()
        };

        let rendered = format!("{config:?}");
        assert!(!rendered.contains(sentinel));
        assert!(rendered.contains("<redacted>"));
    }

    #[test]
    fn test_parse_reserve_box_with_r6_register() {
        // Create a mock scan box with a public key that has the 0x07 prefix
        // and a valid 32-byte tracker NFT ID in R6 register
        let mut registers = HashMap::new();
        // This is a 33-byte public key with 0x07 prefix (GroupElement format)
        let prefixed_pubkey = "07c5b4b2f6c7d8e9f0a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0c1d2e3f4";
        registers.insert("R4".to_string(), prefixed_pubkey.to_string());
        // This is a 32-byte tracker NFT ID with Ergo Coll[Byte] serialization prefix (0e20 + 64 hex chars)
        let tracker_nft_id = "1af23d4e5f6a7b8c9daebfc0d1e2f30415263748596a7b8c9daebfc0d1e2f304";
        let tracker_nft_id_serialized = format!("0e20{}", tracker_nft_id);
        registers.insert("R6".to_string(), tracker_nft_id_serialized);

        let scan_box = ScanBox {
            box_id: "test_box_id".to_string(),
            value: 1000000000, // 1 ERG
            creation_height: 1000,
            ergo_tree: historical_tree(),
            transaction_id: "test_tx_id".to_string(),
            additional_registers: registers,
            assets: vec![],
        };

        // Create a dummy server state for testing
        let config = historical_config();
        let data_dir = std::env::temp_dir().join(format!(
            "basis_scanner_test_{}_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&data_dir);
        let server_state =
            ServerState::new(config, &data_dir).expect("Failed to create server state");

        // Test the parse_reserve_box function
        let result = server_state.parse_reserve_box(&scan_box);

        match result {
            Ok(reserve_info) => {
                // The owner_pubkey should have the 0x07 prefix stripped
                let expected_pubkey = &prefixed_pubkey[2..]; // Remove first 2 characters (07)

                assert_eq!(reserve_info.owner_pubkey, expected_pubkey);
                // The tracker_nft_id should match the one from R6 register
                assert_eq!(reserve_info.base_info.tracker_nft_id, tracker_nft_id);
                println!(
                    "SUCCESS: Prefix was correctly stripped. Original: {}, Stripped: {}",
                    prefixed_pubkey, reserve_info.owner_pubkey
                );
                println!(
                    "SUCCESS: Tracker NFT ID correctly extracted from R6 register: {}",
                    reserve_info.base_info.tracker_nft_id
                );
            }
            Err(e) => {
                panic!("Failed to parse reserve box: {:?}", e);
            }
        }
    }

    #[test]
    fn test_parse_reserve_box_missing_r6_register() {
        // Create a mock scan box with a public key but missing R6 register
        let mut registers = HashMap::new();
        // This is a 33-byte public key with 0x07 prefix (GroupElement format)
        let prefixed_pubkey = "07c5b4b2f6c7d8e9f0a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0c1d2e3f4";
        registers.insert("R4".to_string(), prefixed_pubkey.to_string());
        // Note: R6 register is intentionally missing

        let scan_box = ScanBox {
            box_id: "test_box_id_2".to_string(),
            value: 1000000000, // 1 ERG
            creation_height: 1000,
            ergo_tree: historical_tree(),
            transaction_id: "test_tx_id".to_string(),
            additional_registers: registers,
            assets: vec![],
        };

        // Create a dummy server state for testing
        let config = historical_config();
        let data_dir = std::env::temp_dir().join(format!(
            "basis_scanner_test_{}_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&data_dir);
        let server_state =
            ServerState::new(config, &data_dir).expect("Failed to create server state");

        // Test the parse_reserve_box function - should return an error
        let result = server_state.parse_reserve_box(&scan_box);

        match result {
            Ok(_) => {
                panic!("Expected error when R6 register is missing, but parsing succeeded");
            }
            Err(e) => {
                // Check that the error message mentions the missing R6 register
                assert!(e.to_string().contains("Missing R6 register"));
                println!(
                    "SUCCESS: Correctly returned error for missing R6 register: {:?}",
                    e
                );
            }
        }
    }

    #[test]
    fn test_parse_reserve_box_invalid_r6_length() {
        // Create a mock scan box with an invalid R6 register (not 32 bytes)
        let mut registers = HashMap::new();
        // This is a 33-byte public key with 0x07 prefix (GroupElement format)
        let prefixed_pubkey = "07c5b4b2f6c7d8e9f0a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0c1d2e3f4";
        registers.insert("R4".to_string(), prefixed_pubkey.to_string());
        // This is an invalid tracker NFT ID with wrong length (only 16 bytes = 32 hex chars, should be 32 bytes = 64 hex chars)
        let invalid_tracker_nft_id = "1af23d4e5f6a7b8c9daebfc0d1e2f304";
        registers.insert("R6".to_string(), invalid_tracker_nft_id.to_string());

        let scan_box = ScanBox {
            box_id: "test_box_id_3".to_string(),
            value: 1000000000, // 1 ERG
            creation_height: 1000,
            ergo_tree: historical_tree(),
            transaction_id: "test_tx_id".to_string(),
            additional_registers: registers,
            assets: vec![],
        };

        // Create a dummy server state for testing
        let config = historical_config();
        let data_dir = std::env::temp_dir().join(format!(
            "basis_scanner_test_{}_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&data_dir);
        let server_state =
            ServerState::new(config, &data_dir).expect("Failed to create server state");

        // Test the parse_reserve_box function - should return an error
        let result = server_state.parse_reserve_box(&scan_box);

        match result {
            Ok(_) => {
                panic!("Expected error when R6 register has invalid length, but parsing succeeded");
            }
            Err(e) => {
                // Check that the error message mentions the invalid length
                assert!(e.to_string().contains("Invalid tracker NFT ID length"));
                println!(
                    "SUCCESS: Correctly returned error for invalid R6 register length: {:?}",
                    e
                );
            }
        }
    }

    #[test]
    fn scanner_rejects_missing_v2_and_unknown_generations_before_opening_storage() {
        let root = std::env::temp_dir().join(format!(
            "basis_scanner_generation_guard_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let exact_v2 = crate::contract_compiler::get_basis_v2_contract_p2s(
            crate::contract_compiler::BasisV2ContractKind::Erg,
        )
        .unwrap();
        let configurations = [
            (None, "contract generation is required"),
            (Some(exact_v2), "generation is unsupported"),
            (
                Some("unknown-generation".to_string()),
                "generation is unsupported",
            ),
        ];
        for (configured, expected) in configurations {
            let config = NodeConfig {
                reserve_contract_p2s: configured,
                ..NodeConfig::default()
            };
            assert!(matches!(
                ServerState::new(config, &root),
                Err(ScannerError::Generic(message)) if message.contains(expected)
            ));
            assert!(!root.exists());
        }
    }

    #[test]
    fn unsupported_generation_never_loads_a_seeded_unversioned_reserve() {
        let exact_v2 = crate::contract_compiler::get_basis_v2_contract_p2s(
            crate::contract_compiler::BasisV2ContractKind::Erg,
        )
        .unwrap();
        for (marker, configured) in [
            (1u8, None),
            (2u8, Some(exact_v2)),
            (3u8, Some("unknown-generation".to_string())),
        ] {
            let root = std::env::temp_dir().join(format!(
                "basis_scanner_seeded_generation_guard_{}_{}_{}",
                std::process::id(),
                marker,
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            let _ = std::fs::remove_dir_all(&root);
            let storage = ReserveStorage::open(root.join("reserves")).unwrap();
            let mut reserve = ExtendedReserveInfo::new(
                &[marker; 32],
                &[2u8; 33],
                1_000_000,
                Some(&[3u8; 32]),
                1,
                0,
            );
            reserve.set_contract_address("unversioned-or-wrong-generation".to_string());
            storage.store_reserve(&reserve).unwrap();
            drop(storage);

            let config = NodeConfig {
                reserve_contract_p2s: configured,
                ..NodeConfig::default()
            };
            assert!(ServerState::new(config, &root).is_err());

            let storage = ReserveStorage::open(root.join("reserves")).unwrap();
            let persisted = storage.get_all_reserves().unwrap();
            assert_eq!(persisted.len(), 1);
            assert_eq!(persisted[0].box_id, reserve.box_id);
            assert_eq!(
                persisted[0].base_info.contract_address,
                reserve.base_info.contract_address
            );
        }
    }

    #[test]
    fn parser_rejects_a_box_from_another_contract_generation_first() {
        let mut registers = HashMap::new();
        registers.insert(
            "R4".to_string(),
            "02c5b4b2f6c7d8e9f0a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0c1d2e3".to_string(),
        );
        registers.insert("R6".to_string(), format!("0e20{}", "11".repeat(32)));
        let scan_box = ScanBox {
            box_id: "wrong-generation".to_string(),
            value: 1_000_000,
            ergo_tree: crate::contract_compiler::BASIS_V2_ERG_ERGO_TREE_HEX
                .trim()
                .to_string(),
            creation_height: 1,
            transaction_id: "tx".to_string(),
            additional_registers: registers,
            assets: Vec::new(),
        };
        let root = std::env::temp_dir().join(format!(
            "basis_scanner_parser_guard_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let state = ServerState::new(historical_config(), &root).unwrap();
        assert!(matches!(
            state.parse_reserve_box(&scan_box),
            Err(ScannerError::InvalidReserveBox(message)) if message.contains("generation mismatch")
        ));
    }
}
