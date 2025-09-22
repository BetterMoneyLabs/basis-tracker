use axum::{routing::{get, post}, Router, extract::Path, Json};
use std::net::SocketAddr;

#[derive(serde::Serialize)]
struct ApiResponse<T> {
    success: bool,
    data: T,
    error: Option<String>,
}

#[tokio::main]
async fn main() {
    let app = Router::new()
        .route("/", get(|| async { "Hello, World!" }))
        .route("/notes", post(|| async { 
            Json(ApiResponse { success: true, data: (), error: None })
        }))
        .route("/notes/issuer/{pubkey}", get(|Path(pubkey): Path<String>| async { 
            println!("GET /notes/issuer/{} called", pubkey);
            Json(ApiResponse { success: true, data: vec!["test"], error: None })
        }));
    
    let addr = SocketAddr::from(([127, 0, 0, 1], 3006));
    println!("Starting server on {}", addr);
    
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    println!("Server listening on {}", addr);
    
    axum::serve(listener, app).await.unwrap();
}