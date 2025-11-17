use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::roller::Roller;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)] // Serialize directly as the inner UUID string
pub struct PlayerId(Uuid);

impl PlayerId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum GameStatus {
    InProgress,
    PlayerLost(PlayerId), 
}

pub enum GameError {
    GameFinished,
    NotYourTurn,
}

#[derive(Debug, Clone)]
pub struct Game {
    id: GameId,
    players: Vec<PlayerId>,
    current_max: u32,
    turn_index: usize,
    status: GameStatus,
}


impl Game {
    pub fn new(players: Vec<PlayerId>) -> Self {
        Self {
            id: GameId::new(),
            players,
            current_max: 1000,
            turn_index: 0,
            status: GameStatus::InProgress,
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

    pub fn roll(&mut self, roller: &mut impl Roller) -> u32 {
        let roll_result = roller.roll_in_range(self.current_max);
        self.handle_roll(roll_result);
        roll_result
    }

    
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockRoller {
        value_to_return: u32,
    }

    impl Roller for MockRoller {
        fn roll_in_range(&mut self, _max: u32) -> u32 {
            self.value_to_return
        }
    }

    
    fn setup_game() -> (Game, PlayerId, PlayerId) {
        let p1 = PlayerId::new();
        let p2 = PlayerId::new();
        let players: Vec<PlayerId> = vec![p1, p2];
        (Game::new(players), p1, p2)
    }

    #[test]
    fn test_new_game_initialization() {
        let (game, p1, p2) = setup_game();
        assert_eq!(game.players, vec![p1, p2]);
        assert_eq!(game.current_max, 1000);
        assert_eq!(game.turn_index, 0);
        assert_eq!(game.status, GameStatus::InProgress);
    }

    #[test]
    fn test_player_loses_on_roll_of_1() {
        let (mut game, p1, p2) = setup_game();

        let mut mock_roll = MockRoller { value_to_return: 1 };

        assert_eq!(game.status, GameStatus::InProgress);
        game.roll(&mut mock_roll);
        assert_eq!(game.status, GameStatus::PlayerLost(p1));
    }

    #[test]
    fn test_game_progresses_on_valid_roll() {
        let (mut game, p1, p2) = setup_game();
        let mut mock_roll = MockRoller { value_to_return:500 };

        assert_eq!(game.current_player(), Some(&p1));
        assert_eq!(game.status, GameStatus::InProgress);

        game.roll(&mut mock_roll);
        assert_eq!(game.status, GameStatus::InProgress);
        assert_eq!(game.current_max, 500);
        assert_eq!(game.turn_index, 1); // Turn should advance to Player2
        assert_eq!(game.current_player(), Some(&p2));

        mock_roll.value_to_return = 200;
        game.roll(&mut mock_roll);
        assert_eq!(game.status, GameStatus::InProgress);
        assert_eq!(game.current_max, 200);
        assert_eq!(game.turn_index, 0); // Turn should advance to Player1
        assert_eq!(game.current_player(), Some(&p1));
    }
}