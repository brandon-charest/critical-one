pub mod config;
pub mod data;
pub mod error;
pub mod game;
pub mod handlers;
pub mod state; 

use config::Config;
use state::{AppState, GameSessionManager};
use axum::{
    Router, 
    extract::ws::WebSocket, 
    routing::{get, post}
};
use game::{GameId};
use std::sync::Arc;
use tower_http::trace::{DefaultMakeSpan, TraceLayer};


pub fn create_app(config: Config) -> Router {
    let client = redis::Client::open(config.database.redis_url.clone())
        .expect("Invalid Redis URL");
    
    let state = Arc::new(AppState {
        redis_client: client,
        session_manager: GameSessionManager::default(),
        config: Arc::new(config),
    });

    Router::new()
        .route("/health", get(|| async { "OK" }))
        .route("/game", post(handlers::create_game_handler))
        .route("/game/{id}", get(handlers::get_game_handler))
        .route("/game/{id}/join", post(handlers::join_game_handler))
        //.route("/ws/game/{id}", get(websocket_handler)) 
        .with_state(state)
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(DefaultMakeSpan::default().include_headers(true))
        )
}


async fn handle_socket(socket: WebSocket, game_id: GameId, state: Arc<AppState>) {
    // TODO: finish me!
    tracing::info!(game_id = %game_id, "WebSocket connection established...");
}


#[cfg(test)]
mod tests {
    use super::*;
    use tower::ServiceExt;
    use crate::config::{
        Config, 
        ServerConfig, 
        DatabaseConfig, 
        LoggingConfig
    };
    use axum::{
        http::{StatusCode, Request},
        body::Body,
    };


    fn test_config() -> Config {
        Config {
            server: ServerConfig { addr: "0.0.0.0:0".to_string() },
            database: DatabaseConfig { redis_url: "redis://127.0.0.1:6379/".to_string() },
            logging: LoggingConfig { level: "info".to_string() },
        }
    }

    #[tokio::test]
    async fn test_create_app_initialization() {
        let config = test_config();
        let app = create_app(config.clone());
        let response = app
            .oneshot(Request::builder().uri("/health").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        assert_eq!(&body[..], b"OK");
    }

    #[tokio::test]
    async fn test_create_app_redis_client_connection() {
        let config = test_config();
        let _ = create_app(config.clone());
        let client = redis::Client::open(config.database.redis_url.clone())
            .expect("Invalid Redis URL in test");
            
        let conn_result = client.get_multiplexed_async_connection().await;
        assert!(conn_result.is_ok(), "Failed to connect to Redis. Ensure Redis server is running on 127.0.0.1:6379 for this test.");
    }
}