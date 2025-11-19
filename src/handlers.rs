use axum::{
    Json,
    extract::{State, Path}, 
    http::StatusCode,
};

use tracing::instrument;

use crate::data::{CreateGameRequest, CreateGameResponse, JoinGameRequest}; 
use crate::error::AppError;
use crate::data::{load_game, save_game};
use crate::game::{Game, GameId, PlayerId, GameStatus};
use crate::state::SharedState;

#[instrument(skip(state))]
pub async fn create_game_handler(
    State(state): State<SharedState>,
    Json(payload): Json<CreateGameRequest>,
) -> Result<(StatusCode, Json<CreateGameResponse>), AppError> {

    tracing::info!(host_id = ?payload.host_id, "Attempting to create game");
    
    let host_id = payload.host_id.unwrap_or_else(PlayerId::new);
    let new_game = Game::new(host_id);
    let game_id = new_game.get_id();

    save_game(&state, &new_game).await?;
    let response = CreateGameResponse { 
        game_id, 
        host_id,
    };
    
    tracing::info!(game_id = %game_id, host_id = %host_id, "Game created successfully"); 
    Ok((StatusCode::CREATED, Json(response)))
}

#[instrument(skip(state))]
pub async fn get_game_handler(
    State(state): State<SharedState>,
    Path(game_id): Path<GameId>,
) -> Result<Json<Game>, AppError> {
    let game = load_game(&state, game_id).await?;
    Ok(Json(game))
}

#[instrument(skip(state))]
pub async fn join_game_handler(
    State(state): State<SharedState>,
    Path(game_id): Path<GameId>,
    Json(payload): Json<JoinGameRequest>,
) -> Result<Json<Game>, AppError> {

    let mut game = load_game(&state, game_id).await?;

    let joining_player = payload.player_id.unwrap_or_else(PlayerId::new);
    
    match *game.get_status() {
        GameStatus::WaitingForPlayers => {
            if game.get_players().contains(&joining_player) {
                tracing::info!(game_id = %game_id, player_id = %joining_player, "Host re-joined waiting lobby.");
                Ok(())
            } else {
                game.join(joining_player).map_err(AppError::from)
            }
        },
        _ => {
            if game.get_players().contains(&joining_player) {
                 game.reconnect(joining_player).map_err(AppError::from)
            } else {
                // ALERT: Random player trying to join an active game
                tracing::warn!(game_id = %game_id, intruder = %joining_player, "Unauthorized join attempt on active game.");
                Err(AppError::Forbidden("Unauthorized".to_string()))
            }
        }
    }?;

    save_game(&state, &game).await?;

    tracing::info!(game_id = %game_id, player_id = %joining_player, "Player joined/reconnected successfully.");
    Ok(Json(game))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::state::{AppState, GameSessionManager};
    use std::sync::Arc;

    async fn setup_test_state() -> SharedState {
        // We assume Redis is running on localhost default port for tests
        let config = Config {
            server: crate::config::ServerConfig { addr: "0.0.0.0:0".to_string() },
            database: crate::config::DatabaseConfig { redis_url: "redis://127.0.0.1:6379/".to_string() },
            logging: crate::config::LoggingConfig { level: "debug".to_string() },
        };

        let client = redis::Client::open(config.database.redis_url.clone()).unwrap();    
        Arc::new(AppState {
            redis_client: client,
            session_manager: GameSessionManager::default(),
            config: Arc::new(config),
        })
    }

    #[tokio::test]
    async fn test_create_game_handler() {
        let state = setup_test_state().await;
        let payload = CreateGameRequest { host_id: None };

        let result = create_game_handler(State(state.clone()), Json(payload)).await;
        
        assert!(result.is_ok());
        let (status, Json(response)) = result.unwrap();
        assert_eq!(status, StatusCode::CREATED);
        
        // Verify in Redis
        let game_in_redis = load_game(&state, response.game_id).await;
        assert!(game_in_redis.is_ok());
    }

    #[tokio::test]
    async fn test_get_game_handler_success() {
        let state = setup_test_state().await;
        let host_id = PlayerId::new();
        let payload = CreateGameRequest { host_id: Some(host_id) };
        let (_, Json(created)) = create_game_handler(State(state.clone()), Json(payload)).await.unwrap();
        
        let result = get_game_handler(
            State(state.clone()), 
            Path(created.game_id)
        ).await;

        assert!(result.is_ok());
        let Json(game) = result.unwrap();
        
        assert_eq!(game.get_id(), created.game_id);
        assert_eq!(*game.get_status(), GameStatus::WaitingForPlayers);
    }

    #[tokio::test]
    async fn test_join_game_success() {
        let state = setup_test_state().await;     
        let host_id = PlayerId::new();
        let payload = CreateGameRequest { host_id: Some(host_id) };
        let (_, Json(created)) = create_game_handler(State(state.clone()), Json(payload)).await.unwrap();
        
        // Join with new player
        let guest_id = PlayerId::new();
        let join_payload = JoinGameRequest { player_id: Some(guest_id) };
        
        let result = join_game_handler(
            State(state.clone()), 
            Path(created.game_id), 
            Json(join_payload)
        ).await;

        assert!(result.is_ok());
        let Json(game) = result.unwrap();
        
        assert_eq!(game.get_players().len(), 2);
        assert_eq!(*game.get_status(), GameStatus::InProgress);
    }

    #[tokio::test]
    async fn test_join_full_game_fails() {
        let state = setup_test_state().await; 
        let host_id = PlayerId::new();
        let (_, Json(created)) = create_game_handler(
            State(state.clone()), 
            Json(CreateGameRequest { host_id: Some(host_id) })
        ).await.unwrap();
        
        // 2. Add Player 2 (Game becomes Full)
        let p2_id = PlayerId::new();
        let _ =join_game_handler(
            State(state.clone()), 
            Path(created.game_id), 
            Json(JoinGameRequest { player_id: Some(p2_id) })
        ).await.unwrap();

        // 3. Try to add Player 3
        let intruder_id = PlayerId::new();
        let result = join_game_handler(
            State(state.clone()), 
            Path(created.game_id), 
            Json(JoinGameRequest { player_id: Some(intruder_id) })
        ).await;

        // Expect Forbidden (because Player 3 is not part of the game)
        assert!(result.is_err());
        match result.unwrap_err() {
            AppError::Forbidden(_) => assert!(true),
            _ => assert!(false, "Expected Forbidden error"),
        }
    }

    #[tokio::test]
    async fn test_reconnect_allowed() {
        let state = setup_test_state().await;
        let host_id = PlayerId::new();
        let (_, Json(created)) = create_game_handler(
            State(state.clone()), 
            Json(CreateGameRequest { host_id: Some(host_id) })
        ).await.unwrap();
        
        // 2. Add Player 2
        let p2_id = PlayerId::new();
        let _ =join_game_handler(
            State(state.clone()), 
            Path(created.game_id), 
            Json(JoinGameRequest { player_id: Some(p2_id) })
        ).await.unwrap();
        
        // 3. Host "reconnects"
        let result = join_game_handler(
            State(state.clone()), 
            Path(created.game_id), 
            Json(JoinGameRequest { player_id: Some(host_id) })
        ).await;
        
        assert!(result.is_ok());
        let Json(game) = result.unwrap();
        assert_eq!(*game.get_status(), GameStatus::InProgress);
    }
}