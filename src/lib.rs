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
        .route("/game/:id", get(handlers::get_game_handler))
        .route("/game/:id/join", post(handlers::get_game_handler))
        //.route("/ws/game/:id", get(websocket_handler)) 
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

