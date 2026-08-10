//! Structural regression tests for the nine retired v1 redemption routes.

use axum::{
    body::Body,
    http::{Method, Request, StatusCode},
};
use basis_server::retired_v1_redemption_routes;
use tower::ServiceExt;

#[tokio::test]
async fn every_retired_route_returns_gone_before_body_or_query_parsing() {
    // The router is intentionally constructible without AppState. That makes
    // actor, storage, scanner, node, signer, and broadcast effects unreachable.
    let app = retired_v1_redemption_routes::<()>();
    let routes = [
        (Method::POST, "/redeem"),
        (Method::POST, "/redeem/complete"),
        (Method::GET, "/proof/redemption?issuer_pubkey=not-hex"),
        (Method::GET, "/tracker/proof?issuer_pubkey=not-hex"),
        (Method::GET, "/reserve/proof?issuer_pubkey=not-hex"),
        (Method::POST, "/tracker/signature"),
        (Method::POST, "/redemption/prepare"),
        (Method::POST, "/redemption/build"),
        (Method::POST, "/redemption/submit"),
    ];

    for (method, uri) in routes {
        let request = Request::builder()
            .method(method)
            .uri(uri)
            .header("content-type", "application/json")
            .body(Body::from("{"))
            .unwrap();
        let response = app.clone().oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::GONE, "route {uri}");

        let bytes = axum::body::to_bytes(response.into_body(), 16 * 1024)
            .await
            .unwrap();
        let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(body["success"], false, "route {uri}");
        assert!(body["data"].is_null(), "route {uri}");
        assert!(
            body["error"]
                .as_str()
                .unwrap_or_default()
                .contains("v1 redemption is retired"),
            "route {uri}"
        );
    }
}

#[tokio::test]
async fn absent_generic_proof_route_is_not_a_tombstone_alias() {
    let response = retired_v1_redemption_routes::<()>()
        .oneshot(
            Request::builder()
                .uri("/proof")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}
