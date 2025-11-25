use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Path, Query, State,
    },
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use futures::{sink::SinkExt, stream::StreamExt};
use serde::Deserialize;
use tracing::instrument;

use crate::{
    data::ClientMessage,
    game::{roller::ThreadRngRoller, Game, GameId, GameStatus, PlayerId},
};
use crate::{
    data::{CreateGameRequest, CreateGameResponse, JoinGameRequest, ServerMessage},
    state::GameMessage,
};
use crate::{error::AppError, state::GameSession};
use crate::{game::types::GameEvent, state::SharedState};

// ==============================================================================
// === REST API Handlers
// =============================================================================

#[instrument(skip(state))]
pub async fn create_game_handler(
    State(state): State<SharedState>,
    Json(payload): Json<CreateGameRequest>,
) -> Result<(StatusCode, Json<CreateGameResponse>), AppError> {
    tracing::info!(host_id = ?payload.host_id, "Attempting to create game");

    let host_id = payload.host_id.unwrap_or_else(PlayerId::new);
    let new_game = Game::new(host_id);
    let game_id = new_game.get_id();

    state.repository.save_game(&new_game).await?;
    let response = CreateGameResponse { game_id, host_id };

    tracing::info!(game_id = %game_id, host_id = %host_id, "Game created successfully");
    Ok((StatusCode::CREATED, Json(response)))
}

#[instrument(skip(state))]
pub async fn get_game_handler(
    State(state): State<SharedState>,
    Path(game_id): Path<GameId>,
) -> Result<Json<Game>, AppError> {
    let game = state.repository.load_game(game_id).await?;
    Ok(Json(game))
}

