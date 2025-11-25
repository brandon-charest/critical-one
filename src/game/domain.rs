use crate::game::types::GameEvent;

use super::roller::Roller;
use super::types::{GameError, GameId, GameStatus, PlayerId};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Game {
    id: GameId,
    players: Vec<PlayerId>,
    current_max: u32,
    turn_index: usize,
    status: GameStatus,
}

impl Game {
    #[tracing::instrument]
    pub fn new(host_id: PlayerId) -> Self {
        Self {
            id: GameId::new(),
            players: vec![host_id],
            current_max: 1000, // TODO: make configurable
            turn_index: 0,     // TODO: is there a better way to handle this?
            status: GameStatus::WaitingForPlayers,
        }
    }

    // Getters
    pub fn get_id(&self) -> GameId {
        self.id
    }

    pub fn get_status(&self) -> &GameStatus {
        &self.status
    }

    pub fn get_current_max(&self) -> u32 {
        self.current_max
    }

    pub fn get_players(&self) -> &[PlayerId] {
        &self.players
    }

    pub fn get_current_player(&self) -> Option<&PlayerId> {
        self.players.get(self.turn_index)
    }

    //  --- Public mutators ---
    #[tracing::instrument(skip(self))]
    pub fn join(&mut self, player_id: PlayerId) -> Result<(), GameError> {
        if self.status != GameStatus::WaitingForPlayers {
            return Err(GameError::GameFull);
        }
        self.players.push(player_id);

        if self.players.len() == 2 {
            self.status = GameStatus::InProgress;
        } else {
            tracing::warn!("join called when game is not waiting for players");
        }
        Ok(())
    }

    #[tracing::instrument(skip(self))]
    pub fn reconnect(&mut self, player_id: PlayerId) -> Result<(), GameError> {
        match self.status {
            GameStatus::PausedForReconnect(disconnected_player) => {
                if disconnected_player == player_id {
                    tracing::info!(game_id = %self.id, player_id = %player_id, "Player reconnected. Resuming game.");
                    self.status = GameStatus::InProgress;
                    Ok(())
                } else {
                    Err(GameError::GameFull)
                }
            }
            GameStatus::InProgress => Ok(()),
            _ => Err(GameError::GamePaused),
        }
    }

    #[tracing::instrument(skip(self))]
    pub fn pause_game(&mut self, disconnected_player: PlayerId) -> Result<(), GameError> {
        if self.status != GameStatus::InProgress {
            return Err(GameError::GameFinished);
        }

        self.status = GameStatus::PausedForReconnect(disconnected_player);
        tracing::warn!(game_id = %self.id, player = %disconnected_player, "Game paused due to player disconnect.");
        Ok(())
    }

    #[tracing::instrument(skip(self, roller))]
    pub fn roll(&mut self, player_id: PlayerId, roller: &mut impl Roller) -> Result<Vec<GameEvent>, GameError> {
        match self.status {
            GameStatus::InProgress => {} // OK to proceed
            GameStatus::WaitingForPlayers => return Err(GameError::NotEnoughPlayers),
            GameStatus::PausedForReconnect(_) => return Err(GameError::GamePaused),
            GameStatus::PlayerLost(_) => return Err(GameError::GameFinished),
        }

        // check if roll is by current player!
        if self.get_current_player() != Some(&player_id) {
            return Err(GameError::NotYourTurn);
        }

        let roll_result = roller.roll_in_range(self.current_max);
        let mut events = vec![];

        self.handle_roll(player_id, roll_result, &mut events);

        Ok(events)
    }

    //  --- Private helpers ---
    fn next_turn(&mut self) {
        self.turn_index = (self.turn_index + 1) % self.players.len();
    }

    #[tracing::instrument(skip(self))]
    fn handle_roll(&mut self, player_id: PlayerId, roll_result: u32, events: &mut Vec<GameEvent>) {
        if self.status != GameStatus::InProgress {
            return;
        }

        events.push(GameEvent::Rolled { player_id, value: roll_result });

        // Game Over Logic
        if roll_result == 1 {
            self.status = GameStatus::PlayerLost(player_id);

            // Calculate winner (the other player)
            let winner_id = self
                .players
                .iter()
                .find(|&p| *p != player_id)
                .cloned()
                .unwrap_or(player_id); // Fallback should never happen in 2p game

            // Event: Game Over
            events.push(GameEvent::GameOver { winner_id, loser_id: player_id });
        } else {
            self.current_max = roll_result;
            self.next_turn();
        }
    }
}
