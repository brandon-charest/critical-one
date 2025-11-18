pub mod config;
pub mod game;

use config::Config;
use axum::{
    Json, Router, extract::{State}, http::StatusCode, routing::{get, post}
};
use game::{Game, GameError, GameId, GameStatus, PlayerId};
use tracing_subscriber::fmt::format;
use std::sync::Arc;
use serde::{Deserialize, Serialize};
use tower_http::trace::{DefaultMakeSpan, TraceLayer};
use redis::AsyncTypedCommands;

#[derive(Debug,Deserialize)]
pub struct CreateGameRequest {
    pub host_id: Option<PlayerId>, 
}

#[derive(Serialize)]
pub struct CreateGameResponse {
    pub game_id: GameId,
    pub host_id: PlayerId,
}

pub struct AppState {
    pub redis_client: redis::Client,
}

pub type SharedState = Arc<AppState>;


pub fn create_app(config: Config) -> Router {
    let client = redis::Client::open(config.database.redis_url.clone()).expect("Invalid Redis URL");

    let state = Arc::new(AppState {
        redis_client: client,
    });

    Router::new()
        .route("/health", get(|| async { "OK" }))
        .route("/game", post(create_game_handler))
        //.route("/game/:id", get(get_game_handler))
        .with_state(state)
}

#[tracing::instrument(skip(state))]
async fn create_game_handler(
    State(state): State<SharedState>, 
    Json(payload): Json<CreateGameRequest>
) -> Result<(StatusCode, Json<CreateGameResponse>), (StatusCode, String)> {

    tracing::info!(host_id = ?payload.host_id, "Attempting to create game");
    let host_id: PlayerId = payload.host_id.unwrap_or(PlayerId::new());
    let new_game: Game = Game::new(host_id);
    let game_id: GameId = new_game.id;

    let game_json: String = match serde_json::to_string(&new_game) {
        Ok(json) => json,
        Err(e) => {
            tracing::error!("Failed to serialize game: {}", e);
            return Err((StatusCode::INTERNAL_SERVER_ERROR, e.to_string()));
        }
    };

    tracing::debug!("Getting Redis connection");
    let mut conn = state.redis_client.get_multiplexed_async_connection().await
        .map_err(|e| {
            tracing::error!("Failed to get Redis connection: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
        })?;
 
    tracing::debug!(game_json = %game_json, "Saving game to Redis");
    let key = format!("game:{}", game_id);
    let _: () = conn.set_ex(&key, &game_json, 3600).await
        .map_err(|e| {
            tracing::error!(game_id = %game_id, "Failed to save game to Redis: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
        })?;

    let response = CreateGameResponse {
        game_id,
        host_id,
    };

    tracing::info!(game_id = %game_id, host_id = %host_id, "Game created successfully");
    Ok((StatusCode::CREATED, Json(response)))
}