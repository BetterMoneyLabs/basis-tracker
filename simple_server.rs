use axum::{routing::get, Router};
use std::net::SocketAddr;

#[tokio::main]
async fn main() {
    let app = Router::new().route("/", get(|| async { "Hello, World!" }));
    
    let addr = SocketAddr::from(([127, 0, 0, 1], 3002));
    println!("Starting server on {}", addr);
    
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    println!("Server listening on {}", addr);
    
    axum::serve(listener, app).await.unwrap();
}