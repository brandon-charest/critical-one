use super::types::{GameError, GameId, GameStatus, PlayerId};
use super::roller::Roller;
use serde::{Deserialize, Serialize};


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Game {
    pub id: GameId,
    pub players: Vec<PlayerId>,
    pub current_max: u32,
    pub turn_index: usize,
    pub status: GameStatus,
}


impl Game {
    pub fn new(host_id: PlayerId) -> Self {
        Self {
            id: GameId::new(),
            players: vec![host_id],
            current_max: 1000,
            turn_index: 0,
            status: GameStatus::WaitingForPlayers,
        }
    }

    pub fn current_player(&self) -> Option<&PlayerId> {
        self.players.get(self.turn_index)
    }

    pub fn next_turn(&mut self) {
        self.turn_index = (self.turn_index + 1) % self.players.len();
    }

    pub fn handle_roll(&mut self, roll_result: u32) {
        if self.status != GameStatus::InProgress {
            return
        }

        if roll_result == 1 {
            self.status = GameStatus::PlayerLost(self.current_player().unwrap().clone());
        } else {
            self.current_max = roll_result;
            self.next_turn();
        }

    }

    pub fn roll(&mut self, roller: &mut impl Roller) -> Result<u32, GameError> {
        if self.status != GameStatus::InProgress {
            return Err(GameError::GameFinished);
        }

        let roll_result = roller.roll_in_range(self.current_max);
        self.handle_roll(roll_result);
        Ok(roll_result)
    }

    
}
