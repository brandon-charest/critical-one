use serde_json::Value;
use std::{collections::HashMap, sync::Arc};
use tokio::sync::{mpsc, RwLock};

use crate::config::Config;
use crate::data::GameRepository;
use crate::game::{GameId, PlayerId};

#[derive(Debug, Clone)]
pub struct GameMessage {
    pub r#type: String, // Use r#type because 'type' is a reserved keyword
    pub payload: Value,
}

pub type PlayerSender = mpsc::UnboundedSender<GameMessage>;

#[derive(Debug, Default)]
pub struct GameSession {
    // Maps PlayerId to their WebSocket sender channel
    pub players: RwLock<HashMap<PlayerId, PlayerSender>>,
}

pub struct GameSessionManager {
    // Maps GameId to the in-memory GameSession struct.
    pub sessions: RwLock<HashMap<GameId, Arc<GameSession>>>,
}

impl Default for GameSessionManager {
    fn default() -> Self {
        Self {
            sessions: RwLock::new(HashMap::new()),
        }
    }
}

pub struct AppState {
    pub repository: Arc<dyn GameRepository>,
    pub session_manager: GameSessionManager,
    pub config: Arc<Config>,
}

pub type SharedState = Arc<AppState>;