#[instrument(skip(state))]
pub async fn join_game_handler(
    State(state): State<SharedState>,
    Path(game_id): Path<GameId>,
    Json(payload): Json<JoinGameRequest>,
) -> Result<Json<Game>, AppError> {
    let mut game = state.repository.load_game(game_id).await?;

    let joining_player = payload.player_id.unwrap_or_else(PlayerId::new);

    match *game.get_status() {
        GameStatus::WaitingForPlayers => {
            if game.get_players().contains(&joining_player) {
                tracing::info!(game_id = %game_id, player_id = %joining_player, "Host re-joined waiting lobby.");
                Ok(())
            } else {
                game.join(joining_player).map_err(AppError::from)
            }
        }
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

    state.repository.save_game(&game).await?;

    if *game.get_status() == GameStatus::InProgress {
        if game.get_players().len() == 2 {
            broadcast_message(
                &state,
                game_id,
                ServerMessage::GameStarted { game: game.clone() },
            )
            .await;
        }
    }

    tracing::info!(game_id = %game_id, player_id = %joining_player, "Player joined/reconnected successfully.");
    Ok(Json(game))
}

// ==============================================================================
// === Websocket Handlers
// =============================================================================

async fn broadcast_message(state: &SharedState, game_id: GameId, message: ServerMessage) {
    let sessions = state.session_manager.sessions.read().await;
    if let Some(session) = sessions.get(&game_id) {
        let players = session.players.read().await;
        for (pid, sender) in players.iter() {
            let internal_msg = GameMessage {
                r#type: "SERVER_PUSH".to_string(),
                payload: serde_json::to_value(&message).unwrap(),
            };
            let _ = sender.send(internal_msg);
            tracing::debug!(game_id = %game_id, to_player = %pid, "Broadcasted message");
        }
    }
}

#[derive(Deserialize, Debug)]
pub struct WebSocketParams {
    pub player_id: PlayerId,
}

#[instrument(skip(ws, state))]
pub async fn websocket_handler(
    ws: WebSocketUpgrade,
    Path(game_id): Path<GameId>,
    Query(params): Query<WebSocketParams>,
    State(state): State<SharedState>,
) -> impl IntoResponse {
    tracing::info!(game_id = %game_id, player_id = %params.player_id, "WebSocket upgrade requested.");
    ws.on_upgrade(move |socket| handle_socket(socket, game_id, params.player_id, state))
}

/// Orchestrates the WebSocket lifecycle: Connect -> Register -> Loop -> Disconnect
async fn handle_socket(
    mut socket: WebSocket,
    game_id: GameId,
    player_id: PlayerId,
    state: SharedState,
) {
    tracing::info!(game_id = %game_id, player_id = %player_id, "WebSocket connected.");

    // 1. Verify connections
    if !validate_connection(&state, game_id, player_id).await {
        let _ = socket.close().await;
        return;
    }

    // 2. Register Session & Notify
    let (sender_tx, mut sender_rx) = register_player_session(&state, game_id, player_id).await;

    // 3. Split Socket
    let (mut ws_sender, mut ws_receiver) = socket.split();

    // 4. Spawn Write Task (Server -> Client)
    let send_task = tokio::spawn(async move {
        while let Some(msg) = sender_rx.recv().await {
            let json_str = serde_json::to_string(&msg.payload).unwrap_or_default();
            if ws_sender
                .send(Message::Text(json_str.into()))
                .await
                .is_err()
            {
                break;
            }
        }
    });

    // 5. Read Loop (Client -> Server)
    while let Some(Ok(msg)) = ws_receiver.next().await {
        if let Message::Text(text) = msg {
            if let Ok(client_msg) = serde_json::from_str::<ClientMessage>(&text) {
                process_client_message(client_msg, game_id, player_id, &state).await;
            }
        }
    }

    // 6. Cleanup on Disconnect
    handle_disconnect(&state, game_id, player_id).await;
    send_task.abort();
}

/// Verify player is in the game stored in Redis
async fn validate_connection(state: &SharedState, game_id: GameId, player_id: PlayerId) -> bool {
    let game_check = state.repository.load_game(game_id).await;
    if let Err(e) = game_check {
        tracing::warn!(game_id = %game_id, player_id = %player_id, error = ?e, "Connection rejected: Game load failed.");
        return false;
    }
    let game = game_check.unwrap();
    if !game.get_players().contains(&player_id) {
        tracing::warn!(game_id = %game_id, player_id = %player_id, "Connection rejected: Player not in game.");
        return false;
    }
    true
}

/// Add player to SessionManager and return their message receiver
async fn register_player_session(
    state: &SharedState,
    game_id: GameId,
    player_id: PlayerId,
) -> (
    tokio::sync::mpsc::UnboundedSender<GameMessage>,
    tokio::sync::mpsc::UnboundedReceiver<GameMessage>,
) {
    let (sender_tx, sender_rx) = tokio::sync::mpsc::unbounded_channel::<GameMessage>();

    {
        let mut sessions = state.session_manager.sessions.write().await;
        let session = sessions
            .entry(game_id)
            .or_insert_with(|| std::sync::Arc::new(GameSession::default()));
        session
            .players
            .write()
            .await
            .insert(player_id, sender_tx.clone());
    }

    broadcast_message(state, game_id, ServerMessage::PlayerJoined { player_id }).await;
    (sender_tx, sender_rx)
}

/// Route incoming messages to logic
async fn process_client_message(
    msg: ClientMessage,
    game_id: GameId,
    player_id: PlayerId,
    state: &SharedState,
) {
    tracing::debug!(game_id = %game_id, player_id = %player_id, "Received message: {:#?}", msg);
    match msg {
        ClientMessage::Connect { .. } => {} // No-op
        ClientMessage::Roll => handle_roll_command(game_id, player_id, state).await,
    }
}

/// Execute the ROLL command logic
async fn handle_roll_command(game_id: GameId, player_id: PlayerId, state: &SharedState) {
    if let Ok(mut game) = state.repository.load_game(game_id).await {
        let mut roller = ThreadRngRoller::new();

        // Roll
        match game.roll(player_id, &mut roller) {
            Ok(events) => {
                // Save
                if let Err(e) = state.repository.save_game(&game).await {
                    tracing::error!("Failed to save game state: {}", e);
                    return;
                }

                for event in events {
                    match event {
                        GameEvent::Rolled { player_id, value } => {
                            broadcast_message(
                                state,
                                game_id,
                                ServerMessage::RollResult {
                                    player_id,
                                    rolled_value: value,
                                },
                            )
                            .await
                        }
                        GameEvent::GameOver {
                            winner_id,
                            loser_id,
                        } => {
                            broadcast_message(
                                state,
                                game_id,
                                ServerMessage::GameOver {
                                    winner_id,
                                    loser_id,
                                },
                            )
                            .await
                        }
                    }
                }
                broadcast_message(state, game_id, ServerMessage::GameState(game)).await;
            }
            Err(e) => {
                send_error_to_player(state, game_id, player_id, &e.to_string()).await;
            }
        }
    }
}

/// Cleanup when socket closes
async fn handle_disconnect(state: &SharedState, game_id: GameId, player_id: PlayerId) {
    tracing::info!(game_id = %game_id, player_id = %player_id, "WebSocket disconnected.");

    // Remove from session
    {
        let sessions = state.session_manager.sessions.read().await;
        if let Some(session) = sessions.get(&game_id) {
            session.players.write().await.remove(&player_id);
        }
    }

    // Update Redis state to Paused
    if let Ok(mut game) = state.repository.load_game(game_id).await {
        if *game.get_status() == GameStatus::InProgress {
            let _ = game.pause_game(player_id);
            let _ = state.repository.save_game(&game).await;
            broadcast_message(state, game_id, ServerMessage::GameState(game)).await;
        }
    }
}

/// Send an error message to a specific player
async fn send_error_to_player(
    state: &SharedState,
    game_id: GameId,
    player_id: PlayerId,
    msg: &str,
) {
    if let Some(session) = state.session_manager.sessions.read().await.get(&game_id) {
        if let Some(sender) = session.players.read().await.get(&player_id) {
            let _ = sender.send(GameMessage {
                r#type: "ERROR".into(),
                payload: serde_json::to_value(ServerMessage::Error {
                    message: msg.into(),
                })
                .unwrap(),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::data::MockGameRepository;
    use crate::state::{AppState, GameSessionManager};
    use axum::http::StatusCode;
    use std::sync::Arc;

    async fn setup_test_state() -> SharedState {
        let repository = Arc::new(MockGameRepository::new());
        let config = Config {
            server: crate::config::ServerConfig {
                addr: "0,0,0,0:0".to_string(),
            },
            database: crate::config::DatabaseConfig {
                redis_url: "redis://mock".to_string(),
            },
            logging: crate::config::LoggingConfig {
                level: "debug".to_string(),
            },
        };

        Arc::new(AppState {
            repository,
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
        let game_in_redis = state.repository.load_game(response.game_id).await;
        assert!(game_in_redis.is_ok());
    }

    #[tokio::test]
    async fn test_get_game_handler_success() {
        let state = setup_test_state().await;
        let host_id = PlayerId::new();
        let payload = CreateGameRequest {
            host_id: Some(host_id),
        };
        let (_, Json(created)) = create_game_handler(State(state.clone()), Json(payload))
            .await
            .unwrap();

        let result = get_game_handler(State(state.clone()), Path(created.game_id)).await;

        assert!(result.is_ok());
        let Json(game) = result.unwrap();

        assert_eq!(game.get_id(), created.game_id);
        assert_eq!(*game.get_status(), GameStatus::WaitingForPlayers);
    }

    #[tokio::test]
    async fn test_join_game_success() {
        let state = setup_test_state().await;
        let host_id = PlayerId::new();
        let payload = CreateGameRequest {
            host_id: Some(host_id),
        };
        let (_, Json(created)) = create_game_handler(State(state.clone()), Json(payload))
            .await
            .unwrap();

        // Join with new player
        let guest_id = PlayerId::new();
        let join_payload = JoinGameRequest {
            player_id: Some(guest_id),
        };

        let result = join_game_handler(
            State(state.clone()),
            Path(created.game_id),
            Json(join_payload),
        )
        .await;

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
            Json(CreateGameRequest {
                host_id: Some(host_id),
            }),
        )
        .await
        .unwrap();

        // Add Player 2 (Game becomes Full)
        let p2_id = PlayerId::new();
        let _ = join_game_handler(
            State(state.clone()),
            Path(created.game_id),
            Json(JoinGameRequest {
                player_id: Some(p2_id),
            }),
        )
        .await
        .unwrap();

        // Try to add Player 3
        let intruder_id = PlayerId::new();
        let result = join_game_handler(
            State(state.clone()),
            Path(created.game_id),
            Json(JoinGameRequest {
                player_id: Some(intruder_id),
            }),
        )
        .await;

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

        // Setup Game
        let host_id = PlayerId::new();
        let (_, Json(created)) = create_game_handler(
            State(state.clone()),
            Json(CreateGameRequest {
                host_id: Some(host_id),
            }),
        )
        .await
        .unwrap();
        let guest_id = PlayerId::new();
        let _ = join_game_handler(
            State(state.clone()),
            Path(created.game_id),
            Json(JoinGameRequest {
                player_id: Some(guest_id),
            }),
        )
        .await
        .unwrap();

        // Register Guest (so we can check they receive the pause notification)
        let (_, mut guest_rx) = register_player_session(&state, created.game_id, guest_id).await;
        let _ = guest_rx.recv().await; // Drain initial

        // Host Disconnects
        handle_disconnect(&state, created.game_id, host_id).await;

        // Verify State Update in Redis
        let game = state.repository.load_game(created.game_id).await.unwrap();
        match *game.get_status() {
            GameStatus::PausedForReconnect(pid) => assert_eq!(pid, host_id),
            _ => panic!("Game should be paused"),
        }

        // Verify Broadcast to Guest
        let msg = guest_rx
            .recv()
            .await
            .expect("Guest missed pause notification");
        let server_msg: ServerMessage = serde_json::from_value(msg.payload).unwrap();
        if let ServerMessage::GameState(g) = server_msg {
            match g.get_status() {
                GameStatus::PausedForReconnect(pid) => assert_eq!(*pid, host_id),
                _ => panic!("Broadcasted game state should be paused"),
            }
        } else {
            panic!("Expected GameState broadcast");
        }
    }
}

#[cfg(test)]
mod ws_logic_tests {
    use std::sync::Arc;

    use super::*;
    use crate::config::Config;
    use crate::data::MockGameRepository;
    use crate::game::GameStatus;
    use crate::state::{AppState, GameSessionManager};

    async fn setup_test_state() -> SharedState {
        let repository = Arc::new(MockGameRepository::new());
        let config = Config {
            server: crate::config::ServerConfig {
                addr: "0,0,0,0:0".to_string(),
            },
            database: crate::config::DatabaseConfig {
                redis_url: "redis://mock".to_string(),
            },
            logging: crate::config::LoggingConfig {
                level: "debug".to_string(),
            },
        };

        Arc::new(AppState {
            repository,
            session_manager: GameSessionManager::default(),
            config: Arc::new(config),
        })
    }

    #[tokio::test]
    async fn test_validate_connection() {
        let state = setup_test_state().await;
        let host_id = PlayerId::new();
        let (_, Json(created)) = create_game_handler(
            State(state.clone()),
            Json(CreateGameRequest {
                host_id: Some(host_id),
            }),
        )
        .await
        .unwrap();

        assert!(validate_connection(&state, created.game_id, host_id).await);
        let random_id = PlayerId::new();
        assert!(!validate_connection(&state, created.game_id, random_id).await);
    }

    #[tokio::test]
    async fn test_register_player_session() {
        let state = setup_test_state().await;
        let game_id = GameId::new();
        let player_id = PlayerId::new();
        let (tx, _rx) = register_player_session(&state, game_id, player_id).await;
        let sessions = state.session_manager.sessions.read().await;
        assert!(sessions.contains_key(&game_id));
        assert!(tx
            .send(GameMessage {
                r#type: "TEST".into(),
                payload: serde_json::Value::Null
            })
            .is_ok());
    }

    #[tokio::test]
    async fn test_handle_roll_command_flow() {
        let state = setup_test_state().await;
        let host_id = PlayerId::new();
        let (_, Json(created)) = create_game_handler(
            State(state.clone()),
            Json(CreateGameRequest {
                host_id: Some(host_id),
            }),
        )
        .await
        .unwrap();
        let guest_id = PlayerId::new();
        let _ = join_game_handler(
            State(state.clone()),
            Path(created.game_id),
            Json(JoinGameRequest {
                player_id: Some(guest_id),
            }),
        )
        .await
        .unwrap();

        let (_, mut host_rx) = register_player_session(&state, created.game_id, host_id).await;
        let (_, mut guest_rx) = register_player_session(&state, created.game_id, guest_id).await;

        let _ = host_rx.recv().await;
        let _ = guest_rx.recv().await;
        let _ = host_rx.recv().await;

        handle_roll_command(created.game_id, host_id, &state).await;

        let msg1 = host_rx.recv().await.expect("Host missed message 1");
        let msg2 = host_rx.recv().await.expect("Host missed message 2");

        let server_msg1: ServerMessage =
            serde_json::from_value(msg1.payload.clone()).expect("Failed to deserialize msg1");
        let server_msg2: ServerMessage =
            serde_json::from_value(msg2.payload.clone()).expect("Failed to deserialize msg2");

        let (roll_result, game_state) = match (server_msg1.clone(), server_msg2.clone()) {
            (
                ServerMessage::RollResult {
                    player_id,
                    rolled_value,
                },
                ServerMessage::GameState(g),
            ) => (Some((player_id, rolled_value)), Some(g)),
            (
                ServerMessage::GameState(g),
                ServerMessage::RollResult {
                    player_id,
                    rolled_value,
                },
            ) => (Some((player_id, rolled_value)), Some(g)),
            _ => (None, None),
        };

        assert!(
            roll_result.is_some(),
            "Host did not receive RollResult. Received: {:?} and {:?}",
            server_msg1,
            server_msg2
        );
        assert!(game_state.is_some(), "Host did not receive GameState");

        let (r_pid, r_val) = roll_result.unwrap();
        assert_eq!(r_pid, host_id);
        assert!(r_val > 0 && r_val <= 1000);

        let _ = guest_rx.recv().await.expect("Guest missed message 1");
        let _ = guest_rx.recv().await.expect("Guest missed message 2");
    }

    #[tokio::test]
    async fn test_handle_disconnect_pauses_game() {
        let state = setup_test_state().await;

        // 1. Setup Game
        let host_id = PlayerId::new();
        let (_, Json(created)) = create_game_handler(
            State(state.clone()),
            Json(CreateGameRequest {
                host_id: Some(host_id),
            }),
        )
        .await
        .unwrap();
        let guest_id = PlayerId::new();
        let _ = join_game_handler(
            State(state.clone()),
            Path(created.game_id),
            Json(JoinGameRequest {
                player_id: Some(guest_id),
            }),
        )
        .await
        .unwrap();

        // Register Guest
        let (_, mut guest_rx) = register_player_session(&state, created.game_id, guest_id).await;
        let _ = guest_rx.recv().await;

        // Host Disconnects
        handle_disconnect(&state, created.game_id, host_id).await;

        // Verify State Update in Redis
        let game = state.repository.load_game(created.game_id).await.unwrap();
        match *game.get_status() {
            GameStatus::PausedForReconnect(pid) => assert_eq!(pid, host_id),
            _ => panic!("Game should be paused"),
        }

        // Verify Broadcast to Guest
        // Guest should receive GameState with status Paused
        let msg = guest_rx.recv().await.expect("Guest missed message");
        let server_msg: ServerMessage = serde_json::from_value(msg.payload).unwrap();

        // If first msg is PlayerJoined, ignore and get next
        let final_msg = if let ServerMessage::PlayerJoined { .. } = server_msg {
            let msg2 = guest_rx.recv().await.expect("Guest missed second message");
            serde_json::from_value(msg2.payload).unwrap()
        } else {
            server_msg
        };

        if let ServerMessage::GameState(g) = final_msg {
            match g.get_status() {
                GameStatus::PausedForReconnect(pid) => assert_eq!(*pid, host_id),
                _ => panic!("Broadcasted game state should be paused"),
            }
        } else {
            panic!("Expected GameState broadcast, got {:?}", final_msg);
        }
    }
}
