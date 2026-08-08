use std::{
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex as StdMutex,
    },
    time::Duration,
};

use serde_json::{json, Value};
use tempfile::TempDir;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
    task::JoinHandle,
};

use crate::{
    ergo_scanner::{
        NodeConfig, ScannerError, ServerState, MAX_CONCURRENT_SCANNER_REQUESTS,
        MAX_RESPONSE_BODY_BYTES, SCAN_PAGE_SIZE,
    },
    persistence::{ScannerMetadataStorage, TrackerStorage},
    tracker_scanner::{create_tracker_server_state, TrackerNodeConfig, TrackerServerState},
    ExtendedReserveInfo,
};

#[derive(Clone)]
struct MockResponse {
    status: u16,
    body: Vec<u8>,
    delay: Duration,
    declared_length: Option<usize>,
    omit_length: bool,
}

impl MockResponse {
    fn json(value: Value) -> Self {
        Self {
            status: 200,
            body: serde_json::to_vec(&value).expect("mock JSON must serialize"),
            delay: Duration::ZERO,
            declared_length: None,
            omit_length: false,
        }
    }

    fn status(status: u16) -> Self {
        Self {
            status,
            body: b"{}".to_vec(),
            delay: Duration::ZERO,
            declared_length: None,
            omit_length: false,
        }
    }

    fn delayed(mut self, delay: Duration) -> Self {
        self.delay = delay;
        self
    }

    fn with_declared_length(mut self, length: usize) -> Self {
        self.declared_length = Some(length);
        self
    }

    fn with_body(mut self, body: Vec<u8>) -> Self {
        self.body = body;
        self
    }

    fn without_declared_length(mut self) -> Self {
        self.omit_length = true;
        self
    }
}

struct MockNode {
    base_url: String,
    requests: Arc<StdMutex<Vec<String>>>,
    max_active: Arc<AtomicUsize>,
    task: JoinHandle<()>,
}

impl MockNode {
    async fn start<F>(handler: F) -> Self
    where
        F: Fn(&str) -> MockResponse + Send + Sync + 'static,
    {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("loopback mock must bind");
        let address = listener.local_addr().expect("mock address must exist");
        let requests = Arc::new(StdMutex::new(Vec::new()));
        let active = Arc::new(AtomicUsize::new(0));
        let max_active = Arc::new(AtomicUsize::new(0));
        let handler = Arc::new(handler);
        let task_requests = Arc::clone(&requests);
        let task_active = Arc::clone(&active);
        let task_max_active = Arc::clone(&max_active);

        let task = tokio::spawn(async move {
            loop {
                let (mut stream, _) = match listener.accept().await {
                    Ok(connection) => connection,
                    Err(_) => break,
                };
                let handler = Arc::clone(&handler);
                let requests = Arc::clone(&task_requests);
                let active = Arc::clone(&task_active);
                let max_active = Arc::clone(&task_max_active);
                tokio::spawn(async move {
                    let target = match read_request_target(&mut stream).await {
                        Some(target) => target,
                        None => return,
                    };
                    requests
                        .lock()
                        .expect("request log lock must not be poisoned")
                        .push(target.clone());
                    let now_active = active.fetch_add(1, Ordering::SeqCst) + 1;
                    max_active.fetch_max(now_active, Ordering::SeqCst);

                    let response = handler(&target);
                    tokio::time::sleep(response.delay).await;
                    let reason = match response.status {
                        200 => "OK",
                        404 => "Not Found",
                        429 => "Too Many Requests",
                        500 => "Internal Server Error",
                        503 => "Service Unavailable",
                        _ => "Mock Status",
                    };
                    let content_length = if response.omit_length {
                        String::new()
                    } else {
                        format!(
                            "Content-Length: {}\r\n",
                            response.declared_length.unwrap_or(response.body.len())
                        )
                    };
                    let head = format!(
                        "HTTP/1.1 {} {}\r\nContent-Type: application/json\r\n{}Connection: close\r\n\r\n",
                        response.status, reason, content_length
                    );
                    let _ = stream.write_all(head.as_bytes()).await;
                    let _ = stream.write_all(&response.body).await;
                    let _ = stream.shutdown().await;
                    active.fetch_sub(1, Ordering::SeqCst);
                });
            }
        });

        Self {
            base_url: format!("http://{}", address),
            requests,
            max_active,
            task,
        }
    }

