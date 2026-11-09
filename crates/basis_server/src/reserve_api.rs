//! API handlers for reserve-related endpoints

use axum::{extract::State, http::StatusCode, Json};
use serde::Serialize;

use crate::{
    models::{success_response, ApiResponse},
    AppState,
};

// Helper function to decode potentially double-hex-encoded strings
fn decode_potentially_double_hex_encoded(hex_string: &str) -> String {
    // First, try to decode as hex
    if let Ok(decoded_bytes) = hex::decode(hex_string) {
        // Check if the decoded bytes look like a hex string (only contains valid hex chars)
        let decoded_as_string = String::from_utf8_lossy(&decoded_bytes);
        if is_hex_string(&decoded_as_string) {
            // It was double-encoded, return the single-decoded version
            return decoded_as_string.to_string();
        }
    }
    // If not double-encoded, return the original
    hex_string.to_string()
}

// Helper function to check if a string contains only valid hex characters
fn is_hex_string(s: &str) -> bool {
    s.chars().all(|c| c.is_ascii_hexdigit())
}

/// Get all reserves (regardless of issuer)
#[axum::debug_handler]
pub async fn get_all_reserves(
    State(state): State<AppState>,
) -> (StatusCode, Json<ApiResponse<Vec<SerializableReserveInfo>>>) {
    tracing::debug!("Getting all reserves");

    // Get reserve storage from scanner and query database directly
    let scanner = state.ergo_scanner.lock().await;
    let reserve_storage = scanner.reserve_storage();

    // Get all reserves from database
    match reserve_storage.get_all_reserves() {
        Ok(all_reserves) => {
            let reserves: Vec<SerializableReserveInfo> = all_reserves
                .into_iter()
                .map(|info| {
                    let collateralization_ratio = info.collateralization_ratio();
                    SerializableReserveInfo {
                        box_id: info.box_id,
                        owner_pubkey: decode_potentially_double_hex_encoded(&info.owner_pubkey),
                        collateral_amount: info.base_info.collateral_amount,
                        total_debt: info.total_debt,
                        tracker_nft_id: info.base_info.tracker_nft_id.clone(),
                        reserve_token_id: info.base_info.reserve_token_id.clone(),
                        last_updated_height: info.base_info.last_updated_height,
                        last_updated_timestamp: info.last_updated_timestamp,
                        collateralization_ratio,
                        refund_initiation_height: info.base_info.refund_initiation_height,
                    }
                })
                .collect();

            tracing::info!("Returning {} reserves (from database)", reserves.len());

            (StatusCode::OK, Json(success_response(reserves)))
        }
        Err(e) => {
            tracing::error!("Failed to get reserves from database: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(crate::models::error_response(
                    "Failed to retrieve reserves from database".to_string(),
                )),
            )
        }
    }
}

/// Get reserves by issuer public key
#[axum::debug_handler]
pub async fn get_reserves_by_issuer(
    State(state): State<AppState>,
    axum::extract::Path(pubkey_hex): axum::extract::Path<String>,
) -> (StatusCode, Json<ApiResponse<Vec<SerializableReserveInfo>>>) {
    tracing::debug!("Getting reserves for issuer: {}", pubkey_hex);

    // Get reserve storage from scanner and query database directly
    let scanner = state.ergo_scanner.lock().await;
    let reserve_storage = scanner.reserve_storage();

    // Get all reserves from database and filter by issuer
    match reserve_storage.get_all_reserves() {
        Ok(all_reserves) => {
            let reserves: Vec<SerializableReserveInfo> = all_reserves
                .into_iter()
                .filter(|reserve| reserve.owner_pubkey == pubkey_hex)
                .map(|info| {
                    let collateralization_ratio = info.collateralization_ratio();
                    SerializableReserveInfo {
                        box_id: info.box_id,
                        owner_pubkey: decode_potentially_double_hex_encoded(&info.owner_pubkey),
                        collateral_amount: info.base_info.collateral_amount,
                        total_debt: info.total_debt,
                        tracker_nft_id: info.base_info.tracker_nft_id.clone(),
                        reserve_token_id: info.base_info.reserve_token_id.clone(),
                        last_updated_height: info.base_info.last_updated_height,
                        last_updated_timestamp: info.last_updated_timestamp,
                        collateralization_ratio,
                        refund_initiation_height: info.base_info.refund_initiation_height,
                    }
                })
                .collect();

            tracing::info!(
                "Returning {} reserves for issuer {} (from database)",
                reserves.len(),
                pubkey_hex
            );

            (StatusCode::OK, Json(success_response(reserves)))
        }
        Err(e) => {
            tracing::error!("Failed to get reserves from database: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(crate::models::error_response(
                    "Failed to retrieve reserves from database".to_string(),
                )),
            )
        }
    }
}

