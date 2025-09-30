//! API handlers for reserve-related endpoints

use axum::{extract::State, http::StatusCode, Json};
use serde::Serialize;

use crate::{
    models::{success_response, ApiResponse},
    AppState,
};

/// Get reserves by issuer public key
#[axum::debug_handler]
pub async fn get_reserves_by_issuer(
    State(state): State<AppState>,
    axum::extract::Path(pubkey_hex): axum::extract::Path<String>,
) -> (StatusCode, Json<ApiResponse<Vec<SerializableReserveInfo>>>) {
    tracing::debug!("Getting reserves for issuer: {}", pubkey_hex);

    // Get reserve info from tracker
    let tracker = state.reserve_tracker.lock().await;
    let all_reserves = tracker.get_all_reserves();

    // Filter reserves by owner pubkey
    let reserves: Vec<SerializableReserveInfo> = all_reserves
        .into_iter()
        .filter(|reserve| reserve.owner_pubkey == pubkey_hex)
        .map(|info| {
            let collateralization_ratio = info.collateralization_ratio();
            SerializableReserveInfo {
                box_id: info.box_id,
                owner_pubkey: info.owner_pubkey,
                collateral_amount: info.base_info.collateral_amount,
                total_debt: info.total_debt,
                tracker_nft_id: info.tracker_nft_id,
                last_updated_height: info.base_info.last_updated_height,
                last_updated_timestamp: info.last_updated_timestamp,
                collateralization_ratio,
            }
        })
        .collect();

    tracing::info!(
        "Returning {} reserves for issuer {}",
        reserves.len(),
        pubkey_hex
    );

    (StatusCode::OK, Json(success_response(reserves)))
}

/// Serializable version of ExtendedReserveInfo for API responses
#[derive(Debug, Serialize)]
pub struct SerializableReserveInfo {
    pub box_id: String,
    pub owner_pubkey: String,
    pub collateral_amount: u64,
    pub total_debt: u64,
    pub tracker_nft_id: Option<String>,
    pub last_updated_height: u64,
    pub last_updated_timestamp: u64,
    pub collateralization_ratio: f64,
}