    fn requests(&self) -> Vec<String> {
        self.requests
            .lock()
            .expect("request log lock must not be poisoned")
            .clone()
    }

    fn max_active(&self) -> usize {
        self.max_active.load(Ordering::SeqCst)
    }
}

impl Drop for MockNode {
    fn drop(&mut self) {
        self.task.abort();
    }
}

async fn read_request_target(stream: &mut tokio::net::TcpStream) -> Option<String> {
    let mut request = Vec::new();
    let mut buffer = [0u8; 4_096];
    let (header_end, content_length) = loop {
        let read = stream.read(&mut buffer).await.ok()?;
        if read == 0 {
            return None;
        }
        request.extend_from_slice(&buffer[..read]);
        if request.len() > 64 * 1024 {
            return None;
        }
        if let Some(header_start) = request.windows(4).position(|part| part == b"\r\n\r\n") {
            let header_end = header_start + 4;
            let headers = String::from_utf8_lossy(&request[..header_end]);
            let content_length = headers
                .lines()
                .find_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    name.eq_ignore_ascii_case("content-length")
                        .then(|| value.trim().parse::<usize>().ok())
                        .flatten()
                })
                .unwrap_or(0);
            break (header_end, content_length);
        }
    };
    while request.len() < header_end.checked_add(content_length)? {
        let read = stream.read(&mut buffer).await.ok()?;
        if read == 0 {
            return None;
        }
        request.extend_from_slice(&buffer[..read]);
    }

    String::from_utf8_lossy(&request)
        .lines()
        .next()?
        .split_whitespace()
        .nth(1)
        .map(str::to_string)
}

fn query_offset(target: &str) -> Option<usize> {
    target.split_once('?')?.1.split('&').find_map(|pair| {
        let (name, value) = pair.split_once('=')?;
        (name == "offset")
            .then(|| value.parse::<usize>().ok())
            .flatten()
    })
}

fn indexed_height(height: u64) -> MockResponse {
    MockResponse::json(json!({
        "indexedHeight": height,
        "fullHeight": height
    }))
}

fn indexed_box(index: usize, tracker_asset: bool) -> Value {
    let tracker_nft = "11".repeat(32);
    let reserve_tree = crate::contract_compiler::get_basis_reserve_ergo_tree_hex()
        .expect("embedded historical reserve ErgoTree must compile");
    json!({
        "boxId": format!("{index:064x}"),
        "value": 1_000_000_000u64 + index as u64,
        "ergoTree": reserve_tree,
        "creationHeight": 100u64 + index as u64,
        "transactionId": format!("{:064x}", 10_000usize + index),
        "additionalRegisters": {
            "R4": format!("07{}", format!("02{}", "22".repeat(32))),
            "R5": format!("64{}", "33".repeat(32)),
            "R6": format!("0e20{}", tracker_nft)
        },
        "assets": if tracker_asset {
            vec![json!({ "tokenId": tracker_nft, "amount": 1u64 })]
        } else {
            Vec::<Value>::new()
        }
    })
}

fn malformed_reserve_box(index: usize) -> Value {
    let mut box_ = indexed_box(index, false);
    box_["additionalRegisters"]
        .as_object_mut()
        .expect("registers must be an object")
        .remove("R6");
    box_
}

fn reserve_state(node: &MockNode, temp_dir: &TempDir) -> ServerState {
    ServerState::new(
        NodeConfig {
            start_height: Some(0),
            reserve_contract_p2s: Some(
                crate::contract_compiler::get_basis_reserve_contract_p2s()
                    .expect("embedded historical reserve P2S must compile"),
            ),
            node_url: node.base_url.clone(),
            scan_name: Some("test".to_string()),
            api_key: None,
        },
        temp_dir.path(),
    )
    .expect("reserve scanner state must initialize")
}

