
pub trait Roller {
    fn roll_in_range(&mut self, max: u32) -> u32;
}

pub struct ThreadRngRoller {
    rng: rand::rngs::ThreadRng,
}

impl ThreadRngRoller {
    pub fn new() -> Self {
        Self {
            rng: rand::rng(),
        }
    }
}

impl Roller for ThreadRngRoller {
    fn roll_in_range(&mut self, max: u32) -> u32 {
        use rand::Rng; // We only use the Rng trait here
        
        // NOTE: If your compiler still shows a deprecation warning for this line,
        // simply change `gen_range` to `random_range`. This is now the ONLY
        // place you would ever need to make that change.
        self.rng.random_range(1..=max)
    }
}


#[derive(Debug, PartialEq, Clone)]
pub enum GameStatus {
    InProgress,
    PlayerLost(String),
}

#[derive(Debug, Clone)]
pub struct Game {
    players: Vec<String>,
    current_max: u32,
    turn_index: usize,
    status: GameStatus,
}


impl Game {
    pub fn new(players: Vec<String>) -> Self {
        Self {
            players,
            current_max: 1000,
            turn_index: 0,
            status: GameStatus::InProgress,
        }
    }

    pub fn current_player(&self) -> Option<&String> {
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

    
    fn setup_game() -> Game {
        let players = vec!["Player1".to_string(), "Player2".to_string()];
        Game::new(players)
    }

    #[test]
    fn test_new_game_initialization() {
        let players = vec!["Player1".to_string(), "Player2".to_string()];
        let game = setup_game();
        assert_eq!(game.players, players);
        assert_eq!(game.current_max, 1000);
        assert_eq!(game.turn_index, 0);
        assert_eq!(game.status, GameStatus::InProgress);
    }

    #[test]
    fn test_player_loses_on_roll_of_1() {
        let mut game = setup_game();

        let mut mock_roll = MockRoller { value_to_return: 1 };

        game.roll(&mut mock_roll);
        assert_eq!(game.status, GameStatus::PlayerLost("Player1".to_string()));
    }

    #[test]
    fn test_game_progresses_on_valid_roll() {
        let mut game = setup_game();
        let mut mock_roll = MockRoller { value_to_return:500 };

        game.roll(&mut mock_roll);
        assert_eq!(game.status, GameStatus::InProgress);
        assert_eq!(game.current_max, 500);
        assert_eq!(game.turn_index, 1); // Turn should advance to Player2
    }
}