pub mod game;
pub mod roller;

use axum::{Router, routing::get};
pub fn create_app() -> Router {
    Router::new()
        .route("/health", get(|| async { "OK" }))
}