fn tracker_state(node: &MockNode, temp_dir: &TempDir) -> TrackerServerState {
    let metadata = ScannerMetadataStorage::open(temp_dir.path().join("tracker-metadata"))
        .expect("tracker metadata must open");
    let storage = TrackerStorage::open(temp_dir.path().join("tracker-boxes"))
        .expect("tracker storage must open");
    create_tracker_server_state(
        TrackerNodeConfig {
            start_height: Some(0),
            tracker_nft_id: Some("11".repeat(32)),
            node_url: node.base_url.clone(),
            scan_name: Some("test".to_string()),
            api_key: None,
        },
        metadata,
        storage,
    )
}

fn insert_stale_reserve(state: &ServerState) -> String {
    let reserve = ExtendedReserveInfo::new(
        b"stale-reserve-box",
        &[2u8; 33],
        5_000_000_000,
        Some(&[3u8; 32]),
        50,
        0,
    );
    let box_id = reserve.box_id.clone();
    state
        .reserve_storage
        .store_reserve(&reserve)
        .expect("stale reserve must persist");
    state
        .reserve_tracker
        .update_reserve(reserve)
        .expect("stale reserve must enter memory");
    box_id
}

fn assert_only_stale_reserve(state: &ServerState, stale_box_id: &str) {
    let in_memory = state.reserve_tracker.get_all_reserves();
    assert_eq!(in_memory.len(), 1);
    assert_eq!(in_memory[0].box_id, stale_box_id);
    let persisted = state
        .reserve_storage
        .get_all_reserves()
        .expect("reserve storage must be readable");
    assert_eq!(persisted.len(), 1);
    assert_eq!(persisted[0].box_id, stale_box_id);
}

#[tokio::test]
async fn reserve_scan_paginates_past_explicit_page_size() {
    let first_page: Vec<Value> = (0..SCAN_PAGE_SIZE)
        .map(|index| indexed_box(index, false))
        .collect();
    let last_page = vec![indexed_box(SCAN_PAGE_SIZE, false)];
    let node = MockNode::start(move |target| {
        if target.starts_with("/blockchain/indexedHeight") {
            return indexed_height(500);
        }
        match query_offset(target) {
            Some(0) => MockResponse::json(Value::Array(first_page.clone())),
            Some(offset) if offset == SCAN_PAGE_SIZE => {
                MockResponse::json(Value::Array(last_page.clone()))
            }
            _ => MockResponse::status(500),
        }
    })
    .await;
    let temp_dir = tempfile::tempdir().expect("temporary data dir must open");
    let state = reserve_state(&node, &temp_dir);

    state
        .process_scan_boxes()
        .await
        .expect("complete multi-page snapshot must reconcile");

    assert_eq!(
        state.reserve_tracker.get_all_reserves().len(),
        SCAN_PAGE_SIZE + 1
    );
    let requests = node.requests();
    assert!(requests.iter().any(|request| request.contains("offset=0")));
    assert!(requests
        .iter()
        .any(|request| request.contains(&format!("offset={SCAN_PAGE_SIZE}"))));
    assert!(requests
        .iter()
        .filter(|request| request.contains("/blockchain/box/unspent/byAddress"))
        .all(|request| request.contains("excludeMempoolSpent=false")));
    assert!(
        requests
            .iter()
            .filter(|request| request.starts_with("/blockchain/indexedHeight"))
            .count()
            >= 2
    );
}

#[tokio::test]
async fn tracker_scan_paginates_past_explicit_page_size() {
    let first_page: Vec<Value> = (0..SCAN_PAGE_SIZE)
        .map(|index| indexed_box(index, true))
        .collect();
    let last_page = vec![indexed_box(SCAN_PAGE_SIZE, true)];
    let node = MockNode::start(move |target| {
        if target.starts_with("/blockchain/indexedHeight") {
            return indexed_height(500);
        }
        match query_offset(target) {
            Some(0) => MockResponse::json(Value::Array(first_page.clone())),
            Some(offset) if offset == SCAN_PAGE_SIZE => {
                MockResponse::json(Value::Array(last_page.clone()))
            }
            _ => MockResponse::status(500),
        }
    })
    .await;
    let temp_dir = tempfile::tempdir().expect("temporary data dir must open");
    let state = tracker_state(&node, &temp_dir);

    let boxes = state
        .get_unspent_tracker_boxes()
        .await
        .expect("complete tracker snapshot must succeed");

    assert_eq!(boxes.len(), SCAN_PAGE_SIZE + 1);
}

