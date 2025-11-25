use axum::{
    extract::{
        ws::{Message, WebSocket},
        Path, Query, State, WebSocketUpgrade,
    },
    response::IntoResponse,
};
use futures::{SinkExt, StreamExt};
use serde::Deserialize;
use tracing::instrument;

use crate::{
    data::{ClientMessage, ServerMessage},
    game::{roller::ThreadRngRoller, types::GameEvent, GameId, GameStatus, PlayerId},
    state::{GameMessage, GameSession, SharedState},
};

// ==============================================================================
// === Websocket Handlers
// =============================================================================
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

/// Orchestrates the WebSocket lifecycle: Connect -> Register -> Loop -> Disconnect
async fn handle_socket(mut socket: WebSocket, game_id: GameId, player_id: PlayerId, state: SharedState) {
    tracing::info!(game_id = %game_id, player_id = %player_id, "WebSocket connected.");

    // Verify connections
    if !validate_connection(&state, game_id, player_id).await {
        let _ = socket.close().await;
        return;
    }

    // Register Session & Notify
    let (sender_tx, mut sender_rx) = register_player_session(&state, game_id, player_id).await;

    // Send initial state
    if let Ok(game) = state.repository.load_game(game_id).await {
        let msg = GameMessage {
            r#type: "SERVER_PUSH".into(),
            payload: serde_json::to_value(ServerMessage::GameState(game)).unwrap(),
        };
        let _ = sender_tx.send(msg);
    }

    // Split Socket
    let (mut ws_sender, mut ws_receiver) = socket.split();

    // Spawn Write Task (Server -> Client)
    let send_task = tokio::spawn(async move {
        while let Some(msg) = sender_rx.recv().await {
            let json_str = serde_json::to_string(&msg.payload).unwrap_or_default();
            if ws_sender.send(Message::Text(json_str.into())).await.is_err() {
                break;
            }
        }
    });

    // Read Loop (Client -> Server)
    while let Some(Ok(msg)) = ws_receiver.next().await {
        if let Message::Text(text) = msg {
            if let Ok(client_msg) = serde_json::from_str::<ClientMessage>(&text) {
                process_client_message(client_msg, game_id, player_id, &state).await;
            }
        }
    }

    // Cleanup on Disconnect
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

        session.players.write().await.insert(player_id, sender_tx.clone());
    }

    broadcast_message(state, game_id, ServerMessage::PlayerJoined { player_id }).await;
    (sender_tx, sender_rx)
}

/// Route incoming messages to logic
async fn process_client_message(msg: ClientMessage, game_id: GameId, player_id: PlayerId, state: &SharedState) {
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
                                ServerMessage::RollResult { player_id, rolled_value: value },
                            )
                            .await
                        }
                        GameEvent::GameOver { winner_id, loser_id } => {
                            broadcast_message(state, game_id, ServerMessage::GameOver { winner_id, loser_id }).await
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
async fn send_error_to_player(state: &SharedState, game_id: GameId, player_id: PlayerId, msg: &str) {
    if let Some(session) = state.session_manager.sessions.read().await.get(&game_id) {
        if let Some(sender) = session.players.read().await.get(&player_id) {
            let _ = sender.send(GameMessage {
                r#type: "ERROR".into(),
                payload: serde_json::to_value(ServerMessage::Error { message: msg.into() }).unwrap(),
            });
        }
    }
}

#[cfg(test)]
mod ws_logic_tests {
    use std::sync::Arc;

    use axum::Json;

    use super::*;
    use crate::config::Config;
    use crate::data::{CreateGameRequest, JoinGameRequest, MockGameRepository};
    use crate::game::GameStatus;
    use crate::handlers::{create_game_handler, join_game_handler};
    use crate::state::{AppState, GameSessionManager};

    async fn setup_test_state() -> SharedState {
        let repository = Arc::new(MockGameRepository::new());
        let config = Config {
            server: crate::config::ServerConfig { addr: "0,0,0,0:0".to_string() },
            database: crate::config::DatabaseConfig { redis_url: "redis://mock".to_string() },
            logging: crate::config::LoggingConfig { level: "debug".to_string() },
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
        let (_, Json(created)) =
            create_game_handler(State(state.clone()), Json(CreateGameRequest { host_id: Some(host_id) }))
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
        let (_, Json(created)) =
            create_game_handler(State(state.clone()), Json(CreateGameRequest { host_id: Some(host_id) }))
                .await
                .unwrap();
        let guest_id = PlayerId::new();
        let _ = join_game_handler(
            State(state.clone()),
            Path(created.game_id),
            Json(JoinGameRequest { player_id: Some(guest_id) }),
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
            (ServerMessage::RollResult { player_id, rolled_value }, ServerMessage::GameState(g)) => {
                (Some((player_id, rolled_value)), Some(g))
            }
            (ServerMessage::GameState(g), ServerMessage::RollResult { player_id, rolled_value }) => {
                (Some((player_id, rolled_value)), Some(g))
            }
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
        let (_, Json(created)) =
            create_game_handler(State(state.clone()), Json(CreateGameRequest { host_id: Some(host_id) }))
                .await
                .unwrap();
        let guest_id = PlayerId::new();
        let _ = join_game_handler(
            State(state.clone()),
            Path(created.game_id),
            Json(JoinGameRequest { player_id: Some(guest_id) }),
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
