use super::*;
use crate::game::{roller::Roller, types::GameEvent};

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

#[test]
fn test_join_game() {
    let (mut game, host_id) = setup_game();
    let guest_id = PlayerId::new();

    // 1. Join successfully
    assert!(game.join(guest_id).is_ok());
    assert_eq!(game.get_players().len(), 2);
    assert_eq!(*game.get_status(), GameStatus::InProgress);

    // 2. Join failed (Game Full)
    let intruder_id = PlayerId::new();
    let err = game.join(intruder_id);
    assert_eq!(err, Err(GameError::GameFull));
}

#[test]
fn test_roll_flow() {
    let (mut game, host_id) = setup_game();
    let guest_id = PlayerId::new();
    game.join(guest_id).unwrap();

    // 1. Fail: Wrong Turn
    let mut roller = MockRoller {
        value_to_return: 500,
    };
    let err = game.roll(guest_id, &mut roller);
    assert_eq!(err, Err(GameError::NotYourTurn));

    // 2. Success: Host Rolls
    let events = game.roll(host_id, &mut roller).unwrap();
    assert_eq!(events.len(), 1);
    match events[0] {
        GameEvent::Rolled { player_id, value } => {
            assert_eq!(player_id, host_id);
            assert_eq!(value, 500);
        }
        _ => panic!("Unexpected event"),
    }

    // State Check
    assert_eq!(game.get_current_max(), 500);
    assert_eq!(game.get_current_player(), Some(&guest_id)); // Turn passed
}

#[test]
fn test_game_over_loss() {
    let (mut game, host_id) = setup_game();
    let guest_id = PlayerId::new();
    game.join(guest_id).unwrap();

    let mut roller = MockRoller { value_to_return: 1 }; // Rolling 1 = Loss

    let events = game.roll(host_id, &mut roller).unwrap();

    // Should have Rolled AND GameOver events
    assert_eq!(events.len(), 2);
    assert!(matches!(
        events[1],
        GameEvent::GameOver {
            winner_id: _,
            loser_id: _
        }
    ));

    match *game.get_status() {
        GameStatus::PlayerLost(pid) => assert_eq!(pid, host_id),
        _ => panic!("Game status should be PlayerLost"),
    }

    // Attempt to roll after game over
    let err = game.roll(guest_id, &mut roller);
    assert_eq!(err, Err(GameError::GameFinished));
}

#[test]
fn test_disconnect_and_reconnect() {
    let (mut game, host_id) = setup_game();
    let guest_id = PlayerId::new();
    game.join(guest_id).unwrap();

    // 1. Pause Game
    game.pause_game(host_id).unwrap();
    assert!(matches!(game.get_status(), GameStatus::PausedForReconnect(pid) if *pid == host_id));

    // 2. Attempt Roll while Paused (Should Fail)
    let mut roller = MockRoller {
        value_to_return: 500,
    };
    let err = game.roll(guest_id, &mut roller);
    assert_eq!(err, Err(GameError::GamePaused));

    // 3. Wrong Player Reconnect (Should Fail)
    let err = game.reconnect(guest_id);
    assert_eq!(err, Err(GameError::GameFull)); // Or specific error if logic changed

    // 4. Correct Player Reconnect
    game.reconnect(host_id).unwrap();
    assert_eq!(*game.get_status(), GameStatus::InProgress);
}
