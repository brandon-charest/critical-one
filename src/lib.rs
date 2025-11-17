pub mod game;
use axum::{
    Json, Router, extract::{State}, http::StatusCode, routing::{get, post}
};
use game::{Game, GameError, GameId, GameStatus, PlayerId};
use std::sync::Arc;
use serde::{Deserialize, Serialize};



#[derive(Deserialize)]
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


pub fn create_app() -> Router {
    let client = redis::Client::open("redis://127.0.0.1:6379/").expect("Invalid Redis URL");

    let state = Arc::new(AppState {
        redis_client: client,
    });

    Router::new()
        .route("/health", get(|| async { "OK" }))
        .route("/game", post(create_game_handler))
        //.route("/game/:id", get(get_game_handler))
        .with_state(state)
}

async fn create_game_handler(
    State(state): State<SharedState>, 
    Json(payload): Json<CreateGameRequest>
) -> (StatusCode, Json<CreateGameResponse>) {

    let host_id: PlayerId = payload.host_id.unwrap_or(PlayerId::new());
    let new_game = Game::new(host_id);

    let response = CreateGameResponse {
        game_id: new_game.id,
        host_id,
    };

    (StatusCode::CREATED, Json(response))
}