#[tokio::test]
async fn complete_empty_snapshot_removes_stale_reserves() {
    let node = MockNode::start(|target| {
        if target.starts_with("/blockchain/indexedHeight") {
            indexed_height(500)
        } else {
            MockResponse::json(Value::Array(Vec::new()))
        }
    })
    .await;
    let temp_dir = tempfile::tempdir().expect("temporary data dir must open");
    let state = reserve_state(&node, &temp_dir);
    insert_stale_reserve(&state);

    state
        .process_scan_boxes()
        .await
        .expect("complete empty snapshot must reconcile");

    assert!(state.reserve_tracker.get_all_reserves().is_empty());
    assert!(state
        .reserve_storage
        .get_all_reserves()
        .expect("reserve storage must be readable")
        .is_empty());
}

#[tokio::test]
async fn failed_later_page_preserves_previous_snapshot_without_partial_upserts() {
    let first_page: Vec<Value> = (0..SCAN_PAGE_SIZE)
        .map(|index| indexed_box(index, false))
        .collect();
    let node = MockNode::start(move |target| {
        if target.starts_with("/blockchain/indexedHeight") {
            return indexed_height(500);
        }
        match query_offset(target) {
            Some(0) => MockResponse::json(Value::Array(first_page.clone())),
            Some(offset) if offset == SCAN_PAGE_SIZE => MockResponse::status(503),
            _ => MockResponse::status(500),
        }
    })
    .await;
    let temp_dir = tempfile::tempdir().expect("temporary data dir must open");
    let state = reserve_state(&node, &temp_dir);
    let stale_box_id = insert_stale_reserve(&state);

    let error = state
        .process_scan_boxes()
        .await
        .expect_err("later page failure must fail the snapshot");
    assert!(matches!(error, ScannerError::NodeError(_)));
    assert_only_stale_reserve(&state, &stale_box_id);
}

#[tokio::test]
async fn malformed_only_page_preserves_previous_snapshot() {
    let node = MockNode::start(|target| {
        if target.starts_with("/blockchain/indexedHeight") {
            indexed_height(500)
        } else {
            MockResponse::json(Value::Array(vec![malformed_reserve_box(1)]))
        }
    })
    .await;
    let temp_dir = tempfile::tempdir().expect("temporary data dir must open");
    let state = reserve_state(&node, &temp_dir);
    let stale_box_id = insert_stale_reserve(&state);

    let error = state
        .process_scan_boxes()
        .await
        .expect_err("malformed candidate must reject reconciliation");
    assert!(matches!(error, ScannerError::InvalidReserveBox(_)));
    assert_only_stale_reserve(&state, &stale_box_id);
}

#[tokio::test]
async fn duplicate_across_pages_preserves_previous_snapshot() {
    let first_page: Vec<Value> = (0..SCAN_PAGE_SIZE)
        .map(|index| indexed_box(index, false))
        .collect();
    let duplicate_page = vec![indexed_box(SCAN_PAGE_SIZE - 1, false)];
    let node = MockNode::start(move |target| {
        if target.starts_with("/blockchain/indexedHeight") {
            return indexed_height(500);
        }
        match query_offset(target) {
            Some(0) => MockResponse::json(Value::Array(first_page.clone())),
            Some(offset) if offset == SCAN_PAGE_SIZE => {
                MockResponse::json(Value::Array(duplicate_page.clone()))
            }
            _ => MockResponse::status(500),
        }
    })
    .await;
    let temp_dir = tempfile::tempdir().expect("temporary data dir must open");
    let state = reserve_state(&node, &temp_dir);
    let stale_box_id = insert_stale_reserve(&state);

    let error = state
        .process_scan_boxes()
        .await
        .expect_err("duplicate page row must reject reconciliation");
    assert!(matches!(error, ScannerError::IncoherentSnapshot(_)));
    assert_only_stale_reserve(&state, &stale_box_id);
}

