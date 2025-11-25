use async_trait::async_trait;
use redis::AsyncCommands;
use serde::{Deserialize, Serialize};
use serde_json;

use crate::error::AppError;
use crate::game::{Game, GameId, PlayerId};

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

#[derive(Debug, Deserialize)]
pub struct JoinGameRequest {
    pub player_id: Option<PlayerId>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", content = "payload", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ClientMessage {
    Connect { player_id: PlayerId },
    Roll,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(tag = "type", content = "payload", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ServerMessage {
    GameState(Game),
    Error {
        message: String,
    },
    PlayerJoined {
        player_id: PlayerId,
    },
    RollResult {
        player_id: PlayerId,
        rolled_value: u32,
    },
    GameStarted {
        game: Game,
    },
    GameOver {
        winner_id: PlayerId,
        loser_id: PlayerId,
    },
}

#[async_trait]
pub trait GameRepository: Send + Sync {
    async fn load_game(&self, game_id: GameId) -> Result<Game, AppError>;
    async fn save_game(&self, game: &Game) -> Result<(), AppError>;
}

pub struct RedisRepository {
    redis_client: redis::Client,
}

impl RedisRepository {
    pub fn new(redis_client: redis::Client) -> Self {
        Self { redis_client }
    }
}

#[async_trait]
impl GameRepository for RedisRepository {
    async fn load_game(&self, game_id: GameId) -> Result<Game, AppError> {
        let mut conn = self.redis_client.get_multiplexed_async_connection().await?;
        let key = format!("game:{}", game_id);

        let game_json: String = conn.get(&key).await.map_err(|e| {
            if e.kind() == redis::ErrorKind::ResponseError && format!("{:?}", e).contains("nil") {
                AppError::GameNotFound(game_id)
            } else {
                AppError::Redis(e)
            }
        })?;

        let game: Game = serde_json::from_str(&game_json)?;
        Ok(game)
    }

    async fn save_game(&self, game: &Game) -> Result<(), AppError> {
        let mut conn = self.redis_client.get_multiplexed_async_connection().await?;
        let key = format!("game:{}", game.get_id());
        let game_json = serde_json::to_string(game)?;

        conn.set_ex::<_, _, ()>(&key, game_json, 86400).await?;
        Ok(())
    }
}

// --- Mock Implementation (For Tests) ---

use std::collections::HashMap;
use tokio::sync::RwLock;

pub struct MockGameRepository {
    storage: RwLock<HashMap<GameId, Game>>,
}

impl MockGameRepository {
    pub fn new() -> Self {
        Self {
            storage: RwLock::new(HashMap::new()),
        }
    }
}

#[async_trait]
impl GameRepository for MockGameRepository {
    async fn load_game(&self, game_id: GameId) -> Result<Game, AppError> {
        let store = self.storage.read().await;
        store
            .get(&game_id)
            .cloned()
            .ok_or(AppError::GameNotFound(game_id))
    }

    async fn save_game(&self, game: &Game) -> Result<(), AppError> {
        let mut store = self.storage.write().await;
        store.insert(game.get_id(), game.clone());
        Ok(())
    }
}