/// Get a specific reserve by box ID
#[axum::debug_handler]
pub async fn get_reserve_by_box_id(
    State(state): State<AppState>,
    axum::extract::Path(box_id): axum::extract::Path<String>,
) -> (
    StatusCode,
    Json<ApiResponse<Option<SerializableReserveInfo>>>,
) {
    tracing::debug!("Getting reserve by box ID: {}", box_id);

    // Get reserve storage from scanner and query database directly
    let scanner = state.ergo_scanner.lock().await;
    let reserve_storage = scanner.reserve_storage();

    // Get the specific reserve from database
    match reserve_storage.get_reserve(&box_id) {
        Ok(Some(reserve_info)) => {
            let collateralization_ratio = reserve_info.collateralization_ratio();
            let serializable_reserve = SerializableReserveInfo {
                box_id: reserve_info.box_id,
                owner_pubkey: decode_potentially_double_hex_encoded(&reserve_info.owner_pubkey),
                collateral_amount: reserve_info.base_info.collateral_amount,
                total_debt: reserve_info.total_debt,
                tracker_nft_id: reserve_info.base_info.tracker_nft_id.clone(),
                reserve_token_id: reserve_info.base_info.reserve_token_id.clone(),
                last_updated_height: reserve_info.base_info.last_updated_height,
                last_updated_timestamp: reserve_info.last_updated_timestamp,
                collateralization_ratio,
                refund_initiation_height: reserve_info.base_info.refund_initiation_height,
            };

            tracing::info!("Successfully retrieved reserve with box ID: {}", box_id);

            (
                StatusCode::OK,
                Json(success_response(Some(serializable_reserve))),
            )
        }
        Ok(None) => {
            tracing::info!("Reserve with box ID {} not found", box_id);
            (StatusCode::NOT_FOUND, Json(success_response(None)))
        }
        Err(e) => {
            tracing::error!("Failed to get reserve from database: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(crate::models::error_response(
                    "Failed to retrieve reserve from database".to_string(),
                )),
            )
        }
    }
}

/// Serializable quarantine record for API responses (issue #19).
#[derive(Debug, Serialize)]
pub struct SerializableQuarantinedReserve {
    pub box_id: String,
    pub owner_pubkey: String,
    pub denomination: String,
    pub reason: String,
    pub quarantined_at: u64,
}

/// List reserve boxes rejected by admission control (issue #19).
///
/// Quarantined boxes are never tracked: a duplicate carrying someone else's key
/// in R4 cannot block the victim, and the record tells the operator why the box
/// was rejected.
#[axum::debug_handler]
pub async fn get_quarantined_reserves(
    State(state): State<AppState>,
) -> (
    StatusCode,
    Json<ApiResponse<Vec<SerializableQuarantinedReserve>>>,
) {
    tracing::debug!("Getting quarantined reserves");

    let scanner = state.ergo_scanner.lock().await;
    let reserve_storage = scanner.reserve_storage();

    match reserve_storage.get_all_quarantined() {
        Ok(records) => {
            let quarantined: Vec<SerializableQuarantinedReserve> = records
                .into_iter()
                .map(|r| SerializableQuarantinedReserve {
                    box_id: r.box_id,
                    owner_pubkey: decode_potentially_double_hex_encoded(&r.owner_pubkey),
                    denomination: r.denomination,
                    reason: r.reason,
                    quarantined_at: r.quarantined_at,
                })
                .collect();

            (StatusCode::OK, Json(success_response(quarantined)))
        }
        Err(e) => {
            tracing::error!("Failed to get quarantined reserves from database: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(crate::models::error_response(
                    "Failed to retrieve quarantined reserves from database".to_string(),
                )),
            )
        }
    }
}

