//! Structural tombstones for the retired tracker-assisted v1 redemption API.
//!
//! This module deliberately contains no transaction construction, proof,
//! signing, node submission, broadcast, or state-mutation implementation.

use axum::{extract::State, http::StatusCode, Json};
use ergo_lib::ergo_chain_types::Header;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::models::ApiResponse;
use crate::AppState;

/// Legacy request shape retained only so stale clients receive HTTP 410 rather
/// than reaching an alternate deserialization path.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RedemptionBuildRequest {
    pub issuer_pubkey: String,
    pub recipient_pubkey: String,
    pub amount: u64,
    pub timestamp: u64,
    pub issuer_signature: String,
    #[serde(default)]
    pub emergency: bool,
    #[serde(default)]
    pub tracker_box_id: Option<String>,
}

/// Legacy response shape retained for source compatibility only. No production
/// function can construct or return a successful instance.
#[derive(Debug, Serialize)]
pub struct RedemptionBuildResponse {
    pub unsigned_tx: Value,
    pub partial_tx: Value,
    pub input_box_binaries: Vec<String>,
    pub data_box_binaries: Vec<String>,
    pub headers: Vec<Header>,
    pub reserve_box_id: String,
    pub tracker_box_id: String,
    pub reserve_output_value: u64,
    pub recipient_output_value: u64,
    pub total_debt: u64,
    pub change_amount: u64,
    pub change_address: String,
    pub recipient_address: String,
    pub is_first_redemption: bool,
    pub fee: u64,
    pub new_already_redeemed: u64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RedemptionSubmitRequest {
    pub signed_tx: Value,
}

#[derive(Debug, Serialize)]
pub struct RedemptionSubmitResponse {
    pub tx_id: String,
}

/// Retired v1 build endpoint. V2 has a separate exact-manifest boundary and is
/// intentionally not activated by this compatibility route.
#[axum::debug_handler]
pub async fn build_redemption(
    State(_state): State<AppState>,
    Json(_payload): Json<RedemptionBuildRequest>,
) -> (StatusCode, Json<ApiResponse<RedemptionBuildResponse>>) {
    crate::reject_retired_v1_redemption()
}

/// Retired v1 submit endpoint. There is no node or broadcast client in this
/// module.
#[axum::debug_handler]
pub async fn submit_redemption(
    State(_state): State<AppState>,
    Json(_payload): Json<RedemptionSubmitRequest>,
) -> (StatusCode, Json<ApiResponse<RedemptionSubmitResponse>>) {
    crate::reject_retired_v1_redemption()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_request_rejects_caller_selected_change() {
        let request = serde_json::json!({
            "issuer_pubkey": "02",
            "recipient_pubkey": "03",
            "amount": 1,
            "timestamp": 2,
            "issuer_signature": "04",
            "change_address": "caller-controlled"
        });
        assert!(serde_json::from_value::<RedemptionBuildRequest>(request).is_err());
    }

    #[test]
    fn submit_request_rejects_unverified_accounting_metadata() {
        let request = serde_json::json!({
            "signed_tx": {},
            "issuer_pubkey": "02",
            "new_already_redeemed": 99
        });
        assert!(serde_json::from_value::<RedemptionSubmitRequest>(request).is_err());
    }
}
