use critical_one::game::{GameId, PlayerId};
use futures::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};

// --- Re-definitions of DTOs ---
// In a bigger repo, we would put these in a shared `common` library crate.
// For now, we just redefine the ones we need for the client.

#[derive(Debug, Deserialize)]
struct CreateGameResponse {
    game_id: GameId,
}

#[derive(Debug, Serialize)]
struct CreateGameRequest {
    host_id: Option<PlayerId>,
}

#[derive(Debug, Serialize)]
struct JoinGameRequest {
    player_id: Option<PlayerId>,
}

#[derive(Debug, Serialize)]
#[serde(tag = "type", content = "payload", rename_all = "SCREAMING_SNAKE_CASE")]
enum ClientMessage {
    Roll,
}

async fn spawn_game_connection(
    game_id: GameId,
    player_id: PlayerId,
    name: String,
) -> Result<tokio::task::JoinHandle<()>, Box<dyn std::error::Error>> {
    let ws_base = "ws://127.0.0.1:3000/ws/game";

    let handle = tokio::spawn(async move {
        let url_str = format!("{}/{}?player_id={}", ws_base, game_id, player_id);
        let (ws_stream, _) = connect_async(url_str).await.expect("failed to connect");
        let (mut write, mut read) = ws_stream.split();

        println!("....[{name}] Connected!");

        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
        println!("....[{name}] Rolling dice...");
        let msg = serde_json::to_string(&ClientMessage::Roll).unwrap();
        write
            .send(Message::Text(msg.into()))
            .await
            .expect("failed to send roll");

        while let Some(msg) = read.next().await {
            let msg = msg.expect("Error reading message");
            if msg.is_text() {
                println!("....[{name} RX] {}", msg.to_text().unwrap());
            }
        }
    });

    Ok(handle)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. Setup IDs
    let host_id = PlayerId::new();
    let guest_id = PlayerId::new();
    let client = reqwest::Client::new();
    let base_url = "http://127.0.0.1:3000";

    println!("--- 🎲 CRITICAL ONE TEST CLIENT ---");
    println!("Host ID:  {}", host_id);
    println!("Guest ID: {}", guest_id);

    println!("\n[1] Creating Game...");
    let resp = client
        .post(format!("{}/game", base_url))
        .json(&CreateGameRequest {
            host_id: Some(host_id),
        })
        .send()
        .await?
        .json::<CreateGameResponse>()
        .await?;

    let game_id = resp.game_id;
    println!("Success! Game ID: {}", game_id);

    println!("\n[2] Guest Joining...");
    let _ = client
        .post(format!("{}/game/{}/join", base_url, game_id))
        .json(&JoinGameRequest {
            player_id: Some(guest_id),
        })
        .send()
        .await?;
    println!("Success! Guest joined.");
    println!("\n[3] Connecting WebSockets...");

    let host_handle = spawn_game_connection(game_id, host_id, "Host".to_string()).await?;
    let guest_handle = spawn_game_connection(game_id, guest_id, "Guest".to_string()).await?;

    let _ = tokio::join!(host_handle, guest_handle);

    Ok(())
}
