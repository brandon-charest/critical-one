use core::fmt;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)] // Serialize directly as the inner UUID string
pub struct PlayerId(Uuid);

impl PlayerId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl fmt::Display for PlayerId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct GameId(Uuid);

impl GameId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl fmt::Display for GameId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum GameStatus {
    WaitingForPlayers,
    InProgress,
    PlayerLost(PlayerId), 
}

pub enum GameError {
    GameFinished,
    NotYourTurn,
    GameFull,
}

#[cfg(test)]
mod tests {
    use super::*; // Import all the types from the file
    use uuid::Uuid;

    const NIL_UUID: Uuid = Uuid::nil(); // 0000...

    #[test]
    fn test_player_id_display() {
        let player_id = PlayerId(NIL_UUID);
        assert_eq!(player_id.to_string(), "00000000-0000-0000-0000-000000000000");
    }

    #[test]
    fn test_game_id_display() {
        let game_id = GameId(NIL_UUID);
        assert_eq!(game_id.to_string(), "00000000-0000-0000-0000-000000000000");
    }

    #[test]
    fn test_new_ids_are_unique() {
        let p1 = PlayerId::new();
        let p2 = PlayerId::new();
        assert_ne!(p1.0, p2.0);
    }
}