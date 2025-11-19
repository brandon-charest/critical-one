use super::*;
use crate::game::roller::Roller;

struct MockRoller {
    value_to_return: u32,
}

impl Roller for MockRoller {
    fn roll_in_range(&mut self, _max: u32) -> u32 {
        self.value_to_return
    }
}

fn setup_game() -> (Game, PlayerId) {
    let host_id = PlayerId::new();
    (Game::new(host_id), host_id)
}

#[test]
fn test_new_game_initial_state() {
    let (game, host_id) = setup_game();
    assert_eq!(game.get_players().len(), 1);
    assert_eq!(game.get_current_max(), 1000);
    assert_eq!(game.get_current_player(), Some(&host_id));
    assert_eq!(*game.get_status(), GameStatus::WaitingForPlayers);
}
