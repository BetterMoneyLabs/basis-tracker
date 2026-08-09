//! Structural tombstones for the retired tracker-assisted v1 redemption API.
//!
//! This module deliberately contains no transaction construction, proof,
//! signing, node submission, broadcast, or state-mutation implementation.

use axum::{http::StatusCode, Json};

use crate::models::ApiResponse;

/// Retired v1 build endpoint. V2 has a separate exact-manifest boundary and is
/// intentionally not activated by this compatibility route.
#[axum::debug_handler]
pub async fn build_redemption() -> (StatusCode, Json<ApiResponse<()>>) {
    crate::reject_retired_v1_redemption()
}

/// Retired v1 submit endpoint. There is no node or broadcast client in this
/// module.
#[axum::debug_handler]
pub async fn submit_redemption() -> (StatusCode, Json<ApiResponse<()>>) {
    crate::reject_retired_v1_redemption()
}
