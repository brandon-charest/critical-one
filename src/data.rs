use redis::{AsyncCommands};
use serde::{Deserialize, Serialize};
use tracing::instrument;
use serde_json;

use crate::game::{Game, GameId, PlayerId};
use crate::error::AppError;
use crate::state::SharedState;

// --- DTOs (Data Transfer Objects) ---
#[derive(Debug, Deserialize)]
pub struct CreateGameRequest {
    pub host_id: Option<PlayerId>, 
}

#[derive(Serialize)]
pub struct CreateGameResponse {
    pub game_id: GameId,
    pub host_id: PlayerId,
    //pub invite_code: String,
}

// --- Data Service Methods (Repository) ---
#[instrument(skip_all)]
pub async fn load_game(state: &SharedState, game_id: GameId) -> Result<Game, AppError> {
    let mut conn = state.redis_client.get_multiplexed_async_connection().await?;
    let key = format!("game:{}", game_id);
    
    let game_json: String = conn.get(&key).await
        .map_err(|e| {
            if e.kind() == redis::ErrorKind::ResponseError && format!("{:?}", e).contains("nil") {
                AppError::GameNotFound(game_id)
            } else {
                AppError::Redis(e)
            }
        })?;
    
    let game: Game = serde_json::from_str(&game_json)?;
    Ok(game)
}

#[instrument(skip_all)]
pub async fn save_game(state: &SharedState, game: &Game) -> Result<(), AppError> {
    let mut conn = state.redis_client.get_multiplexed_async_connection().await?;
    let key = format!("game:{}", game.get_id());
    let game_json = serde_json::to_string(game)?;
    
    conn.set_ex::<_, _, ()>(&key, game_json, 86400).await?; 
    Ok(())
}