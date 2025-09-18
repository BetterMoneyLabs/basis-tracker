//! API handlers for reserve-related endpoints

use axum::{extract::State, http::StatusCode, Json};
use serde::Serialize;

use crate::{error_response, success_response, ApiResponse, AppState};

/// Get reserves by issuer public key
#[axum::debug_handler]
pub async fn get_reserves_by_issuer(
    State(_state): State<AppState>,
    axum::extract::Path(pubkey_hex): axum::extract::Path<String>,
) -> (StatusCode, Json<ApiResponse<Vec<SerializableReserveInfo>>>) {
    tracing::debug!("Getting reserves for issuer: {}", pubkey_hex);

    // In a real implementation, this would:
    // 1. Connect to the reserve tracker (which would be part of AppState)
    // 2. Look up reserves by owner public key
    // 3. Return the reserve information

    // For now, return a realistic mock response
    let reserves = vec![
        SerializableReserveInfo {
            box_id: "f1e2d3c4b5a697887766554433221100ffeeddccbbaa99887766554433221100".to_string(),
            owner_pubkey: pubkey_hex.clone(),
            collateral_amount: 2500000000, // 2.5 ERG
            total_debt: 1200000000,        // 1.2 ERG debt
            tracker_nft_id: Some(
                "a1b2c3d4e5f67788990011223344556677889900112233445566778899001122".to_string(),
            ),
            last_updated_height: 1250,
            last_updated_timestamp: 1672531200, // Jan 1, 2023
            collateralization_ratio: 2.08,
        },
        SerializableReserveInfo {
            box_id: "aa11bb22cc33dd44ee55ff6677889900aabbccddeeff00112233445566778899".to_string(),
            owner_pubkey: pubkey_hex.clone(),
            collateral_amount: 1000000000, // 1.0 ERG
            total_debt: 800000000,         // 0.8 ERG debt
            tracker_nft_id: None,
            last_updated_height: 1248,
            last_updated_timestamp: 1672444800, // Dec 31, 2022
            collateralization_ratio: 1.25,
        },
    ];

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
