use axum::extract::Path;
use axum::routing::get;
use axum::{Json, Router};
use serde_json::json;

async fn user(Path(id): Path<String>) -> Json<serde_json::Value> {
    Json(json!({ "id": id }))
}

#[tokio::main]
async fn main() {
    let app = Router::new().route("/user/:id", get(user));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:8081")
        .await
        .expect("bind 8081");
    println!("rust axum server on http://127.0.0.1:8081");
    axum::serve(listener, app).await.expect("serve");
}