use axum::{
    Json,
    extract::{State, Path}, 
    http::StatusCode,
};

use tracing::instrument;


use crate::data::{CreateGameRequest, CreateGameResponse}; 
use crate::error::AppError;
use crate::data::{load_game, save_game};
use crate::game::{Game, GameId, PlayerId, GameStatus};
use crate::state::SharedState;

#[instrument(skip(state))]
pub async fn create_game_handler(
    State(state): State<SharedState>,
    Json(payload): Json<CreateGameRequest>,
) -> Result<(StatusCode, Json<CreateGameResponse>), AppError> {

    tracing::info!(host_id = ?payload.host_id, "Attempting to create game");
    
    // 1. Logic
    let host_id = payload.host_id.unwrap_or_else(PlayerId::new);
    let new_game = Game::new(host_id);
    let game_id = new_game.get_id();

    // 2. Persistence (using helper)
    save_game(&state, &new_game).await?;

    let response = CreateGameResponse { 
        game_id, 
        host_id,
    };
    
    tracing::info!(game_id = %game_id, host_id = %host_id, "Game created successfully");
    
    Ok((StatusCode::CREATED, Json(response)))
}

#[instrument(skip(state))]
pub async fn get_game_handler(
    State(state): State<SharedState>,
    Path(game_id): Path<GameId>,
) -> Result<Json<Game>, AppError> {
    let game = load_game(&state, game_id).await?;
    Ok(Json(game))
}

#[instrument(skip(state))]
pub async fn join_game_handler(
    State(state): State<SharedState>,
    Path(game_id): Path<GameId>,
    Json(payload): Json<CreateGameRequest>,
) -> Result<Json<Game>, AppError> {

    let mut game = load_game(&state, game_id).await?;

    let joining_player = payload.host_id.unwrap_or_else(PlayerId::new);
    
    // 1. Determine action based on status (Join if Waiting, Reconnect otherwise)
    let join_result = match *game.get_status() {
        // Join if Waiting
        GameStatus::WaitingForPlayers => game.join(joining_player),
        // Reconnect
        GameStatus::PausedForReconnect(_) => game.reconnect(joining_player),
        // If InProgress, player is trying to join a game they are already in or it's full.
        // Game::join will return GameError::GameFull if a third player attempts to join.
        _ => game.reconnect(joining_player),
    };

    join_result?;
    save_game(&state, &game).await?;

    tracing::info!(game_id = %game_id, player_id = %joining_player, "Player joined/reconnected successfully.");
    Ok(Json(game))
}