#[tokio::test]
async fn height_drift_preserves_previous_snapshot() {
    let height_call = Arc::new(AtomicUsize::new(0));
    let handler_height_call = Arc::clone(&height_call);
    let node = MockNode::start(move |target| {
        if target.starts_with("/blockchain/indexedHeight") {
            let call = handler_height_call.fetch_add(1, Ordering::SeqCst);
            return indexed_height(if call == 0 { 500 } else { 501 });
        }
        MockResponse::json(Value::Array(vec![indexed_box(1, false)]))
    })
    .await;
    let temp_dir = tempfile::tempdir().expect("temporary data dir must open");
    let state = reserve_state(&node, &temp_dir);
    let stale_box_id = insert_stale_reserve(&state);

    let error = state
        .process_scan_boxes()
        .await
        .expect_err("moving indexed height must reject reconciliation");
    assert!(matches!(error, ScannerError::IncoherentSnapshot(_)));
    assert_only_stale_reserve(&state, &stale_box_id);
}

#[tokio::test]
async fn indexed_height_lag_preserves_previous_snapshot_without_page_query() {
    let node = MockNode::start(|target| {
        if target.starts_with("/blockchain/indexedHeight") {
            MockResponse::json(json!({ "indexedHeight": 499u64, "fullHeight": 500u64 }))
        } else {
            MockResponse::status(500)
        }
    })
    .await;
    let temp_dir = tempfile::tempdir().expect("temporary data dir must open");
    let state = reserve_state(&node, &temp_dir);
    let stale_box_id = insert_stale_reserve(&state);

    let error = state
        .process_scan_boxes()
        .await
        .expect_err("lagging index must reject reconciliation");
    assert!(matches!(error, ScannerError::IndexLag { .. }));
    assert_only_stale_reserve(&state, &stale_box_id);
    assert!(node
        .requests()
        .iter()
        .all(|request| !request.contains("/blockchain/box/unspent/byAddress")));
}

#[tokio::test]
async fn oversized_page_is_rejected_before_json_parsing() {
    let node = MockNode::start(|target| {
        if target.starts_with("/blockchain/indexedHeight") {
            indexed_height(500)
        } else {
            MockResponse::json(json!([])).with_declared_length(MAX_RESPONSE_BODY_BYTES + 1)
        }
    })
    .await;
    let temp_dir = tempfile::tempdir().expect("temporary data dir must open");
    let state = reserve_state(&node, &temp_dir);

    let error = state
        .get_unspent_reserve_boxes()
        .await
        .expect_err("oversized response must be rejected");
    assert!(matches!(error, ScannerError::ResponseTooLarge { .. }));
}

#[tokio::test]
async fn oversized_page_without_content_length_is_still_rejected() {
    let node = MockNode::start(|target| {
        if target.starts_with("/blockchain/indexedHeight") {
            indexed_height(500)
        } else {
            MockResponse::status(200)
                .without_declared_length()
                .with_body(vec![b' '; MAX_RESPONSE_BODY_BYTES + 1])
        }
    })
    .await;
    let temp_dir = tempfile::tempdir().expect("temporary data dir must open");
    let state = reserve_state(&node, &temp_dir);

    let error = state
        .get_unspent_reserve_boxes()
        .await
        .expect_err("streamed oversized response must be rejected");
    assert!(matches!(error, ScannerError::ResponseTooLarge { .. }));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn shared_gate_bounds_concurrent_scanner_requests() {
    let node = MockNode::start(|_| {
        MockResponse::json(json!({ "fullHeight": 500u64 })).delayed(Duration::from_millis(100))
    })
    .await;
    let temp_dir = tempfile::tempdir().expect("temporary data dir must open");
    let state = reserve_state(&node, &temp_dir);

    let mut requests = Vec::new();
    for _ in 0..(MAX_CONCURRENT_SCANNER_REQUESTS * 2) {
        let state = state.clone();
        requests.push(tokio::spawn(
            async move { state.fetch_current_height().await },
        ));
    }
    let mut succeeded = 0;
    let mut rejected = 0;
    for request in requests {
        match request.await.expect("height task must not panic") {
            Ok(_) => succeeded += 1,
            Err(ScannerError::RequestCapacityExceeded) => rejected += 1,
            Err(error) => panic!("unexpected scanner error: {error}"),
        }
    }

    assert_eq!(succeeded, MAX_CONCURRENT_SCANNER_REQUESTS);
    assert_eq!(rejected, MAX_CONCURRENT_SCANNER_REQUESTS);
    assert!(node.max_active() <= MAX_CONCURRENT_SCANNER_REQUESTS);
    assert!(node.max_active() >= 2);
}
