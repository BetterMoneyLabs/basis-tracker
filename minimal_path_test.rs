use axum::{routing::get, Router, extract::Path};
use std::net::SocketAddr;

#[tokio::main]
async fn main() {
    let app = Router::new()
        .route("/", get(|| async { "Hello, World!" }))
        .route("/test/:param", get(|Path(param): Path<String>| async move {
            format!("Received param: {}", param)
        }));
    
    let addr = SocketAddr::from(([127, 0, 0, 1], 3004));
    println!("Starting server on {}", addr);
    
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    println!("Server listening on {}", addr);
    
    axum::serve(listener, app).await.unwrap();
}