/// Serializable version of ExtendedReserveInfo for API responses
#[derive(Debug, Serialize)]
pub struct SerializableReserveInfo {
    pub box_id: String,
    pub owner_pubkey: String,
    pub collateral_amount: u64,
    pub total_debt: u64,
    pub tracker_nft_id: String, // Tracker NFT ID from R6 register (hex-encoded serialized SColl(SByte) format following byte_array_register_serialization.md spec)
    /// Reserve token ID (hex-encoded) for token-backed reserves, or empty for ERG reserves.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub reserve_token_id: String,
    pub last_updated_height: u64,
    pub last_updated_timestamp: u64,
    pub collateralization_ratio: f64,
    pub refund_initiation_height: u64,
}

/// Query parameters for GET /reserves/state-digest.
#[derive(Debug, serde::Deserialize)]
pub struct ReserveStateDigestQuery {
    /// Issuer (reserve owner) public key, hex-encoded 33 bytes.
    pub issuer_pubkey: String,
    /// Denomination: "erg" or "token:<64-hex>"; defaults to "erg".
    #[serde(default)]
    pub denomination: String,
}

/// Get the tracker's reserve AVL tree state for an (issuer, denomination)
/// pair: the 33-byte root digest and the serialized SAvlTree value that a
/// reserve (re)creation must carry in R5 (issue #15 review N9).
///
/// Clients use this to verify the R5 register of a `/reserves/create` payload
/// before signing it with their own node.
#[axum::debug_handler]
pub async fn get_reserve_state_digest(
    State(state): State<AppState>,
    axum::extract::Query(params): axum::extract::Query<ReserveStateDigestQuery>,
) -> (
    StatusCode,
    Json<ApiResponse<crate::models::ReserveStateDigestResponse>>,
) {
    let issuer_pubkey_bytes = match hex::decode(&params.issuer_pubkey) {
        Ok(bytes) => bytes,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(crate::models::error_response(
                    "issuer_pubkey must be hex-encoded".to_string(),
                )),
            )
        }
    };
    let issuer_pubkey: basis_store::PubKey = match issuer_pubkey_bytes.try_into() {
        Ok(arr) => arr,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(crate::models::error_response(
                    "issuer_pubkey must be 33 bytes".to_string(),
                )),
            )
        }
    };

    let denomination = if params.denomination.trim().is_empty() {
        basis_store::DenomKey::Erg
    } else {
        match state.config.parse_served_denomination(&params.denomination) {
            Ok(d) => d,
            Err(msg) => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(crate::models::error_response(format!(
                        "invalid denomination: {}",
                        msg
                    ))),
                )
            }
        }
    };

    let (digest_tx, digest_rx) = tokio::sync::oneshot::channel();
    if state
        .tx
        .send(crate::TrackerCommand::GetReserveStateDigest {
            issuer_pubkey,
            denomination,
            response_tx: digest_tx,
        })
        .await
        .is_err()
    {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(crate::models::error_response(
                "Tracker thread unavailable".to_string(),
            )),
        );
    }
    let digest = match digest_rx.await {
        Ok(digest) => digest,
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(crate::models::error_response(
                    "Tracker thread did not respond".to_string(),
                )),
            )
        }
    };

    let reserve_r5 = hex::encode(basis_offchain::ergo_tx::build_savl_tree_from_digest(
        &hex::encode(&digest),
    ));
    (
        StatusCode::OK,
        Json(success_response(
            crate::models::ReserveStateDigestResponse {
                issuer_pubkey: params.issuer_pubkey,
                denomination: denomination.denomination_id(),
                reserve_state_digest: hex::encode(digest),
                reserve_r5,
            },
        )),
    )
}
